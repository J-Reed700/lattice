//! The capture engine behind frozen web snapshots: URL in, one self-contained
//! sanitized HTML document out. Design: `docs/design/2026-10-13-frozen-web-snapshots.md` §6.
//!
//! - [`capture`]: the HTTP capture flow (document fetch, the two passes, output).
//! - [`sanitize`]: the rule table both passes read, pass 1 (collect) and pass 2 (rewrite).
//! - [`css`]: the CSS tokenizer that collects and rewrites `url()` and `@import`.
//! - [`resources`]: subresource fetching under the caps of §5 T4 and the type allowlist of T11.
//! - [`clutter`]: the capture-time ad removal list of §6.6.
//!
//! `build_page_capture` is the single constructor dependency injection calls.

use std::sync::Arc;
use std::time::Duration;

use crate::features::web::services::web::public_web_client;
use crate::features::web::traits::page_snapshot::PageCapturePort;
use lattice_core::shared::error::Result;

mod capture;
mod clutter;
mod css;
mod resources;
mod sanitize;

pub use sanitize::synthesize_archival_html;

#[cfg(test)]
mod tests;

/// The document is read as a stream and refused beyond this (§6.2).
const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;
/// One subresource, decoded bytes (T4).
const MAX_RESOURCE_BYTES: usize = 8 * 1024 * 1024;
/// All inlined subresources of one snapshot together (T4).
const MAX_TOTAL_BYTES: u64 = 48 * 1024 * 1024;
/// Distinct subresources per snapshot (T4).
const MAX_RESOURCES: u32 = 400;
/// The whole capture (T4).
const CAPTURE_DEADLINE: Duration = Duration::from_secs(60);
/// One subresource, request and body (T4).
const RESOURCE_TIMEOUT: Duration = Duration::from_secs(15);
/// Subresource requests in flight at once (T4).
const RESOURCE_CONCURRENCY: usize = 8;
/// `@import` nesting below a document stylesheet (T4).
const MAX_IMPORT_DEPTH: u8 = 3;
/// A favicon larger than this is not kept (§6.3).
const MAX_FAVICON_BYTES: usize = 16 * 1024;
/// An `img` `src` that is a `data:` URL shorter than this is a lazy-loading placeholder (§6.3).
const PLACEHOLDER_DATA_URL_LEN: usize = 1024;
/// Emitted inline bytes (base64 `data:` URLs and stylesheet text, counted per
/// use) beyond which further inlining is dropped: the stored budget in base64
/// plus room for a sheet or image referenced more than once.
const MAX_EMITTED_BYTES: usize = 96 * 1024 * 1024;

/// The production capture engine: plain HTTP through the guarded public web client.
pub fn build_page_capture() -> Result<Arc<dyn PageCapturePort>> {
    Ok(Arc::new(
        capture::HttpPageCapture::new(public_web_client()?),
    ))
}
