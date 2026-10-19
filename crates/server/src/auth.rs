//! Session authentication with `axum-login` (prof + admin accounts).

use std::collections::HashMap;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
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
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Form(creds): Form<Credentials>,
) -> Response {
    if !same_origin(&headers, &state.cfg.server.public_base_url) {
        return (StatusCode::FORBIDDEN, "Requête refusée.").into_response();
    }

    let ip_key = client_ip_key(&headers, Some(peer));
    let key = login_key(&ip_key, &creds.username);
    let account_key = account_key(&creds.username);
    if state.login_limiter.blocked(&key) || state.login_limiter.blocked_account(&account_key) {
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
            state.login_limiter.clear(&account_key);
            if auth.login(&user).await.is_err() {
                return (StatusCode::INTERNAL_SERVER_ERROR, "Session indisponible.")
                    .into_response();
            }
            Redirect::to(&safe_next(next.as_deref())).into_response()
        }
        _ => {
            state.login_limiter.fail(&key);
            state.login_limiter.fail(&account_key);
            login_html(
                next.as_deref(),
                Some("Identifiants incorrects."),
                StatusCode::UNAUTHORIZED,
            )
        }
    }
}

pub async fn logout(
    State(state): State<AppState>,
    mut auth: AuthSession,
    headers: HeaderMap,
) -> Response {
    if !same_origin(&headers, &state.cfg.server.public_base_url) {
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

/// Lower-cased `scheme://authority` with default ports removed.
fn origin_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    let authority = rest.split(['/', '?', '#']).next()?.to_ascii_lowercase();
    if authority.is_empty() {
        return None;
    }
    let authority = match scheme.as_str() {
        "http" => authority.strip_suffix(":80").unwrap_or(&authority),
        "https" => authority.strip_suffix(":443").unwrap_or(&authority),
        _ => &authority,
    };
    Some(format!("{scheme}://{authority}"))
}

/// Strict same-origin check used for logins, admin mutations and teacher sockets.
///
/// The reference is `public_base_url` from the config, never the client-supplied
/// `Host` header. `Origin` must match exactly. Requests without `Origin` are only
/// accepted when the browser explicitly says `Sec-Fetch-Site: same-origin`/`none`.
pub fn same_origin(headers: &HeaderMap, public_base_url: &str) -> bool {
    let expected = match origin_of(public_base_url) {
        Some(expected) => expected,
        None => return false,
    };
    let site = headers
        .get("sec-fetch-site")
        .and_then(|value| value.to_str().ok());
    if let Some(site) = site {
        if site != "same-origin" && site != "none" {
            return false;
        }
    }
    match headers.get(header::ORIGIN) {
        Some(origin) => origin
            .to_str()
            .ok()
            .and_then(origin_of)
            .map(|origin| origin == expected)
            .unwrap_or(false),
        None => matches!(site, Some("same-origin" | "none")),
    }
}

/// Client IP. `X-Forwarded-For` is trusted only when the direct peer is loopback
/// (the Caddy reverse proxy on this host); direct remote peers are always used.
pub fn client_ip(headers: &HeaderMap, peer: Option<SocketAddr>) -> Option<IpAddr> {
    let forwarded = || {
        headers
            .get("x-forwarded-for")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(',').next())
            .map(str::trim)
            .and_then(|value| value.parse::<IpAddr>().ok())
    };
    match peer.map(|peer| peer.ip()) {
        Some(ip) if ip.is_loopback() => forwarded().or(Some(ip)),
        Some(ip) => Some(ip),
        None => forwarded(),
    }
}

/// Group IPv6 clients by /64 so rotating the host bits cannot bypass a limit.
fn ip_key(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(ip) => ip.to_string(),
        IpAddr::V6(ip) => {
            let segments = ip.segments();
            Ipv6Addr::new(
                segments[0],
                segments[1],
                segments[2],
                segments[3],
                0,
                0,
                0,
                0,
            )
            .to_string()
        }
    }
}

fn client_ip_key(headers: &HeaderMap, peer: Option<SocketAddr>) -> String {
    client_ip(headers, peer)
        .map(ip_key)
        .unwrap_or_else(|| "unknown".to_string())
}

fn login_key(ip_key: &str, username: &str) -> String {
    format!("{ip_key}|{username}")
}

fn account_key(username: &str) -> String {
    format!("account|{username}")
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
    // A per-account lockout is shared by every source IP. Keep it high enough
    // that an attacker cannot trivially lock the teacher out with five tries.
    const ACCOUNT_MAX_FAILURES: u32 = 20;

    pub fn blocked(&self, key: &str) -> bool {
        self.blocked_with(key, Self::MAX_FAILURES)
    }

    pub fn blocked_account(&self, key: &str) -> bool {
        self.blocked_with(key, Self::ACCOUNT_MAX_FAILURES)
    }

    fn blocked_with(&self, key: &str, max_failures: u32) -> bool {
        let Ok(attempts) = self.attempts.lock() else {
            return false;
        };
        match attempts.get(key) {
            Some((started, count)) => started.elapsed() < Self::WINDOW && *count >= max_failures,
            None => false,
        }
    }

    pub fn fail(&self, key: &str) {
        let Ok(mut attempts) = self.attempts.lock() else {
            return;
        };
        {
            let entry = attempts
                .entry(key.to_string())
                .or_insert_with(|| (Instant::now(), 0));
            if entry.0.elapsed() >= Self::WINDOW {
                *entry = (Instant::now(), 0);
            }
            entry.1 += 1;
        }
        if attempts.len() > 4096 {
            attempts.retain(|_, (started, _)| started.elapsed() < Self::WINDOW);
        }
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
        if windows.len() > 8192 {
            windows
                .retain(|_, (started, _)| now.duration_since(*started) <= Duration::from_secs(60));
        }
        let entry = windows.entry(key.to_string()).or_insert((now, 0));
        if now.duration_since(entry.0) > Duration::from_secs(60) {
            *entry = (now, 0);
        }
        entry.1 += 1;
        entry.1 <= self.limit
    }
}

fn rate_limit_key(headers: &HeaderMap, peer: Option<SocketAddr>) -> String {
    client_ip_key(headers, peer)
}

pub async fn rate_limit(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let path = req.uri().path();
    if path == "/healthz" || path.starts_with("/figures/") || path.starts_with("/assets/") {
        return next.run(req).await;
    }
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0);
    let key = rate_limit_key(req.headers(), peer);
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderName, HeaderValue};
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.insert(
                HeaderName::from_static(name),
                HeaderValue::from_static(value),
            );
        }
        headers
    }

    #[test]
    fn same_origin_requires_an_exact_match() {
        let base = "https://quiz.example";
        assert!(same_origin(
            &headers(&[("origin", "https://quiz.example")]),
            base
        ));
        assert!(same_origin(
            &headers(&[("origin", "https://quiz.example:443")]),
            base
        ));
        assert!(!same_origin(
            &headers(&[("origin", "https://evilquiz.example")]),
            base
        ));
        assert!(!same_origin(
            &headers(&[("origin", "https://quiz.example.evil.test")]),
            base
        ));
        assert!(!same_origin(&headers(&[]), base));
        assert!(!same_origin(&headers(&[("origin", "null")]), base));
    }

    #[test]
    fn same_origin_rejects_cross_site_fetch_metadata() {
        assert!(!same_origin(
            &headers(&[
                ("origin", "https://quiz.example"),
                ("sec-fetch-site", "same-site")
            ]),
            "https://quiz.example"
        ));
        assert!(same_origin(
            &headers(&[
                ("origin", "https://quiz.example"),
                ("sec-fetch-site", "same-origin")
            ]),
            "https://quiz.example"
        ));
    }

    #[test]
    fn forwarded_for_is_only_trusted_from_loopback() {
        let loopback: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        let remote: SocketAddr = "198.51.100.9:5000".parse().unwrap();
        let headers = headers(&[("x-forwarded-for", "203.0.113.5, 127.0.0.1")]);
        assert_eq!(
            client_ip(&headers, Some(loopback)),
            Some(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 5)))
        );
        assert_eq!(
            client_ip(&headers, Some(remote)),
            Some(IpAddr::V4(Ipv4Addr::new(198, 51, 100, 9)))
        );
        assert_eq!(
            client_ip(&headers, None),
            Some(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 5)))
        );
    }

    #[test]
    fn ipv6_is_keyed_by_64() {
        let one: Ipv6Addr = "2001:db8:1:2:aaaa::1".parse().unwrap();
        let two: Ipv6Addr = "2001:db8:1:2:bbbb::2".parse().unwrap();
        let other: Ipv6Addr = "2001:db8:1:3::1".parse().unwrap();
        assert_eq!(ip_key(IpAddr::V6(one)), ip_key(IpAddr::V6(two)));
        assert_ne!(ip_key(IpAddr::V6(one)), ip_key(IpAddr::V6(other)));
    }
}
