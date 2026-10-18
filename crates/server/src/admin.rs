//! `/admin`: import job, report and stats.

use std::fs;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use bia_importer::Args as ImportArgs;
use diesel::prelude::*;
use serde::Serialize;

use crate::auth::AdminUser;
use crate::{db, AppState};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    #[default]
    Idle,
    Downloading,
    Importing,
    Done,
    Failed,
}

#[derive(Default)]
pub struct ImportJob {
    pub state: JobState,
    pub done: usize,
    pub report: Option<bia_importer::Report>,
    pub error: Option<String>,
    pub source: Option<String>,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
}

#[derive(Serialize)]
struct JobView {
    state: JobState,
    done: usize,
    report: Option<bia_importer::Report>,
    error: Option<String>,
    source: Option<String>,
    started_at: Option<i64>,
    finished_at: Option<i64>,
}

#[derive(Serialize)]
struct Stats {
    questions: i64,
    active: i64,
    figures: i64,
    chapters: i64,
    db_bytes: u64,
    upstream_url: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/stats", get(stats))
        .route("/api/admin/import", get(job_view).post(start_import))
        .route("/api/admin/import/report.txt", get(report_txt))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn conflict(message: &str) -> Response {
    (StatusCode::CONFLICT, message.to_string()).into_response()
}

async fn room_is_live(state: &AppState) -> bool {
    let guard = state.room.lock().await;
    guard.as_ref().map(|room| room.is_alive()).unwrap_or(false)
}

async fn stats(_user: AdminUser, State(state): State<AppState>) -> Response {
    let counts = state
        .db
        .call(|conn| {
            use bia_core::schema::{chapters, figures, questions};
            let total: i64 = questions::table.count().get_result(conn)?;
            let active: i64 = questions::table
                .filter(questions::active.eq(true))
                .count()
                .get_result(conn)?;
            let figures: i64 = figures::table.count().get_result(conn)?;
            let chapters: i64 = chapters::table.count().get_result(conn)?;
            Ok((total, active, figures, chapters))
        })
        .await;

    let Ok((questions, active, figures, chapters)) = counts else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "Base indisponible.").into_response();
    };
    let db_bytes = fs::metadata(&state.cfg.db.path)
        .map(|meta| meta.len())
        .unwrap_or(0);

    Json(Stats {
        questions,
        active,
        figures,
        chapters,
        db_bytes,
        upstream_url: state.cfg.import.upstream_url.clone(),
    })
    .into_response()
}

async fn job_view(_user: AdminUser, State(state): State<AppState>) -> Response {
    let job = state.import.lock().expect("import job lock");
    Json(JobView {
        state: job.state,
        done: job.done,
        report: job.report.clone(),
        error: job.error.clone(),
        source: job.source.clone(),
        started_at: job.started_at,
        finished_at: job.finished_at,
    })
    .into_response()
}

async fn report_txt(_user: AdminUser, State(state): State<AppState>) -> Response {
    let job = state.import.lock().expect("import job lock");
    match &job.report {
        Some(report) => {
            let mut response = Response::new(report.to_text().into());
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain; charset=utf-8"),
            );
            response
        }
        None => (StatusCode::NOT_FOUND, "Aucun rapport disponible.").into_response(),
    }
}

async fn start_import(
    _user: AdminUser,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if !crate::auth::same_origin(&headers) {
        return (StatusCode::FORBIDDEN, "Requête refusée.").into_response();
    }
    if room_is_live(&state).await {
        return conflict("Une partie est en cours : terminez-la avant d'importer.");
    }
    {
        let mut job = state.import.lock().expect("import job lock");
        if matches!(job.state, JobState::Downloading | JobState::Importing) {
            return conflict("Un import est déjà en cours.");
        }
        *job = ImportJob {
            state: JobState::Downloading,
            source: Some("upstream".into()),
            started_at: Some(now_ms()),
            ..ImportJob::default()
        };
    }

    spawn_job(state);
    StatusCode::ACCEPTED.into_response()
}

fn spawn_job(state: AppState) {
    let db_path = state.cfg.db.path.clone();
    let upstream = state.cfg.import.upstream_url.clone();
    let job = state.import.clone();
    let figures = state.figures.clone();
    let db = state.db.clone();

    tokio::spawn(async move {
        let job_for_progress = job.clone();
        let result =
            tokio::task::spawn_blocking(move || -> anyhow::Result<bia_importer::Report> {
                // The downloaded TSV/figures are input only: keep them out of the
                // state directory and delete them when the job ends.
                let work = tempfile::Builder::new().prefix("bia-import-").tempdir()?;
                let args = ImportArgs {
                    tsv: work.path().join("annales-bia.tsv"),
                    db: db_path.clone(),
                    images: work.path().join("figures"),
                    chapters: None,
                    report: work.path().join("import-report.txt"),
                    fetch: true,
                    upstream,
                };
                let mut conn = db::open_connection(&db_path)?;
                let mut progress = move |done: usize| {
                    if let Ok(mut current) = job_for_progress.lock() {
                        current.state = JobState::Importing;
                        current.done = done;
                    }
                };
                bia_importer::run(&mut conn, &args, &mut progress)
            })
            .await;

        let reload = {
            let mut current = job.lock().expect("import job lock");
            match result {
                Ok(Ok(report)) => {
                    current.state = JobState::Done;
                    current.done = report.read;
                    current.report = Some(report);
                    current.finished_at = Some(now_ms());
                    true
                }
                Ok(Err(err)) => {
                    current.state = JobState::Failed;
                    current.error = Some(err.to_string());
                    current.finished_at = Some(now_ms());
                    false
                }
                Err(err) => {
                    current.state = JobState::Failed;
                    current.error = Some(format!("tâche interrompue: {err}"));
                    current.finished_at = Some(now_ms());
                    false
                }
            }
        };

        if reload {
            if let Ok(rows) = db::load_figures(&db).await {
                let map = rows
                    .into_iter()
                    .map(|figure| (figure.id, Arc::new(figure.data)))
                    .collect();
                if let Ok(mut cache) = figures.write() {
                    *cache = map;
                }
            }
        }
    });
}
