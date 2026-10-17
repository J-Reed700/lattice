//! The private `lattice-image` URL scheme (§7.7): serves a stored picture to
//! the main webview after re-checking its hash, with the type sniffed when
//! it was stored and nothing the webview could run.

use std::sync::Arc;

use tauri::http::{header, Request, Response, StatusCode};
use tauri::{AppHandle, Manager, Runtime};

use super::is_allowed_extension;
use super::store::{is_valid_id, Verified};
use super::ChatImageService;

pub const SCHEME: &str = "lattice-image";
/// The only webview allowed to load pictures (§5 T6).
pub const ALLOWED_WEBVIEW: &str = "main";

/// Nothing a picture response could carry may load or run anything.
pub const CSP: &str = "default-src 'none'";

/// An id names one file's content forever, so a served picture never changes.
const IMMUTABLE: &str = "private, max-age=31536000, immutable";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Image { id: String, ext: String },
    NotFound,
}

/// Parses a request. `path` is the raw (possibly percent-encoded) URI path;
/// it is decoded once, then validated: `GET /<32 hex>.<jpg|png|gif|webp>`.
pub fn route(method: &str, path: &str) -> Route {
    if method != "GET" {
        return Route::NotFound;
    }
    let Ok(decoded) = urlencoding::decode(path) else {
        return Route::NotFound;
    };
    let Some((id, ext)) = decoded
        .strip_prefix('/')
        .and_then(|name| name.split_once('.'))
    else {
        return Route::NotFound;
    };
    if !is_valid_id(id) || !is_allowed_extension(ext) {
        return Route::NotFound;
    }
    Route::Image {
        id: id.to_string(),
        ext: ext.to_string(),
    }
}

/// An empty answer with `status`, never cached.
fn empty(status: StatusCode) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_SECURITY_POLICY, CSP)
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::CONTENT_LENGTH, "0")
        .body(Vec::new())
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

/// `404`, empty body: every request that is not a stored picture.
pub fn not_found() -> Response<Vec<u8>> {
    empty(StatusCode::NOT_FOUND)
}

fn picture(mime: &str, body: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CONTENT_LENGTH, body.len().to_string())
        .header(header::CACHE_CONTROL, IMMUTABLE)
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::CONTENT_SECURITY_POLICY, CSP)
        .body(body)
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

/// Answers one request for `webview_label`.
pub async fn respond(
    service: &ChatImageService,
    webview_label: &str,
    method: &str,
    path: &str,
) -> Response<Vec<u8>> {
    if webview_label != ALLOWED_WEBVIEW {
        return not_found();
    }
    let Route::Image { id, ext } = route(method, path) else {
        return not_found();
    };
    let row = match service.repository().get(&id).await {
        Ok(Some(row)) if row.ext == ext => row,
        Ok(_) => return not_found(),
        Err(error) => {
            tracing::warn!(%error, "Could not read a chat image row");
            return not_found();
        }
    };
    match service.store().read_verified(&id, &ext, &row.sha256).await {
        Verified::Intact(bytes) => picture(&row.mime, bytes),
        Verified::Mismatch | Verified::Missing => {
            tracing::warn!(image_id = %id, "A chat image failed its integrity check");
            empty(StatusCode::CONFLICT)
        }
    }
}

/// The scheme handler body: resolves the service the web plugin manages in
/// app state and answers the request. Called on the async runtime by `main.rs`.
pub async fn handle<R: Runtime>(
    app: AppHandle<R>,
    webview_label: String,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    let Some(service) = app.try_state::<Arc<ChatImageService>>() else {
        return not_found();
    };
    respond(
        service.as_ref(),
        &webview_label,
        request.method().as_str(),
        request.uri().path(),
    )
    .await
}
