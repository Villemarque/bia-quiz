use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

use crate::auth::ApiUser;
use crate::{db, AppState};

pub async fn healthz(State(state): State<AppState>) -> Response {
    let check = state
        .db
        .call(|conn| {
            use diesel::prelude::*;
            Ok(
                diesel::select(diesel::dsl::sql::<diesel::sql_types::Integer>("1"))
                    .get_result::<i32>(conn)?,
            )
        })
        .await;
    match check {
        Ok(_) => (StatusCode::OK, "ok").into_response(),
        Err(err) => (StatusCode::SERVICE_UNAVAILABLE, err.to_string()).into_response(),
    }
}

pub async fn meta(State(state): State<AppState>) -> Response {
    let commit = env!("BIA_QUIZ_COMMIT");
    let source_url = &state.cfg.server.source_url;
    let commit_url = if commit != "unknown" && !source_url.is_empty() {
        Some(format!(
            "{}/commit/{}",
            source_url.trim_end_matches('/'),
            commit
        ))
    } else {
        None
    };
    Json(serde_json::json!({
        "source_url": source_url,
        "join_code_length": state.cfg.server.join_code_length,
        "version": env!("CARGO_PKG_VERSION"),
        "commit": commit,
        "commit_url": commit_url,
    }))
    .into_response()
}

pub async fn chapters(State(state): State<AppState>) -> Response {
    match db::chapters(&state.db).await {
        Ok(rows) => Json(rows).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

pub async fn me(user: ApiUser) -> Response {
    Json(serde_json::json!({
        "username": user.0.username,
        "role": user.0.role,
    }))
    .into_response()
}

pub async fn prof_sessions(_user: ApiUser, State(state): State<AppState>) -> Response {
    match db::list_sessions(&state.db).await {
        Ok(rows) => Json(rows).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Serialize)]
struct SessionDetail {
    session: bia_core::models::Session,
    questions: Vec<db::SessionQuestionRow>,
    answers: Vec<db::SessionAnswerRow>,
}

pub async fn prof_session(
    _user: ApiUser,
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Response {
    let session = match db::session(&state.db, id).await {
        Ok(Some(session)) => session,
        Ok(None) => return (StatusCode::NOT_FOUND, "Session introuvable.").into_response(),
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    };
    let questions = match db::session_questions(&state.db, id).await {
        Ok(rows) => rows,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    };
    let answers = match db::session_answers(&state.db, id).await {
        Ok(rows) => rows,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    };
    Json(SessionDetail {
        session,
        questions,
        answers,
    })
    .into_response()
}

pub async fn prof_session_csv(
    _user: ApiUser,
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Response {
    let answers = match db::session_answers(&state.db, id).await {
        Ok(rows) => rows,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    };

    let mut writer = csv::WriterBuilder::new()
        .delimiter(b';')
        .from_writer(Vec::new());
    if writer
        .write_record([
            "nickname",
            "position",
            "choice_original_index",
            "is_correct",
            "points",
        ])
        .is_err()
    {
        return (StatusCode::INTERNAL_SERVER_ERROR, "CSV").into_response();
    }
    for row in &answers {
        let _ = writer.write_record([
            csv_safe(&row.nickname),
            row.position.to_string(),
            row.choice_idx.to_string(),
            row.is_correct.to_string(),
            row.points.to_string(),
        ]);
    }
    let Ok(bytes) = writer.into_inner() else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "CSV").into_response();
    };

    let mut response = Response::new(Body::from(bytes));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    let disposition = format!("attachment; filename=\"session-{id}.csv\"");
    if let Ok(value) = HeaderValue::from_str(&disposition) {
        response
            .headers_mut()
            .insert(header::CONTENT_DISPOSITION, value);
    }
    response
}

/// Neutralise spreadsheet formulas in student-controlled fields.
fn csv_safe(value: &str) -> String {
    match value.chars().next() {
        Some('=' | '+' | '-' | '@' | '\t' | '\r') => format!("'{value}"),
        _ => value.to_string(),
    }
}

pub async fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "Route inconnue.").into_response()
}

#[cfg(test)]
mod tests {
    use super::csv_safe;

    #[test]
    fn csv_formulas_are_neutralised() {
        assert_eq!(csv_safe("=1+1"), "'=1+1");
        assert_eq!(csv_safe("@SUM(A1)"), "'@SUM(A1)");
        assert_eq!(csv_safe("Max"), "Max");
    }
}
