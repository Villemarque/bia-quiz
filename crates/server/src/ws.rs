//! WebSocket entry point. One socket per client, JSON messages.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::Response;
use std::collections::BTreeMap;

use bia_core::protocol::{ChapterPick, ClientMsg, ServerMsg};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::{mpsc, oneshot};

use crate::db;
use crate::room::{self, RoomHandle, TeacherCmd};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    role: Option<String>,
    room: Option<String>,
    token: Option<String>,
}

pub async fn ws(
    upgrade: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(query): Query<WsQuery>,
) -> Response {
    upgrade.on_upgrade(move |socket| handle(socket, state, query))
}

async fn handle(socket: WebSocket, state: AppState, query: WsQuery) {
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<ServerMsg>();

    let writer = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            let Ok(text) = serde_json::to_string(&message) else {
                continue;
            };
            if sink.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    match query.role.as_deref() {
        Some("teacher") => teacher_loop(&state, query.room.as_deref(), &mut stream, &tx).await,
        Some("student") => {
            student_loop(
                &state,
                query.room.as_deref(),
                query.token.as_deref(),
                &mut stream,
                &tx,
            )
            .await
        }
        _ => {
            let _ = tx.send(ServerMsg::Error {
                message: "Rôle invalide.".into(),
            });
        }
    }

    // The room keeps a clone of `tx`, so the writer must be stopped explicitly.
    drop(tx);
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    writer.abort();
}

fn parse(text: &str) -> Result<ClientMsg, ServerMsg> {
    serde_json::from_str(text).map_err(|_| ServerMsg::Error {
        message: "Message invalide.".into(),
    })
}

fn teacher_command(message: ClientMsg) -> Option<TeacherCmd> {
    match message {
        ClientMsg::TeacherStartQuestion => Some(TeacherCmd::StartQuestion),
        ClientMsg::TeacherForceReveal => Some(TeacherCmd::ForceReveal),
        ClientMsg::TeacherNext => Some(TeacherCmd::Next),
        ClientMsg::TeacherPause => Some(TeacherCmd::Pause),
        ClientMsg::TeacherResume => Some(TeacherCmd::Resume),
        ClientMsg::TeacherSkip => Some(TeacherCmd::Skip),
        ClientMsg::TeacherEnd => Some(TeacherCmd::End),
        _ => None,
    }
}

fn new_code() -> String {
    use rand::Rng;
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::thread_rng();
    (0..6)
        .map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char)
        .collect()
}

async fn active_room(state: &AppState, code: Option<&str>) -> Option<RoomHandle> {
    let guard = state.room.lock().await;
    guard
        .as_ref()
        .filter(|room| room.is_alive() && code.map(|c| c == room.code).unwrap_or(true))
        .cloned()
}

async fn teacher_loop(
    state: &AppState,
    room_code: Option<&str>,
    stream: &mut futures_util::stream::SplitStream<WebSocket>,
    tx: &mpsc::UnboundedSender<ServerMsg>,
) {
    let _ = tx.send(ServerMsg::Hello {
        role: "teacher".into(),
        server_now: room::now_ms(),
    });

    let mut handle: Option<RoomHandle> = active_room(state, room_code).await;
    if let Some(room) = &handle {
        room.attach_teacher(tx.clone());
    }

    while let Some(Ok(message)) = stream.next().await {
        let Message::Text(text) = message else {
            if matches!(message, Message::Close(_)) {
                break;
            }
            continue;
        };
        let client = match parse(&text) {
            Ok(c) => c,
            Err(error) => {
                let _ = tx.send(error);
                continue;
            }
        };

        if let ClientMsg::TeacherCreate { chapters, seconds } = client {
            if handle.is_some() {
                let _ = tx.send(ServerMsg::Error {
                    message: "Une partie est déjà en cours.".into(),
                });
                continue;
            }
            match create_room(state, tx, chapters, seconds).await {
                Ok(room) => handle = Some(room),
                Err(message) => {
                    let _ = tx.send(ServerMsg::Error { message });
                }
            }
            continue;
        }

        let Some(command) = teacher_command(client) else {
            let _ = tx.send(ServerMsg::Error {
                message: "Message professeur inconnu.".into(),
            });
            continue;
        };
        let Some(room) = &handle else {
            let _ = tx.send(ServerMsg::Error {
                message: "Aucune partie en cours.".into(),
            });
            continue;
        };
        room.teacher(command);
    }
}

async fn create_room(
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerMsg>,
    chapters: Vec<ChapterPick>,
    seconds: u32,
) -> Result<RoomHandle, String> {
    let mut merged: BTreeMap<i32, u32> = BTreeMap::new();
    for pick in chapters {
        if pick.count == 0 {
            continue;
        }
        if pick.count > 50 {
            return Err("Le nombre de questions par chapitre doit être entre 1 et 50.".into());
        }
        let entry = merged.entry(pick.number).or_default();
        *entry = entry.saturating_add(pick.count);
    }
    if merged.is_empty() {
        return Err("Choisissez au moins un chapitre.".into());
    }
    let requested: u64 = merged.values().map(|count| u64::from(*count)).sum();
    if requested == 0 || requested > 50 {
        return Err("Le nombre total de questions doit être entre 1 et 50.".into());
    }
    if !(5..=120).contains(&seconds) {
        return Err("La durée doit être entre 5 et 120 secondes.".into());
    }

    // One room at a time: hold the lock across creation so two teachers cannot race.
    let mut guard = state.room.lock().await;
    if let Some(existing) = guard.as_ref() {
        if existing.is_alive() {
            return Err("Une partie est déjà en cours sur ce serveur.".into());
        }
    }

    let picks: Vec<(i32, u32)> = merged.into_iter().collect();
    let draw_picks: Vec<(i32, usize)> = picks
        .iter()
        .map(|(number, count)| (*number, *count as usize))
        .collect();
    let (questions, available) = db::draw(&state.db, &draw_picks)
        .await
        .map_err(|e| e.to_string())?;
    let chapter_numbers: Vec<i32> = picks.iter().map(|(number, _)| *number).collect();
    let chapter_ids = db::chapter_ids(&state.db, &chapter_numbers)
        .await
        .map_err(|e| e.to_string())?;
    let code = new_code();
    let session_id = db::create_session(
        &state.db,
        db::NewSessionData {
            code: &code,
            started_at: room::now_ms(),
            question_count: questions.len() as i32,
            seconds: seconds as i32,
            chapter_ids: &chapter_ids,
        },
    )
    .await
    .map_err(|e| e.to_string())?;

    let join_url = state.cfg.join_url(&code);
    let warning = if questions.len() < requested as usize {
        Some(format!(
            "Seules {} questions ont pu être tirées sur {} demandées ({} disponibles pour ces chapitres).",
            questions.len(),
            requested,
            available
        ))
    } else {
        None
    };

    let room = room::spawn(
        room::RoomSpec {
            db: state.db.clone(),
            session_id,
            code: code.clone(),
            seconds,
            max_players: state.cfg.server.max_players,
            questions,
        },
        tx.clone(),
    );
    room.attach_teacher(tx.clone());
    *guard = Some(room.clone());
    drop(guard);

    let _ = tx.send(ServerMsg::RoomCreated {
        code,
        join_url,
        available,
        requested: requested as usize,
        warning,
    });
    Ok(room)
}

async fn student_loop(
    state: &AppState,
    room_code: Option<&str>,
    token: Option<&str>,
    stream: &mut futures_util::stream::SplitStream<WebSocket>,
    tx: &mpsc::UnboundedSender<ServerMsg>,
) {
    let _ = tx.send(ServerMsg::Hello {
        role: "student".into(),
        server_now: room::now_ms(),
    });

    let Some(code) = room_code else {
        let _ = tx.send(ServerMsg::Error {
            message: "Code de partie manquant.".into(),
        });
        return;
    };
    let Some(room) = active_room(state, Some(code)).await else {
        let _ = tx.send(ServerMsg::Error {
            message: "Aucune partie avec ce code.".into(),
        });
        return;
    };

    let mut player_id: Option<i32> = None;

    while let Some(Ok(message)) = stream.next().await {
        let Message::Text(text) = message else {
            if matches!(message, Message::Close(_)) {
                break;
            }
            continue;
        };
        let client = match parse(&text) {
            Ok(c) => c,
            Err(error) => {
                let _ = tx.send(error);
                continue;
            }
        };

        match client {
            ClientMsg::StudentJoin { nickname } => {
                if player_id.is_some() {
                    let _ = tx.send(ServerMsg::Error {
                        message: "Vous avez déjà rejoint la partie.".into(),
                    });
                    continue;
                }
                let reply: oneshot::Receiver<Result<i32, String>> =
                    room.join_student(tx.clone(), token.map(str::to_string), nickname);
                match reply.await {
                    Ok(Ok(id)) => player_id = Some(id),
                    Ok(Err(message)) => {
                        let _ = tx.send(ServerMsg::Error { message });
                        break;
                    }
                    Err(_) => {
                        let _ = tx.send(ServerMsg::Error {
                            message: "La partie ne répond plus.".into(),
                        });
                        break;
                    }
                }
            }
            ClientMsg::StudentAnswer { choice } => match player_id {
                Some(id) => room.answer(id, choice),
                None => {
                    let _ = tx.send(ServerMsg::Error {
                        message: "Rejoignez la partie avant de répondre.".into(),
                    });
                }
            },
            _ => {
                let _ = tx.send(ServerMsg::Error {
                    message: "Message élève inconnu.".into(),
                });
            }
        }
    }

    if let Some(id) = player_id {
        room.detach_student(id);
    }
}
