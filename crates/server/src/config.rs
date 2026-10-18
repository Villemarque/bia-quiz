use std::path::PathBuf;

use anyhow::Context;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub server: ServerConfig,
    pub db: DbConfig,
    pub auth: AuthConfig,
    pub prof: AccountConfig,
    pub admin: AccountConfig,
    pub import: ImportConfig,
    pub limits: LimitsConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub bind: String,
    pub public_base_url: String,
    pub max_players: usize,
    /// Public URL of the deployed source (AGPL network-use obligation).
    pub source_url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DbConfig {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    pub session_hours: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AccountConfig {
    pub user: String,
    pub password: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ImportConfig {
    pub upstream_url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LimitsConfig {
    pub requests_per_ip_per_minute: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            db: DbConfig::default(),
            auth: AuthConfig::default(),
            prof: AccountConfig {
                user: "prof".into(),
                password: String::new(),
            },
            admin: AccountConfig {
                user: "admin".into(),
                password: String::new(),
            },
            import: ImportConfig::default(),
            limits: LimitsConfig::default(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:8080".into(),
            public_base_url: "http://localhost:8080".into(),
            max_players: 60,
            source_url: "https://github.com/Villemarque/bia-quiz".into(),
        }
    }
}

impl Default for DbConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from("bia.sqlite3"),
        }
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self { session_hours: 12 }
    }
}

impl Default for ImportConfig {
    fn default() -> Self {
        Self {
            upstream_url: "https://github.com/Villemarque/bia-annales-database-csv/archive/refs/heads/master.tar.gz".into(),
        }
    }
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            requests_per_ip_per_minute: 240,
        }
    }
}

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let path = env("BIA_QUIZ_CONFIG").unwrap_or_else(|| "bia.toml".into());
        let mut cfg: Config = if std::path::Path::new(&path).is_file() {
            let raw = std::fs::read_to_string(&path)
                .with_context(|| format!("cannot read config {path}"))?;
            toml::from_str(&raw).with_context(|| format!("invalid config {path}"))?
        } else {
            Config::default()
        };

        if let Some(v) = env("BIA_QUIZ_BIND") {
            cfg.server.bind = v;
        }
        if let Some(v) = env("BIA_QUIZ_PUBLIC_BASE_URL") {
            cfg.server.public_base_url = v;
        }
        if let Some(v) = env("BIA_QUIZ_MAX_PLAYERS") {
            cfg.server.max_players = v.parse()?;
        }
        if let Some(v) = env("BIA_QUIZ_SOURCE_URL") {
            cfg.server.source_url = v;
        }
        if let Some(v) = env("BIA_QUIZ_DB_PATH") {
            cfg.db.path = v.into();
        }
        if let Some(v) = env("BIA_QUIZ_SESSION_HOURS") {
            cfg.auth.session_hours = v.parse()?;
        }
        if let Some(v) = env("BIA_QUIZ_PROF_USER") {
            cfg.prof.user = v;
        }
        if let Some(v) = env("BIA_QUIZ_PROF_PASSWORD") {
            cfg.prof.password = v;
        }
        if let Some(v) = env("BIA_QUIZ_ADMIN_USER") {
            cfg.admin.user = v;
        }
        if let Some(v) = env("BIA_QUIZ_ADMIN_PASSWORD") {
            cfg.admin.password = v;
        }
        if let Some(v) = env("BIA_QUIZ_UPSTREAM_URL") {
            cfg.import.upstream_url = v;
        }
        if let Some(v) = env("BIA_QUIZ_RATE") {
            cfg.limits.requests_per_ip_per_minute = v.parse()?;
        }

        cfg.server.public_base_url = cfg.server.public_base_url.trim_end_matches('/').to_string();
        Ok(cfg)
    }

    pub fn join_url(&self, code: &str) -> String {
        format!("{}/j/{code}", self.server.public_base_url)
    }

    pub fn secure_cookies(&self) -> bool {
        self.server.public_base_url.starts_with("https://")
    }
}
