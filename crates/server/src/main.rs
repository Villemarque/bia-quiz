use std::collections::HashMap;
use std::io::IsTerminal;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

use anyhow::Context;
use bia_server::admin::ImportJob;
use bia_server::auth::RateLimiter;
use bia_server::config::Config;
use bia_server::db::Db;
use bia_server::{router, AppState};
use tokio::sync::Mutex;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_ansi(std::io::stdout().is_terminal())
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = Config::load().context("configuration")?;
    if config.prof.password.is_empty() {
        tracing::warn!(
            "prof.password is empty: /prof and /api/prof/* will answer 503 until it is set"
        );
    }

    let db = Db::open(&config.db.path)
        .with_context(|| format!("database {}", config.db.path.display()))?;

    let figure_rows = bia_server::db::load_figures(&db).await?;
    let figure_count = figure_rows.len();
    let figures: HashMap<String, Arc<Vec<u8>>> = figure_rows
        .into_iter()
        .map(|figure| (figure.id, Arc::new(figure.data)))
        .collect();
    if figure_count == 0 {
        tracing::warn!("no figures in the database: run bia-import to store them");
    } else {
        tracing::info!("loaded {figure_count} figures from SQLite");
    }

    let state = AppState {
        cfg: Arc::new(config.clone()),
        db,
        room: Arc::new(Mutex::new(None)),
        limiter: Arc::new(RateLimiter::new(config.limits.requests_per_ip_per_minute)),
        login_limiter: Arc::new(bia_server::auth::LoginLimiter::default()),
        figures: Arc::new(RwLock::new(figures)),
        import: Arc::new(std::sync::Mutex::new(ImportJob::default())),
    };

    let app = router(state);
    let listener = tokio::net::TcpListener::bind(&config.server.bind)
        .await
        .with_context(|| format!("cannot bind {}", config.server.bind))?;

    tracing::info!(
        "bia-quiz listening on http://{} (public: {})",
        config.server.bind,
        config.server.public_base_url
    );

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .context("server error")?;
    Ok(())
}
