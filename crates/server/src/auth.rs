//! Session authentication with `axum-login` (prof + admin accounts).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::{ConnectInfo, FromRequestParts, Query, Request, State};
use axum::http::request::Parts;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::Form;
use axum_login::{AuthUser, AuthnBackend, UserId};
use serde::{Deserialize, Serialize};

use crate::config::AccountConfig;
use crate::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Prof,
    Admin,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub role: Role,
    pub password_hash: String,
}

impl std::fmt::Debug for User {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("User")
            .field("id", &self.id)
            .field("username", &self.username)
            .field("role", &self.role)
            .field("password_hash", &"[redacted]")
            .finish()
    }
}

impl AuthUser for User {
    type Id = i64;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn session_auth_hash(&self) -> &[u8] {
        self.password_hash.as_bytes()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Credentials {
    pub username: String,
    pub password: String,
    pub next: Option<String>,
}

#[derive(Clone)]
pub struct Backend {
    users: Arc<Vec<User>>,
}

impl Backend {
    /// Build the two service accounts from config, hashing plaintext passwords at startup.
    /// Empty passwords disable the account.
    pub fn from_config(prof: &AccountConfig, admin: &AccountConfig) -> Self {
        let mut users = Vec::new();
        if !prof.user.is_empty() && !prof.password.is_empty() {
            users.push(User::new(1, &prof.user, Role::Prof, &prof.password));
        }
        if !admin.user.is_empty() && !admin.password.is_empty() {
            users.push(User::new(2, &admin.user, Role::Admin, &admin.password));
        }
        Self {
            users: Arc::new(users),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.users.is_empty()
    }
}

impl User {
    fn new(id: i64, username: &str, role: Role, password: &str) -> Self {
        Self {
            id,
            username: username.to_string(),
            role,
            password_hash: hash_password(password),
        }
    }
}

fn hash_password(password: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("argon2 hashing")
        .to_string()
}

fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .ok()
        .map(|parsed| {
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        })
        .unwrap_or(false)
}

impl AuthnBackend for Backend {
    type User = User;
    type Credentials = Credentials;
    type Error = std::convert::Infallible;

    async fn authenticate(
        &self,
        creds: Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        let user = self
            .users
            .iter()
            .find(|user| user.username == creds.username)
            .filter(|user| verify_password(&creds.password, &user.password_hash));
        Ok(user.cloned())
    }

    async fn get_user(&self, user_id: &UserId<Self>) -> Result<Option<Self::User>, Self::Error> {
        Ok(self.users.iter().find(|user| user.id == *user_id).cloned())
    }
}

pub type AuthSession = axum_login::AuthSession<Backend>;

// ---------------------------------------------------------------- API guards

fn unauthorized() -> Response {
    (StatusCode::UNAUTHORIZED, "Authentification requise.").into_response()
}

fn forbidden() -> Response {
    (StatusCode::FORBIDDEN, "Accès réservé à l'administrateur.").into_response()
}

pub struct ApiUser(pub User);

impl FromRequestParts<AppState> for ApiUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth = <AuthSession as FromRequestParts<AppState>>::from_request_parts(parts, state)
            .await
            .map_err(|_| unauthorized())?;
        match auth.user {
            Some(user) => Ok(ApiUser(user)),
            None => Err(unauthorized()),
        }
    }
}

pub struct AdminUser(pub User);

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth = <AuthSession as FromRequestParts<AppState>>::from_request_parts(parts, state)
            .await
            .map_err(|_| unauthorized())?;
        match auth.user {
            Some(user) if user.role == Role::Admin => Ok(AdminUser(user)),
            Some(_) => Err(forbidden()),
            None => Err(unauthorized()),
        }
    }
}

// ---------------------------------------------------------------- pages

fn redirect_login(next: &str) -> Response {
    Redirect::to(&format!("/login?next={next}")).into_response()
}

fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

pub async fn prof_page(auth: AuthSession) -> Response {
    match auth.user {
        Some(_) => no_store(crate::assets::index_response()),
        None => redirect_login("/prof"),
    }
}

pub async fn admin_page(auth: AuthSession) -> Response {
    match auth.user {
        Some(user) if user.role == Role::Admin => no_store(crate::assets::index_response()),
        Some(_) => forbidden(),
        None => redirect_login("/admin"),
    }
}

// ---------------------------------------------------------------- login/logout

#[derive(Debug, Deserialize)]
pub struct NextUrl {
    pub next: Option<String>,
}

pub async fn login_page(auth: AuthSession, Query(query): Query<NextUrl>) -> Response {
    if auth.user.is_some() {
        return Redirect::to("/prof").into_response();
    }
    login_html(query.next.as_deref(), None, StatusCode::OK)
}

pub async fn login_submit(
    State(state): State<AppState>,
    mut auth: AuthSession,
    Query(query): Query<NextUrl>,
    headers: HeaderMap,
    Form(creds): Form<Credentials>,
) -> Response {
    if !same_origin(&headers) {
        return (StatusCode::FORBIDDEN, "Requête refusée.").into_response();
    }

    let key = client_key(&headers, &creds.username);
    if state.login_limiter.blocked(&key) {
        return login_html(
            query.next.as_deref(),
            Some("Trop de tentatives. Réessayez dans 15 minutes."),
            StatusCode::TOO_MANY_REQUESTS,
        );
    }

    let next = query.next.clone().or_else(|| creds.next.clone());
    match auth.authenticate(creds).await {
        Ok(Some(user)) => {
            state.login_limiter.clear(&key);
            if auth.login(&user).await.is_err() {
                return (StatusCode::INTERNAL_SERVER_ERROR, "Session indisponible.")
                    .into_response();
            }
            Redirect::to(&safe_next(next.as_deref())).into_response()
        }
        _ => {
            state.login_limiter.fail(&key);
            login_html(
                next.as_deref(),
                Some("Identifiants incorrects."),
                StatusCode::UNAUTHORIZED,
            )
        }
    }
}

pub async fn logout(mut auth: AuthSession, headers: HeaderMap) -> Response {
    if !same_origin(&headers) {
        return (StatusCode::FORBIDDEN, "Requête refusée.").into_response();
    }
    match auth.logout().await {
        Ok(_) => Redirect::to("/login").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Session indisponible.").into_response(),
    }
}

fn safe_next(next: Option<&str>) -> String {
    match next {
        Some(path) if path.starts_with('/') && !path.starts_with("//") => path.to_string(),
        _ => "/prof".to_string(),
    }
}

// ---------------------------------------------------------------- CSRF / IP

pub fn same_origin(headers: &HeaderMap) -> bool {
    if let Some(site) = headers
        .get("sec-fetch-site")
        .and_then(|value| value.to_str().ok())
    {
        if site == "cross-site" {
            return false;
        }
    }
    if let (Some(origin), Some(host)) = (
        headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()),
        headers.get(header::HOST).and_then(|v| v.to_str().ok()),
    ) {
        return origin.ends_with(host);
    }
    true
}

fn client_key(headers: &HeaderMap, username: &str) -> String {
    let ip = headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(|value| value.trim())
        .unwrap_or("unknown");
    format!("{ip}|{username}")
}

pub struct LoginLimiter {
    attempts: Mutex<HashMap<String, (Instant, u32)>>,
}

impl Default for LoginLimiter {
    fn default() -> Self {
        Self {
            attempts: Mutex::new(HashMap::new()),
        }
    }
}

impl LoginLimiter {
    const WINDOW: Duration = Duration::from_secs(15 * 60);
    const MAX_FAILURES: u32 = 5;

    pub fn blocked(&self, key: &str) -> bool {
        let Ok(attempts) = self.attempts.lock() else {
            return false;
        };
        match attempts.get(key) {
            Some((started, count)) => {
                started.elapsed() < Self::WINDOW && *count >= Self::MAX_FAILURES
            }
            None => false,
        }
    }

    pub fn fail(&self, key: &str) {
        let Ok(mut attempts) = self.attempts.lock() else {
            return;
        };
        let entry = attempts
            .entry(key.to_string())
            .or_insert_with(|| (Instant::now(), 0));
        if entry.0.elapsed() >= Self::WINDOW {
            *entry = (Instant::now(), 0);
        }
        entry.1 += 1;
    }

    pub fn clear(&self, key: &str) {
        if let Ok(mut attempts) = self.attempts.lock() {
            attempts.remove(key);
        }
    }
}

// ---------------------------------------------------------------- rate limit

/// Fixed one-minute window per IP for the whole application.
pub struct RateLimiter {
    limit: u32,
    windows: Mutex<HashMap<String, (Instant, u32)>>,
}

impl RateLimiter {
    pub fn new(limit: u32) -> Self {
        Self {
            limit,
            windows: Mutex::new(HashMap::new()),
        }
    }

    fn allow(&self, key: &str) -> bool {
        if self.limit == 0 {
            return true;
        }
        let mut windows = match self.windows.lock() {
            Ok(windows) => windows,
            Err(_) => return true,
        };
        let now = Instant::now();
        let entry = windows.entry(key.to_string()).or_insert((now, 0));
        if now.duration_since(entry.0) > Duration::from_secs(60) {
            *entry = (now, 0);
        }
        entry.1 += 1;
        entry.1 <= self.limit
    }
}

fn rate_limit_key(headers: &HeaderMap, extensions: &axum::http::Extensions) -> String {
    if let Some(forwarded) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first) = forwarded.split(',').next() {
            let first = first.trim();
            if !first.is_empty() {
                return first.to_string();
            }
        }
    }
    extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

pub async fn rate_limit(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let path = req.uri().path();
    if path == "/healthz" || path.starts_with("/figures/") || path.starts_with("/assets/") {
        return next.run(req).await;
    }
    let key = rate_limit_key(req.headers(), req.extensions());
    if !state.limiter.allow(&key) {
        return (StatusCode::TOO_MANY_REQUESTS, "Trop de requêtes.").into_response();
    }
    next.run(req).await
}

// ---------------------------------------------------------------- HTML

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn login_html(next: Option<&str>, error: Option<&str>, status: StatusCode) -> Response {
    let next = next.unwrap_or("/prof");
    let error = error
        .map(|message| format!("<p class=\"error\">{}</p>", escape(message)))
        .unwrap_or_default();
    let body = format!(
        r#"<!doctype html>
<html lang="fr">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex">
<title>Connexion — Quiz BIA</title>
<style>
@font-face {{ font-family: 'Source Sans 3'; src: url('/fonts/source-sans-3.woff2') format('woff2'); font-weight: 300 900; }}
:root {{ --navy:#000a33; --navy2:#071037; --coral:#e97770; --gold:#ffe484; --muted:#b9bdc7; }}
* {{ box-sizing:border-box; }}
body {{ margin:0; min-height:100vh; display:grid; place-items:center; padding:1.5rem;
  font-family:'Source Sans 3', system-ui, sans-serif;
  background: radial-gradient(900px 500px at 80% -10%, rgba(233,119,112,.28), transparent 60%), var(--navy);
  color:#fff; }}
.card {{ width:min(420px,100%); background:#fff; color:var(--navy); border-radius:18px;
  padding:2rem; box-shadow:0 18px 60px rgba(0,0,0,.35); }}
.brand {{ display:flex; align-items:center; gap:.6rem; font-weight:700; margin-bottom:1.2rem; }}
.mark {{ width:34px;height:34px;border-radius:10px;background:var(--coral);display:grid;place-items:center;color:var(--navy); }}
h1 {{ margin:0 0 .4rem; font-size:1.6rem; }}
p {{ margin:0 0 1rem; color:#6e737a; }}
label {{ display:block; font-weight:600; margin:.8rem 0 .3rem; color:var(--navy); }}
input {{ width:100%; padding:.7rem .9rem; border:1px solid #e6e6e6; border-radius:12px; font:inherit; }}
button {{ width:100%; margin-top:1.2rem; border:0; border-radius:999px; padding:.85rem;
  background:var(--coral); color:var(--navy); font:inherit; font-weight:700; cursor:pointer; }}
button:hover {{ background:#f98780; }}
.error {{ background:#fdecea; color:#8c2b25; border-radius:10px; padding:.6rem .8rem; font-weight:600; }}
a {{ color:#e97770; }}
.small {{ font-size:.85rem; }}
</style>
</head>
<body>
  <main class="card">
    <div class="brand"><span class="mark">✈</span> Quiz BIA · Triple A</div>
    <h1>Connexion</h1>
    <p>Accès réservé au professeur.</p>
    {error}
    <form method="post" action="/login">
      <input type="hidden" name="next" value="{next}">
      <label for="username">Identifiant</label>
      <input id="username" name="username" autocomplete="username" required autofocus>
      <label for="password">Mot de passe</label>
      <input id="password" name="password" type="password" autocomplete="current-password" required>
      <button type="submit">Se connecter</button>
    </form>
    <p class="small" style="margin-top:1rem"><a href="/">Retour au quiz</a></p>
  </main>
</body>
</html>"#,
        next = escape(next),
        error = error
    );

    let mut response = (status, Html(body)).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
