//! End-to-end: a teacher and a student play one question over real WebSockets.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use bia_server::auth::RateLimiter;
use bia_server::config::Config;
use bia_server::db::Db;
use bia_server::{router, AppState};
use diesel::prelude::*;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn seed(db: &Db) {
    db.call(|conn| {
        diesel::sql_query("INSERT INTO chapters (number, title) VALUES (0, '1.1 Les aéronefs')")
            .execute(conn)?;
        diesel::sql_query(
            "INSERT INTO questions \
             (qid, year, subject, text, used_fixed, choice_a, choice_b, choice_c, choice_d, \
              answer_idx, mixed_choices, attachment_link, seconds) \
             VALUES ('Q1', 2024, 2, 'Quel est le bon choix ?', 0, \
                     'Un', 'Deux', 'Trois', 'Quatre', 0, 0, NULL, 2)",
        )
        .execute(conn)?;
        diesel::sql_query(
            "INSERT INTO question_chapters (question_qid, chapter_id) VALUES ('Q1', 1)",
        )
        .execute(conn)?;
        Ok(())
    })
    .await
    .expect("seed");
}

async fn next_json(ws: &mut Ws, label: &str) -> serde_json::Value {
    loop {
        let frame = match timeout(Duration::from_secs(10), ws.next()).await {
            Ok(Some(Ok(frame))) => frame,
            Ok(Some(Err(err))) => panic!("websocket error while waiting for {label}: {err}"),
            Ok(None) => panic!("socket closed while waiting for {label}"),
            Err(_) => panic!("timeout waiting for {label}"),
        };
        if let Message::Text(text) = frame {
            return serde_json::from_str(text.as_str()).expect("valid JSON");
        }
    }
}

async fn wait_for(ws: &mut Ws, kind: &str) -> serde_json::Value {
    for _ in 0..40 {
        let value = next_json(ws, kind).await;
        if value["type"] == "error" {
            panic!(
                "server error while waiting for {kind}: {}",
                value["message"]
            );
        }
        if value["type"] == kind {
            return value;
        }
    }
    panic!("did not receive {kind}");
}

async fn send(ws: &mut Ws, value: serde_json::Value) {
    ws.send(Message::Text(value.to_string().into()))
        .await
        .expect("send");
}

#[tokio::test]
async fn full_question_flow() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("test.sqlite3")).unwrap();
    seed(&db).await;

    let mut config = Config::default();
    config.server.public_base_url = "http://test.local".into();
    let state = AppState {
        cfg: Arc::new(config),
        db: db.clone(),
        room: Arc::new(Mutex::new(None)),
        limiter: Arc::new(RateLimiter::new(0)),
        login_limiter: Arc::new(bia_server::auth::LoginLimiter::default()),
        figures: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
        import: Arc::new(std::sync::Mutex::new(
            bia_server::admin::ImportJob::default(),
        )),
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = router(state);
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });

    let base = format!("ws://{addr}/ws");
    let mut request = format!("{base}?role=teacher")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("origin", HeaderValue::from_static("http://test.local"));
    let (mut teacher, _) = connect_async(request).await.unwrap();
    assert_eq!(next_json(&mut teacher, "hello").await["type"], "hello");

    send(
        &mut teacher,
        serde_json::json!({
            "type": "teacher.create",
            "chapters": [{ "subject": 2, "chapter": 0, "count": 1 }],
            "seconds": 5
        }),
    )
    .await;

    let created = wait_for(&mut teacher, "room.created").await;
    let code = created["code"].as_str().unwrap().to_string();
    assert_eq!(created["available"], 1);
    assert!(created["warning"].is_null());

    let (mut student, _) = connect_async(format!("{base}?role=student&room={code}"))
        .await
        .unwrap();
    assert_eq!(next_json(&mut student, "hello").await["type"], "hello");
    send(
        &mut student,
        serde_json::json!({ "type": "student.join", "nickname": "Max" }),
    )
    .await;
    let welcome = wait_for(&mut student, "player.welcome").await;
    assert!(welcome["token"].as_str().unwrap().len() >= 16);

    send(
        &mut teacher,
        serde_json::json!({ "type": "teacher.start_question" }),
    )
    .await;

    let question = wait_for(&mut student, "question.start").await;
    assert_eq!(question["text"], "Quel est le bon choix ?");
    assert_eq!(question["total"], 1);
    assert_eq!(question["choices"][0]["text"], "Un");
    assert_eq!(
        question["seconds"], 2,
        "the per-question value wins over the 5 s fallback"
    );
    let teacher_question = wait_for(&mut teacher, "question.start").await;
    assert_eq!(teacher_question["choices"][0]["text"], "Un");

    send(
        &mut student,
        serde_json::json!({ "type": "student.answer", "choice": 0 }),
    )
    .await;

    let saved = wait_for(&mut student, "answer.saved").await;
    assert_eq!(saved["choice"], 0);

    // The room also sends an initial 0/total at question start; wait for the real one.
    let mut count = wait_for(&mut teacher, "answers.count").await;
    if count["count"] != 1 {
        count = wait_for(&mut teacher, "answers.count").await;
    }
    assert_eq!(count["count"], 1);

    let reveal = wait_for(&mut teacher, "question.reveal").await;
    assert_eq!(reveal["correct_index"], 0);
    assert_eq!(reveal["counts"][0], 1);

    let result = wait_for(&mut student, "question.result").await;
    assert_eq!(result["correct"], true);

    send(
        &mut teacher,
        serde_json::json!({ "type": "teacher.show_leaderboard" }),
    )
    .await;
    let board = wait_for(&mut teacher, "leaderboard").await;
    assert_eq!(board["top"][0]["nickname"], "Max");

    // The points were written to SQLite.
    let points: i32 = db
        .call(|conn| {
            Ok(
                diesel::select(diesel::dsl::sql::<diesel::sql_types::Integer>(
                    "COALESCE(MAX(points), -1) FROM answers",
                ))
                .get_result::<i32>(conn)?,
            )
        })
        .await
        .unwrap();
    assert!(
        points > 500,
        "fast correct answer should score high, got {points}"
    );
}
