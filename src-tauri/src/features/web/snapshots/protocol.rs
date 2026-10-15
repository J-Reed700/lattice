//! The private `lattice-snapshot` URL scheme (§8): serves a frozen copy to the
//! main webview with a locked-down CSP after re-checking its hash, and turns a
//! link click inside it into an event plus `204 No Content`.

use std::sync::Arc;

use tauri::http::{header, Request, Response, StatusCode};
use tauri::{AppHandle, Manager, Runtime};

use super::dto::{WebPageSnapshotLinkEvent, WebPageSnapshotStatus, LINK_EVENT};
use super::store::{is_valid_id, Verified};
use super::WebPageSnapshotService;

pub const SCHEME: &str = "lattice-snapshot";
/// The only webview allowed to load snapshots (T13).
pub const ALLOWED_WEBVIEW: &str = "main";
const MAX_LINK_LEN: usize = 2048;

pub const CSP: &str = "default-src 'none'; img-src data:; media-src data:; font-src data:; \
style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'; sandbox";

/// The clean layer of §6.6, added only for `?clean=1`; the file on disk is
/// never modified.
pub const CLEAN_CSS: &str = r#"[class*="cookie" i], [id*="cookie" i], [class*="consent" i], [id*="consent" i],
[class*="gdpr" i], [class*="newsletter" i][class*="modal" i], [class*="newsletter" i][class*="popup" i],
[class*="paywall" i], [id*="paywall" i], [class*="subscribe-modal" i], [class*="interstitial" i],
[class*="modal-backdrop" i], [class*="overlay" i][class*="modal" i], [role="dialog"], [aria-modal="true"],
[class*="sticky-footer" i], [class*="smartbanner" i] { display: none !important; }
html, body { overflow: auto !important; height: auto !important; position: static !important; }
* { cursor: default !important; }
a, area { cursor: pointer !important; }"#;

const INTEGRITY_PAGE: &str =
    "<!doctype html><meta charset=\"utf-8\"><title>Integrity check failed</title>\
<p>This frozen copy no longer matches its recorded fingerprint and was not shown.</p>";
const UNKNOWN_PAGE: &str = "<!doctype html><meta charset=\"utf-8\"><title>Not found</title>\
<p>There is no frozen copy here.</p>";
const PENDING_PAGE: &str = "<!doctype html><meta charset=\"utf-8\"><title>Capturing</title>\
<p>This frozen copy is still being captured.</p>";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Snapshot { id: String, clean: bool },
    Link { url: String, id: String },
    NotFound,
}

/// Parses a request. `path` is the raw (possibly percent-encoded) URI path;
/// it is decoded once, then validated.
pub fn route(method: &str, path: &str, query: Option<&str>) -> Route {
    if method != "GET" {
        return Route::NotFound;
    }
    let Ok(decoded) = urlencoding::decode(path) else {
        return Route::NotFound;
    };
    let pairs = || url::form_urlencoded::parse(query.unwrap_or_default().as_bytes());
    if decoded == "/l" {
        let mut url = None;
        let mut id = None;
        for (key, value) in pairs() {
            match key.as_ref() {
                "u" => url = Some(value.into_owned()),
                "s" => id = Some(value.into_owned()),
                _ => {}
            }
        }
        let (Some(url), Some(id)) = (url, id) else {
            return Route::NotFound;
        };
        let web = url.len() < MAX_LINK_LEN
            && url::Url::parse(&url)
                .is_ok_and(|parsed| matches!(parsed.scheme(), "http" | "https"));
        if !web || !is_valid_id(&id) {
            return Route::NotFound;
        }
        return Route::Link { url, id };
    }
    let Some(id) = decoded
        .strip_prefix('/')
        .and_then(|rest| rest.strip_suffix("/index.html"))
    else {
        return Route::NotFound;
    };
    if !is_valid_id(id) {
        return Route::NotFound;
    }
    let clean = pairs().any(|(key, value)| key == "clean" && value == "1");
    Route::Snapshot {
        id: id.to_string(),
        clean,
    }
}

/// `404`, empty body: every request that is not a snapshot or a link.
pub fn not_found() -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::CONTENT_LENGTH, "0")
        .body(Vec::new())
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

/// A response with the full header set of §8.4.
fn locked(status: StatusCode, body: Vec<u8>) -> Response<Vec<u8>> {
    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_SECURITY_POLICY, CSP)
        .header(header::REFERRER_POLICY, "no-referrer")
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::CONTENT_LENGTH, body.len().to_string());
    if !body.is_empty() {
        builder = builder.header(header::CONTENT_TYPE, "text/html; charset=utf-8");
    }
    builder
        .body(body)
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

fn page(status: StatusCode, html: &str) -> Response<Vec<u8>> {
    locked(status, html.as_bytes().to_vec())
}

/// Inserts the clean layer before the first `</head>` (or at the start when
/// there is none).
pub fn with_clean_layer(html: &[u8]) -> Vec<u8> {
    let style = format!("<style id=\"lattice-clean\">{CLEAN_CSS}</style>");
    let needle = b"</head>";
    let at = html
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle))
        .unwrap_or(0);
    let (before, after) = html.split_at(at.min(html.len()));
    let mut out = Vec::with_capacity(html.len() + style.len());
    out.extend_from_slice(before);
    out.extend_from_slice(style.as_bytes());
    out.extend_from_slice(after);
    out
}

/// Answers one request for `webview_label`.
pub async fn respond(
    service: &WebPageSnapshotService,
    webview_label: &str,
    method: &str,
    path: &str,
    query: Option<&str>,
) -> Response<Vec<u8>> {
    if webview_label != ALLOWED_WEBVIEW {
        return not_found();
    }
    match route(method, path, query) {
        Route::NotFound => not_found(),
        Route::Link { url, id } => {
            let payload = WebPageSnapshotLinkEvent {
                snapshot_id: id,
                url,
            };
            if let Ok(value) = serde_json::to_value(&payload) {
                service.shared().events.emit(LINK_EVENT, value);
            }
            locked(StatusCode::NO_CONTENT, Vec::new())
        }
        Route::Snapshot { id, clean } => {
            let shared = service.shared();
            let row = match shared.repository.get(&id).await {
                Ok(Some(row)) => row,
                Ok(None) => return page(StatusCode::NOT_FOUND, UNKNOWN_PAGE),
                Err(error) => {
                    tracing::warn!(%error, "Could not read a web snapshot row");
                    return page(StatusCode::NOT_FOUND, UNKNOWN_PAGE);
                }
            };
            match row.status() {
                WebPageSnapshotStatus::Pending => return page(StatusCode::TOO_EARLY, PENDING_PAGE),
                WebPageSnapshotStatus::Failed => return page(StatusCode::NOT_FOUND, UNKNOWN_PAGE),
                WebPageSnapshotStatus::Ready => {}
            }
            let Some(expected) = row.sha256.as_deref() else {
                return page(StatusCode::CONFLICT, INTEGRITY_PAGE);
            };
            match shared.store.read_verified(&id, expected).await {
                Verified::Intact(bytes) => {
                    let body = if clean {
                        with_clean_layer(&bytes)
                    } else {
                        bytes
                    };
                    locked(StatusCode::OK, body)
                }
                Verified::Mismatch | Verified::Missing => {
                    tracing::warn!(snapshot_id = %id, "A frozen web snapshot failed its integrity check");
                    page(StatusCode::CONFLICT, INTEGRITY_PAGE)
                }
            }
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
    let Some(service) = app.try_state::<Arc<WebPageSnapshotService>>() else {
        return not_found();
    };
    respond(
        service.as_ref(),
        &webview_label,
        request.method().as_str(),
        request.uri().path(),
        request.uri().query(),
    )
    .await
}
