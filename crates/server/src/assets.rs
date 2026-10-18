use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../frontend/dist"]
struct Frontend;

fn embedded(file: rust_embed::EmbeddedFile, name: &str, cache: &'static str) -> Response {
    let mime = mime_guess::from_path(name).first_or_octet_stream();
    let mut response = Response::new(Body::from(file.data.into_owned()));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(mime.as_ref()).unwrap(),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    response
}

/// Figures live in SQLite (`figures` table) and are cached in memory at startup.
pub async fn figure(Path(name): Path<String>, State(state): State<crate::AppState>) -> Response {
    let id = name.strip_suffix(".jpeg").unwrap_or(&name);
    let data = state
        .figures
        .read()
        .ok()
        .and_then(|cache| cache.get(id).cloned());
    match data {
        Some(data) => {
            let mut response = Response::new(Body::from(data.as_ref().clone()));
            response
                .headers_mut()
                .insert(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"));
            response.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=86400"),
            );
            response
        }
        None => (StatusCode::NOT_FOUND, "Figure introuvable.").into_response(),
    }
}

fn index() -> Response {
    match Frontend::get("index.html") {
        Some(file) => embedded(file, "index.html", "no-cache"),
        None => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Frontend non compilé (frontend/dist absent).",
        )
            .into_response(),
    }
}

/// Serve a static file when it exists, otherwise fall back to the SPA shell.
pub async fn spa(uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.is_empty() {
        return index();
    }
    match Frontend::get(path) {
        Some(file) => {
            let cache = if path.starts_with("assets/") {
                "public, max-age=31536000, immutable"
            } else {
                "no-cache"
            };
            embedded(file, path, cache)
        }
        None => index(),
    }
}

/// The SPA shell, used by the authenticated `/prof` and `/admin` pages.
pub fn index_response() -> Response {
    index()
}
