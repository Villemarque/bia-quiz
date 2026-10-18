//! `bia-quiz` server library: router, state and modules (also used by the E2E test).

pub mod admin;
pub mod api;
pub mod assets;
pub mod auth;
pub mod config;
pub mod db;
pub mod room;
pub mod ws;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use axum::extract::DefaultBodyLimit;
use axum::middleware;
use axum::routing::{get, post};
use axum::Router;
use time::Duration as TimeDuration;
use tokio::sync::Mutex;
use tower_sessions::cookie::{Key, SameSite};
use tower_sessions::MemoryStore;

use crate::admin::ImportJob;
use crate::auth::{LoginLimiter, RateLimiter};
use crate::config::Config;
use crate::db::Db;
use crate::room::RoomHandle;

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub db: Db,
    pub room: Arc<Mutex<Option<RoomHandle>>>,
    pub limiter: Arc<RateLimiter>,
    pub login_limiter: Arc<LoginLimiter>,
    /// Figures loaded from SQLite at startup, keyed by attachment id.
    pub figures: Arc<RwLock<HashMap<String, Arc<Vec<u8>>>>>,
    /// One import job at a time.
    pub import: Arc<std::sync::Mutex<ImportJob>>,
}

impl AppState {
    /// Reload the figure cache from SQLite (after an import).
    pub async fn reload_figures(&self) -> anyhow::Result<usize> {
        let rows = db::load_figures(&self.db).await?;
        let count = rows.len();
        let map = rows
            .into_iter()
            .map(|figure| (figure.id, Arc::new(figure.data)))
            .collect();
        *self.figures.write().expect("figure cache lock") = map;
        Ok(count)
    }
}

pub fn router(state: AppState) -> Router {
    let backend = auth::Backend::from_config(&state.cfg.prof, &state.cfg.admin);
    if backend.is_empty() {
        tracing::warn!(
            "no login accounts configured: set [prof].password and [admin].password in bia.toml"
        );
    }
    let session_layer =
        axum_login::tower_sessions::SessionManagerLayer::new(MemoryStore::default())
            .with_name("bia_session")
            .with_secure(state.cfg.secure_cookies())
            .with_same_site(SameSite::Lax)
            .with_http_only(true)
            .with_signed(Key::generate())
            .with_expiry(axum_login::tower_sessions::Expiry::OnInactivity(
                TimeDuration::hours(state.cfg.auth.session_hours.max(1)),
            ));
    let auth_layer = axum_login::AuthManagerLayerBuilder::new(backend, session_layer).build();

    Router::new()
        .route("/healthz", get(api::healthz))
        .route("/api/chapters", get(api::chapters))
        .route("/api/auth/me", get(api::me))
        .route("/api/meta", get(api::meta))
        .route("/api/prof/sessions", get(api::prof_sessions))
        .route("/api/prof/sessions/{id}", get(api::prof_session))
        .route(
            "/api/prof/sessions/{id}/export.csv",
            get(api::prof_session_csv),
        )
        .merge(admin::router())
        .route("/api/{*rest}", get(api::not_found))
        .route("/ws", get(ws::ws))
        .route("/figures/{file}", get(assets::figure))
        .route("/login", get(auth::login_page).post(auth::login_submit))
        .route("/logout", post(auth::logout))
        .route("/prof", get(auth::prof_page))
        .route("/prof/", get(auth::prof_page))
        .route("/admin", get(auth::admin_page))
        .route("/admin/", get(auth::admin_page))
        .fallback(assets::spa)
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::rate_limit,
        ))
        .layer(auth_layer)
        .with_state(state)
}
