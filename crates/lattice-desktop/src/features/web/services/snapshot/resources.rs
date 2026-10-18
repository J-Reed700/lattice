//! Subresource fetching for a capture (§6.3 "Fetch", §5 T3, T4, T11).
//!
//! Every URL goes through the same guarded client as the document: the
//! scheme check, `validate_url_resolved`, the redirect policy and the
//! public-only resolver. Bodies are read as streams and cut at the cap;
//! `content-length` may refuse early but is never trusted to accept. Only an
//! allowlist of content types becomes a `data:` URL; stylesheets are decoded
//! to UTF-8 text and inlined later as `<style>`, never as `data:`.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Mutex;

use base64::Engine;
use encoding_rs::{Encoding, UTF_8};
use futures::future::join_all;
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use reqwest::Client;
use tokio::sync::Semaphore;
use tokio::time::Instant;
use url::Url;

use super::css::{self, CssRefKind};
use super::{
    MAX_FAVICON_BYTES, MAX_IMPORT_DEPTH, MAX_RESOURCES, MAX_RESOURCE_BYTES, MAX_TOTAL_BYTES,
    RESOURCE_CONCURRENCY, RESOURCE_TIMEOUT,
};
use crate::features::web::services::identity::{self, BrowserProfile};
use crate::features::web::services::web::{fetchable_url, validate_url_resolved};
use crate::features::web::traits::page_snapshot::{
    CaptureProgress, DataUri, DroppedResource, ProgressSink, ResourceReport,
};

/// The MIME type stylesheets carry inside [`ResolvedResource::Inlined`]; the
/// bytes are UTF-8 text for a `<style>`, never a `data:` URL.
pub(super) const CSS_MIME: &str = "text/css";

/// What a resource is used as, which decides `Accept` and the allowed types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum ResourceKind {
    Image,
    Stylesheet,
    Font,
    Favicon,
}

impl ResourceKind {
    /// The `Accept` header a browser sends for this kind (§6.2).
    fn accept(self) -> &'static str {
        match self {
            Self::Image | Self::Favicon => "image/avif,image/webp,image/*,*/*;q=0.8",
            Self::Stylesheet => "text/css,*/*;q=0.1",
            Self::Font => "*/*",
        }
    }
}

/// A resource to fetch: its absolute (or `data:`) URL, kind and `@import` depth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResourceRef {
    pub url: String,
    pub kind: ResourceKind,
    pub depth: u8,
}

/// The outcome for one resource URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ResolvedResource {
    Inlined(DataUri),
    Dropped { reason: String },
}

fn dropped(reason: &str) -> ResolvedResource {
    ResolvedResource::Dropped {
        reason: reason.to_string(),
    }
}

/// Absolute URL to outcome, for the whole capture.
#[derive(Debug, Clone, Default)]
pub(super) struct ResolvedResources {
    map: HashMap<String, ResolvedResource>,
}

impl ResolvedResources {
    pub(super) fn insert(&mut self, url: impl Into<String>, resolved: ResolvedResource) {
        self.map.insert(url.into(), resolved);
    }

    pub(super) fn get(&self, url: &str) -> Option<&ResolvedResource> {
        self.map.get(url)
    }

    /// Counts for the snapshot row: inlined resources, their bytes, and the
    /// dropped ones sorted by URL.
    pub(super) fn report(&self) -> ResourceReport {
        let mut report = ResourceReport::default();
        for (url, resolved) in &self.map {
            match resolved {
                ResolvedResource::Inlined(data) => {
                    report.inlined += 1;
                    report.bytes_inlined += data.bytes.len() as u64;
                }
                ResolvedResource::Dropped { reason } => report.dropped.push(DroppedResource {
                    url: url.clone(),
                    reason: reason.clone(),
                }),
            }
        }
        report.dropped.sort_by(|a, b| a.url.cmp(&b.url));
        report
    }
}

/// Resolve a reference against `base`: a `data:` URL is kept as written; an
/// `http(s)` URL is made absolute without its fragment; anything else (and
/// garbage) is refused as `blocked_host` with no request made.
pub(super) fn resolve_reference(base: &Url, raw: &str) -> Result<String, &'static str> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("blocked_host");
    }
    if is_data_url(raw) {
        return Ok(raw.to_string());
    }
    let mut url = base.join(raw).map_err(|_| "blocked_host")?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("blocked_host");
    }
    url.set_fragment(None);
    Ok(url.to_string())
}

pub(super) fn is_data_url(url: &str) -> bool {
    url.get(..5)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("data:"))
}

/// Fetch one resource and judge it. Never returns an error: every failure
/// is a [`ResolvedResource::Dropped`] with the design's reason.
pub(super) async fn fetch_resource(
    client: &Client,
    profile: &'static BrowserProfile,
    url: &str,
    referer: &str,
    kind: ResourceKind,
    cap: usize,
) -> ResolvedResource {
    if is_data_url(url) {
        return resolve_data_url(url, kind, cap);
    }
    if fetchable_url(url).is_err() {
        return dropped("blocked_host");
    }
    let work = async {
        if validate_url_resolved(url).await.is_err() {
            return dropped("blocked_host");
        }
        download(client, profile, url, referer, kind, cap).await
    };
    tokio::time::timeout(RESOURCE_TIMEOUT, work)
        .await
        .unwrap_or_else(|_| dropped("timeout"))
}

/// The request itself: status, capped body, type check.
async fn download(
    client: &Client,
    profile: &'static BrowserProfile,
    url: &str,
    referer: &str,
    kind: ResourceKind,
    cap: usize,
) -> ResolvedResource {
    let response = match client
        .get(url)
        .headers(identity::browser_headers(profile, Some(referer)))
        .header(ACCEPT, kind.accept())
        .send()
        .await
    {
        Ok(response) => response,
        Err(e) if e.is_redirect() => return dropped("blocked_host"),
        Err(e) if e.is_timeout() => return dropped("timeout"),
        Err(_) => return dropped("network"),
    };
    if !response.status().is_success() {
        return dropped(&format!("http_{}", response.status().as_u16()));
    }
    let declared = content_type(&response);
    match read_capped(response, cap).await {
        Ok(bytes) => accept_body(kind, declared.as_deref(), bytes),
        Err(ReadError::TooLarge) => dropped("too_large"),
        Err(ReadError::Network(_)) => dropped("network"),
    }
}

/// The `Content-Type` header, if it is valid text.
pub(super) fn content_type(response: &reqwest::Response) -> Option<String> {
    response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

/// Why a body could not be read.
#[derive(Debug)]
pub(super) enum ReadError {
    TooLarge,
    Network(String),
}

/// Read `response` as a stream, refusing it once it passes `cap` decoded
/// bytes. A declared length over the cap refuses before reading.
pub(super) async fn read_capped(
    response: reqwest::Response,
    cap: usize,
) -> Result<Vec<u8>, ReadError> {
    if response
        .content_length()
        .is_some_and(|length| length > cap as u64)
    {
        return Err(ReadError::TooLarge);
    }
    let mut response = response;
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| ReadError::Network(e.to_string()))?
    {
        if body.len().saturating_add(chunk.len()) > cap {
            return Err(ReadError::TooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// Decode a `data:` URL under the same cap and type allowlist as a fetch.
pub(super) fn resolve_data_url(url: &str, kind: ResourceKind, cap: usize) -> ResolvedResource {
    let Some((meta, payload)) = url[5..].split_once(',') else {
        return dropped("bad_type");
    };
    let is_base64 = meta.trim_end().to_ascii_lowercase().ends_with(";base64");
    // Cheap refusal before decoding: base64 is 4/3 of the bytes, percent-encoding at most 3x.
    let ceiling = if is_base64 {
        cap / 3 * 4 + 8
    } else {
        cap.saturating_mul(3)
    };
    if payload.len() > ceiling {
        return dropped("too_large");
    }
    let bytes = if is_base64 {
        match decode_base64(payload) {
            Some(bytes) => bytes,
            None => return dropped("bad_type"),
        }
    } else {
        urlencoding::decode_binary(payload.as_bytes()).into_owned()
    };
    if bytes.len() > cap {
        return dropped("too_large");
    }
    let mime = meta.split(';').next().unwrap_or_default().trim();
    accept_body(kind, Some(mime), bytes)
}

fn decode_base64(payload: &str) -> Option<Vec<u8>> {
    let compact: String = payload
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect();
    let compact = urlencoding::decode(&compact).ok()?.into_owned();
    let engines = [
        base64::engine::general_purpose::STANDARD,
        base64::engine::general_purpose::STANDARD_NO_PAD,
    ];
    engines
        .iter()
        .find_map(|engine| engine.decode(compact.as_bytes()).ok())
}

/// The type allowlist of T11. Images: `image/*` (SVG included: every place a
/// snapshot shows an image, `<img>`, `poster`, SVG `<image>` and CSS, loads
/// it as an image, which cannot run script). Fonts: `font/*`,
/// `application/font-*`, `application/x-font-*`. Stylesheets: `text/css`,
/// decoded to UTF-8. A body that starts like markup is never an image or a
/// font, whatever its header says.
pub(super) fn accept_body(
    kind: ResourceKind,
    declared: Option<&str>,
    bytes: Vec<u8>,
) -> ResolvedResource {
    let mime = mime_essence(declared);
    let markup = looks_like_markup(&bytes);
    let accepted = match kind {
        ResourceKind::Image | ResourceKind::Favicon => (mime.starts_with("image/")
            && (mime == "image/svg+xml" || !markup))
            .then(|| mime.clone()),
        ResourceKind::Font if markup => None,
        ResourceKind::Font if is_font_mime(&mime) => Some(mime.clone()),
        ResourceKind::Font => sniff_font(&mime, &bytes).map(str::to_string),
        ResourceKind::Stylesheet => {
            if mime != CSS_MIME {
                return dropped("bad_type");
            }
            let text = decode_css(&bytes, declared);
            return ResolvedResource::Inlined(DataUri {
                mime: CSS_MIME.to_string(),
                bytes: text.into_bytes(),
            });
        }
    };
    match accepted.filter(|mime| is_safe_mime_token(mime)) {
        Some(mime) => ResolvedResource::Inlined(DataUri { mime, bytes }),
        None => dropped("bad_type"),
    }
}

/// `type/subtype` lowercased, parameters removed; empty when absent.
pub(super) fn mime_essence(declared: Option<&str>) -> String {
    declared
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

fn is_font_mime(mime: &str) -> bool {
    mime.starts_with("font/")
        || mime.starts_with("application/font-")
        || mime.starts_with("application/x-font-")
}

/// Fonts served as `application/octet-stream` (or with no type) are common
/// on CDNs; accept them only when the bytes carry a font signature.
fn sniff_font(mime: &str, bytes: &[u8]) -> Option<&'static str> {
    if !matches!(
        mime,
        "" | "application/octet-stream" | "binary/octet-stream"
    ) {
        return None;
    }
    match bytes.get(..4)? {
        b"wOF2" => Some("font/woff2"),
        b"wOFF" => Some("font/woff"),
        b"OTTO" => Some("font/otf"),
        [0, 1, 0, 0] | b"true" => Some("font/ttf"),
        _ => None,
    }
}

/// Only characters that are safe inside a quoted `data:` URL in CSS and HTML.
fn is_safe_mime_token(mime: &str) -> bool {
    mime.contains('/')
        && mime
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'+' | b'-' | b'_'))
}

/// Whether a body begins (after a BOM and whitespace) with `<`.
fn looks_like_markup(bytes: &[u8]) -> bool {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    bytes.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'<')
}

/// The `charset` parameter of a `Content-Type` value.
pub(super) fn charset_label(content_type: Option<&str>) -> Option<&'static Encoding> {
    content_type?.split(';').skip(1).find_map(|parameter| {
        let (name, value) = parameter.trim().split_once('=')?;
        name.trim()
            .eq_ignore_ascii_case("charset")
            .then(|| Encoding::for_label(value.trim().trim_matches(['"', '\'']).as_bytes()))
            .flatten()
    })
}

/// Decode a stylesheet: BOM, then the header charset, then a leading
/// `@charset "…";`, else UTF-8 (lossy).
fn decode_css(bytes: &[u8], declared: Option<&str>) -> String {
    let at_charset = || {
        let head = bytes.strip_prefix(b"@charset \"")?;
        let end = head.iter().take(40).position(|b| *b == b'"')?;
        Encoding::for_label(head.get(..end)?)
    };
    let encoding = charset_label(declared).or_else(at_charset).unwrap_or(UTF_8);
    encoding.decode(bytes).0.into_owned()
}

/// Mutable bookkeeping shared by the concurrent fetches of one capture.
#[derive(Default)]
struct FetchState {
    resolved: ResolvedResources,
    admitted: HashSet<String>,
    bytes: u64,
    done: u32,
    reasons: BTreeSet<String>,
}

/// What the fetch step produced.
pub(super) struct FetchOutcome {
    pub resources: ResolvedResources,
    pub partial_reasons: Vec<String>,
}

/// Fetches a capture's subresources under its caps.
pub(super) struct Fetcher<'a> {
    client: &'a Client,
    profile: &'static BrowserProfile,
    /// The final document URL, sent as `Referer`.
    referer: String,
    progress: ProgressSink,
    permits: Semaphore,
    state: Mutex<FetchState>,
}

impl<'a> Fetcher<'a> {
    pub(super) fn new(
        client: &'a Client,
        profile: &'static BrowserProfile,
        referer: String,
        progress: ProgressSink,
    ) -> Self {
        Self {
            client,
            profile,
            referer,
            progress,
            permits: Semaphore::new(RESOURCE_CONCURRENCY),
            state: Mutex::new(FetchState::default()),
        }
    }

    /// Fetch `refs` and, level by level, what their stylesheets reference,
    /// until nothing is left or `deadline` passes. What was not resolved by
    /// then is `timeout`.
    pub(super) async fn fetch_all(
        &self,
        refs: Vec<ResourceRef>,
        deadline: Instant,
    ) -> FetchOutcome {
        let mut level = self.admit(refs);
        while !level.is_empty() {
            match tokio::time::timeout_at(deadline, self.fetch_level(level)).await {
                Ok(next) => level = self.admit(next),
                Err(_) => break,
            }
        }
        self.finish()
    }

    /// The favicon, if it is an image of at most 16 KiB; never inlined.
    pub(super) async fn fetch_favicon(
        &self,
        url: Option<String>,
        deadline: Instant,
    ) -> Option<DataUri> {
        let url = url?;
        let fetch = fetch_resource(
            self.client,
            self.profile,
            &url,
            &self.referer,
            ResourceKind::Favicon,
            MAX_FAVICON_BYTES,
        );
        match tokio::time::timeout_at(deadline, fetch).await {
            Ok(ResolvedResource::Inlined(data)) => Some(data),
            _ => None,
        }
    }

    /// Keep the refs not seen before, refusing those past the resource count.
    fn admit(&self, refs: Vec<ResourceRef>) -> Vec<ResourceRef> {
        let mut state = self.lock();
        let mut admitted = Vec::new();
        for resource in refs {
            if state.admitted.contains(&resource.url) || state.resolved.get(&resource.url).is_some()
            {
                continue;
            }
            if state.admitted.len() >= MAX_RESOURCES as usize {
                state.resolved.insert(resource.url, dropped("over_budget"));
                state.reasons.insert("over_budget".to_string());
                continue;
            }
            state.admitted.insert(resource.url.clone());
            admitted.push(resource);
        }
        admitted
    }

    /// Fetch one level concurrently; returns what its stylesheets reference.
    async fn fetch_level(&self, level: Vec<ResourceRef>) -> Vec<ResourceRef> {
        let nested = join_all(level.into_iter().map(|resource| self.fetch_one(resource))).await;
        nested.into_iter().flatten().collect()
    }

    async fn fetch_one(&self, resource: ResourceRef) -> Vec<ResourceRef> {
        let _permit = self.permits.acquire().await.ok();
        let exhausted = self.lock().bytes >= MAX_TOTAL_BYTES;
        let resolved = if exhausted {
            dropped("over_budget")
        } else {
            fetch_resource(
                self.client,
                self.profile,
                &resource.url,
                &self.referer,
                resource.kind,
                MAX_RESOURCE_BYTES,
            )
            .await
        };
        let nested = self.record(&resource, resolved);
        self.tick();
        nested
    }

    /// Store an outcome under the byte budget; a stylesheet yields its references.
    fn record(&self, resource: &ResourceRef, resolved: ResolvedResource) -> Vec<ResourceRef> {
        let mut state = self.lock();
        let mut nested = Vec::new();
        let resolved = match resolved {
            ResolvedResource::Inlined(data)
                if state.bytes + data.bytes.len() as u64 > MAX_TOTAL_BYTES =>
            {
                dropped("over_budget")
            }
            ResolvedResource::Inlined(data) => {
                state.bytes += data.bytes.len() as u64;
                if resource.kind == ResourceKind::Stylesheet {
                    nested = sheet_refs(resource, &data, &mut state.reasons);
                }
                ResolvedResource::Inlined(data)
            }
            other => other,
        };
        if let ResolvedResource::Dropped { reason } = &resolved {
            if matches!(reason.as_str(), "too_large" | "over_budget") {
                state.reasons.insert(reason.clone());
            }
        }
        state.resolved.insert(resource.url.clone(), resolved);
        nested
    }

    /// Count a finished resource and report every fifth.
    fn tick(&self) {
        let (done, total) = {
            let mut state = self.lock();
            state.done += 1;
            (state.done, state.admitted.len() as u32)
        };
        if done % 5 == 0 {
            (self.progress)(CaptureProgress {
                done,
                total,
                message: format!("Saved {done} of {total} resources"),
            });
        }
    }

    /// Admitted URLs with no outcome ran out of time.
    fn finish(&self) -> FetchOutcome {
        let mut state = self.lock();
        let pending: Vec<String> = state
            .admitted
            .iter()
            .filter(|url| state.resolved.get(url).is_none())
            .cloned()
            .collect();
        if !pending.is_empty() {
            state.reasons.insert("timeout".to_string());
        }
        for url in pending {
            state.resolved.insert(url, dropped("timeout"));
        }
        FetchOutcome {
            resources: std::mem::take(&mut state.resolved),
            partial_reasons: state.reasons.iter().cloned().collect(),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, FetchState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The references inside a fetched stylesheet, resolved against its own URL.
/// `@import` below `MAX_IMPORT_DEPTH` is not followed (and marks the capture partial).
fn sheet_refs(
    sheet: &ResourceRef,
    data: &DataUri,
    reasons: &mut BTreeSet<String>,
) -> Vec<ResourceRef> {
    let Ok(base) = Url::parse(&sheet.url) else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&data.bytes);
    let mut refs = Vec::new();
    for found in css::collect_refs(&text) {
        let (kind, depth) = match found.kind {
            CssRefKind::Import if sheet.depth >= MAX_IMPORT_DEPTH => {
                reasons.insert("import_depth".to_string());
                continue;
            }
            CssRefKind::Import => (ResourceKind::Stylesheet, sheet.depth + 1),
            CssRefKind::Font => (ResourceKind::Font, sheet.depth),
            CssRefKind::Image => (ResourceKind::Image, sheet.depth),
        };
        if let Ok(url) = resolve_reference(&base, &found.url) {
            refs.push(ResourceRef { url, kind, depth });
        }
    }
    refs
}
