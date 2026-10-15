//! Reading a page through a real, hidden browser window.
//!
//! Some sites answer a plain HTTP client with a JavaScript check (Cloudflare's
//! "Just a moment…") or ship an empty shell that scripts fill in. No choice of
//! headers gets past either: the page has to run. The app already contains a
//! browser engine — the system webview Tauri draws its own UI with — so a page
//! that refuses the HTTP client is loaded once more in an invisible window of
//! that engine, on the user's machine, exactly as if they had opened it.
//! Nothing is proxied or hosted; the request leaves from where a normal visit
//! would.
//!
//! # How the text gets back
//!
//! Tauri's `eval` returns nothing, and granting a remote page the app's IPC
//! would let any site we read call into Lattice. So the page never talks to the
//! app. An injected script waits for the page to settle, pulls out its text,
//! and navigates to a reserved `.invalid` address carrying that text in the
//! fragment. The window's navigation hook recognizes the address, takes the
//! text, and cancels the navigation — it never reaches a network.
//!
//! # Containment
//!
//! - The window is labelled outside every capability (capabilities name the
//!   `main` window only), so the page has no access to app commands.
//! - It is incognito: no cookies or storage outlive the read, and nothing it
//!   stores mixes with any other page's.
//! - Navigations to non-web schemes and to local or private hosts are refused,
//!   so a redirect cannot turn the reader against the user's own network.
//! - Downloads are refused, the window is never shown, and it is destroyed
//!   when the read ends, however it ends.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Deserialize;
use tauri::{AppHandle, WebviewUrl, WebviewWindowBuilder, Wry};
use tokio::sync::{oneshot, Semaphore};
use tracing::{debug, info, warn};
use url::{Host, Url};

use super::web::is_blocked_ip;
use lattice_core::shared::error::{AppError, Result};
use std::net::IpAddr;

/// The reserved address the page reports back through. `.invalid` can never
/// resolve (RFC 2606), so a report that somehow escaped would go nowhere.
const REPORT_HOST: &str = "lattice-page-reader.invalid";

/// How long one read may take, challenge included. A Cloudflare check clears
/// in a few seconds when it clears at all.
pub const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(25);

/// Browser windows open at once. Each is a whole web process; a deep-research
/// round that hits several blocked sites should not open a dozen.
const MAX_CONCURRENT_READS: usize = 2;

/// Reads pages in hidden windows of the running app's browser engine.
///
/// One per app: the composition root builds it once the app exists and hands
/// it to the web service. A web service without one — in tests, or before
/// setup — fails every browser read fast and keeps the HTTP result.
pub struct BrowserReader {
    app: AppHandle<Wry>,
    /// Bounds the reader windows open at once.
    slots: Semaphore,
    /// Numbers the reader's windows: a label must be unique within its app.
    next_label: AtomicU64,
}

/// Label prefix of every reader window.
const LABEL_PREFIX: &str = "page-reader-";

/// Whether `label` names one of the reader's hidden windows. They are not the
/// user's windows: the app quits when the last *user* window closes, whatever
/// reads are still running.
pub fn is_reader_window(label: &str) -> bool {
    label.starts_with(LABEL_PREFIX)
}

/// A page as the browser rendered it.
#[derive(Debug, Clone)]
pub struct BrowserPage {
    /// Where the browser ended up, after redirects and the challenge.
    pub final_url: String,
    pub title: Option<String>,
    /// Paragraph and heading text, chosen as the HTTP extractor chooses it.
    pub text: String,
}

/// What the page's script posts back. It carries text only: the page writes
/// it, so anything it says about *where* it is could be a lie. A page that
/// claimed another URL here would have its text cached under that URL and
/// served as that site's in later citations.
#[derive(Deserialize)]
struct Report {
    title: String,
    text: String,
    /// The page was still behind its check when the script gave up: one that
    /// wants a person to click, which the reader does not do.
    #[serde(default)]
    blocked: bool,
    /// Which part of the text this is, of `parts`. A report travels in a URL,
    /// and a whole page's text can be longer than one URL may be, so it is
    /// sent in parts and put back together here.
    #[serde(default)]
    part: usize,
    #[serde(default = "one_part")]
    parts: usize,
    /// Which reading of the page the part belongs to. A page that replaces
    /// its document mid-transfer starts a new reading, and parts of two
    /// different texts must never be joined.
    #[serde(default)]
    snapshot: String,
}

fn one_part() -> usize {
    1
}

/// Characters in one part of a report, as the check script cuts them (UTF-16
/// units there, so never more characters than this here).
const PART_CHARS: usize = 100_000;

/// The most parts a report may claim: 16 Mi characters of page text, about
/// eight thousand printed pages. The page writes the part count, so it is
/// checked before anything is kept; a page claiming more is refused, and the
/// read says why, rather than the app trying to hold what it claims.
const MAX_REPORT_PARTS: usize = (16 * 1024 * 1024usize).div_ceil(PART_CHARS);

/// The parts of one report received so far. Parts may arrive more than once
/// (the page sends them round until the window closes) and in any order.
/// Storage grows with the parts that arrive, never with the count a part
/// claims.
#[derive(Default)]
struct Assembly {
    snapshot: String,
    parts: usize,
    received: std::collections::BTreeMap<usize, String>,
    title: String,
    blocked: bool,
    /// The part count of a report refused as larger than the reader takes.
    refused: Option<usize>,
}

impl Assembly {
    /// Add `report`'s part; the whole report once every part is in.
    fn add(&mut self, report: Report) -> Option<Report> {
        let parts = report.parts.max(1);
        if parts > MAX_REPORT_PARTS {
            self.refused = Some(parts);
            return None;
        }
        if report.part >= parts || report.text.chars().count() > PART_CHARS {
            return None;
        }
        // A new reading of the page, or a different cut of it: start over.
        if self.snapshot != report.snapshot || self.parts != parts {
            *self = Self {
                snapshot: report.snapshot.clone(),
                parts,
                ..Self::default()
            };
        }
        self.title = report.title;
        self.blocked = report.blocked;
        self.received.entry(report.part).or_insert(report.text);
        if self.received.len() < self.parts {
            return None;
        }
        Some(self.take(false))
    }

    /// What has arrived when time runs out: the parts in order, each missing
    /// one marked where it belongs. `None` when nothing has.
    fn partial(&mut self) -> Option<Report> {
        (!self.received.is_empty()).then(|| self.take(true))
    }

    fn take(&mut self, mark_gaps: bool) -> Report {
        let parts = self.parts;
        let mut received = std::mem::take(&mut self.received);
        let mut text = String::new();
        for index in 0..parts {
            match received.remove(&index) {
                Some(part) => text.push_str(&part),
                None if mark_gaps => text.push_str(&format!(
                    "\n\n[Part {} of {parts} of this page's text did not arrive before the \
                     reader's time ran out.]\n\n",
                    index + 1
                )),
                None => {}
            }
        }
        Report {
            title: std::mem::take(&mut self.title),
            text,
            blocked: self.blocked,
            part: 0,
            parts: 1,
            snapshot: std::mem::take(&mut self.snapshot),
        }
    }
}

/// Why a read fails when the web service has no [`BrowserReader`].
pub fn unavailable() -> AppError {
    AppError::ServiceNotAvailable("no browser is available to read pages".to_string())
}

impl BrowserReader {
    /// The reader for the running `app`.
    pub fn new(app: AppHandle<Wry>) -> Self {
        Self {
            app,
            slots: Semaphore::new(MAX_CONCURRENT_READS),
            next_label: AtomicU64::new(1),
        }
    }

    /// Load `url` in a hidden browser window and return its text.
    ///
    /// Fails when the URL is not a public web address, the page never gets past
    /// its check within `timeout`, or the window cannot be created. The caller
    /// decides what a failure means; nothing is cached here.
    pub async fn read(&self, url: &str, timeout: Duration) -> Result<BrowserPage> {
        let app = &self.app;
        let target =
            Url::parse(url).map_err(|e| AppError::InvalidUrl(format!("not a URL: {e}")))?;
        if !navigation_allowed(&target) {
            return Err(AppError::Security("not a public web address".to_string()));
        }

        let _slot = self.slots.acquire().await.map_err(|_| {
            AppError::ServiceNotAvailable("the page reader is shutting down".to_string())
        })?;

        let label = format!(
            "{LABEL_PREFIX}{}",
            self.next_label.fetch_add(1, Ordering::Relaxed)
        );
        let (sender, receiver) = oneshot::channel::<Report>();
        let sender = Arc::new(Mutex::new(Some(sender)));

        let reporter = Arc::clone(&sender);
        let assembly = Arc::new(Mutex::new(Assembly::default()));
        let collector = Arc::clone(&assembly);
        // A part in, more to come: look again now rather than at the next
        // tick, so a long page is not read at one part per poll.
        let part_arrived = Arc::new(tokio::sync::Notify::new());
        let arrival = Arc::clone(&part_arrived);
        let window = WebviewWindowBuilder::new(app, &label, WebviewUrl::External(target))
            .title("Lattice page reader")
            .visible(false)
            .focused(false)
            .skip_taskbar(true)
            .inner_size(1280.0, 900.0)
            .incognito(true)
            .on_navigation(move |next| {
                if next.host_str() == Some(REPORT_HOST) {
                    debug!(chars = next.as_str().len(), "Page reader report arrived");
                    let whole = decode_report(next).and_then(|part| {
                        collector.lock().ok().and_then(|mut parts| parts.add(part))
                    });
                    match whole {
                        Some(report) => {
                            if let Some(sender) =
                                reporter.lock().ok().and_then(|mut slot| slot.take())
                            {
                                let _ = sender.send(report);
                            }
                        }
                        None => arrival.notify_one(),
                    }
                    return false;
                }
                let allowed = navigation_allowed(next);
                debug!(url = %next, allowed, "Page reader navigation");
                allowed
            })
            .on_page_load(|_, payload| {
                debug!(url = %payload.url(), event = ?payload.event(), "Page reader load");
            })
            .on_download(|_, _| false)
            .build()
            .map_err(|e| {
                AppError::InternalError(format!("could not open a browser window: {e}"))
            })?;

        // The check is driven from here, not from timers inside the page: a
        // challenge page replaces its document mid-check, and anything injected
        // once at load is thrown away with it.
        let started = Instant::now();
        let poll = async {
            let mut receiver = receiver;
            let mut ticker = tokio::time::interval(POLL_INTERVAL);
            loop {
                tokio::select! {
                    report = &mut receiver => return report,
                    _ = ticker.tick() => {}
                    () = part_arrived.notified() => {}
                }
                let elapsed = started.elapsed();
                let script = check_script(elapsed >= GIVE_UP_ON_CHECK, elapsed >= SETTLE_LIMIT);
                if let Err(error) = window.eval(script) {
                    debug!(%error, "Page reader could not run its check");
                }
            }
        };
        let outcome = tokio::time::timeout(timeout, poll).await;
        // Where the page is, from the browser rather than from the page. The
        // report navigation was cancelled, so the window still shows the page
        // that sent it; history.pushState can move within its origin only.
        let landed = window.url().ok();
        if let Err(error) = window.destroy() {
            warn!(%error, label, "Page reader window could not be destroyed");
        }
        drop(sender);

        let report = match outcome {
            Ok(Ok(report)) => report,
            Ok(Err(_)) => {
                return Err(AppError::InternalError(
                    "the browser window closed before the page settled".to_string(),
                ))
            }
            Err(_) => {
                let mut assembly = assembly
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if let Some(parts) = assembly.refused {
                    return Err(AppError::Network(format!(
                        "the page reported its text in {parts} parts, more than the \
                         {MAX_REPORT_PARTS} the reader takes in"
                    )));
                }
                // Time ran out partway through a long page: what arrived is
                // kept, with each missing part marked, rather than thrown away.
                match assembly.partial() {
                    Some(report) => report,
                    None => {
                        return Err(AppError::Network(format!(
                            "the page did not get past its check within {}s",
                            timeout.as_secs()
                        )))
                    }
                }
            }
        };
        let page = page_from_report(report, landed)?;
        info!(
            url,
            final_url = page.final_url.as_str(),
            chars = page.text.len(),
            "Page read through the browser"
        );
        Ok(page)
    }
}

/// The page read, located at `landed`, the webview's own URL. No page is
/// returned when the browser cannot say where it is or it is somewhere the
/// reader may not go.
fn page_from_report(report: Report, landed: Option<Url>) -> Result<BrowserPage> {
    if report.blocked {
        return Err(AppError::Network(
            "the page is behind a check that asks a person to click".to_string(),
        ));
    }
    let landed = landed.ok_or_else(|| {
        AppError::InternalError("the browser could not say where the page ended up".to_string())
    })?;
    if !matches!(landed.scheme(), "http" | "https") || !navigation_allowed(&landed) {
        return Err(AppError::Security(format!(
            "the page ended up somewhere not allowed: {landed}"
        )));
    }
    Ok(BrowserPage {
        final_url: landed.to_string(),
        title: Some(report.title.trim().to_string()).filter(|t| !t.is_empty()),
        text: report.text,
    })
}

fn decode_report(url: &Url) -> Option<Report> {
    let fragment = url.fragment()?;
    let json = urlencoding::decode(fragment).ok()?;
    serde_json::from_str(&json).ok()
}

/// Whether the reader window may go to `url`: the public web only.
///
/// Checked on every navigation, redirects included. A host name is taken at
/// its word — resolving it here would block the browser's own thread — but the
/// names that mean "this machine" or "this network" are refused outright.
pub(crate) fn navigation_allowed(url: &Url) -> bool {
    match url.scheme() {
        "http" | "https" => {}
        // In-page documents — blank and srcdoc frames, generated content.
        // They never touch a network, and challenge pages are built from them.
        "about" | "data" | "blob" => return true,
        _ => return false,
    }
    match url.host() {
        // The same ranges the HTTP client refuses.
        Some(Host::Ipv4(ip)) => !is_blocked_ip(IpAddr::V4(ip)),
        Some(Host::Ipv6(ip)) => !(ip.to_ipv4_mapped().is_some() || is_blocked_ip(IpAddr::V6(ip))),
        Some(Host::Domain(domain)) => {
            let domain = domain.trim_end_matches('.').to_ascii_lowercase();
            let local = domain == "localhost"
                || [".localhost", ".local", ".internal", ".lan", ".home.arpa"]
                    .iter()
                    .any(|suffix| domain.ends_with(suffix));
            !local && domain.contains('.')
        }
        None => false,
    }
}

/// How often the page is looked at.
const POLL_INTERVAL: Duration = Duration::from_millis(700);
/// A check that clears by itself does so in a few seconds. One still up after
/// this is waiting for a person to click, which the reader does not do.
const GIVE_UP_ON_CHECK: Duration = Duration::from_secs(10);
/// A page whose text never stops changing (a live ticker, an endless feed) is
/// read as it stands after this.
const SETTLE_LIMIT: Duration = Duration::from_secs(15);

/// One look at the page, injected every [`POLL_INTERVAL`]. State lives on the
/// page's `window`, so a replaced document simply starts over.
///
/// Reports once the text has held still for two looks. The block choice
/// mirrors `WebService::extract_article_text`: paragraphs and headings, from
/// `<article>` when that holds at least a third of the page's text, otherwise
/// from the whole page.
fn check_script(give_up_on_check: bool, settle_now: bool) -> String {
    CHECK_SCRIPT
        .replace(
            "__GIVE_UP__",
            if give_up_on_check { "true" } else { "false" },
        )
        .replace("__SETTLE_NOW__", if settle_now { "true" } else { "false" })
        .replace("__PART__", &PART_CHARS.to_string())
}

const CHECK_SCRIPT: &str = r#"
(() => {
  if (!/^https?:$/.test(location.protocol) || document.readyState === 'loading') return;
  const state = window.latticePageReaderState = window.latticePageReaderState || { last: -1, steady: 0, parts: null, next: 0, id: '' };

  // A report travels in a URL, so a long page goes in parts, one per look,
  // round and round until the window closes: a part the app missed comes by
  // again. Each part is whole characters; a cut between the halves of a
  // surrogate pair could not be encoded.
  const PART = __PART__;
  const send = () => {
    const index = state.next;
    state.next = (index + 1) % state.parts.length;
    const payload = JSON.stringify({
      title: state.title, blocked: state.blocked, part: index, parts: state.parts.length, text: state.parts[index],
      snapshot: state.id
    });
    location.href = 'https://lattice-page-reader.invalid/#' + encodeURIComponent(payload);
  };
  if (state.parts) { send(); return; }
  const report = (blocked, text) => {
    const parts = [];
    let start = 0;
    do {
      let end = Math.min(start + PART, text.length);
      if (end < text.length && /[\uDC00-\uDFFF]/.test(text[end])) end -= 1;
      parts.push(text.slice(start, end));
      start = end;
    } while (start < text.length);
    state.parts = parts; state.title = document.title || ''; state.blocked = blocked; state.next = 0;
    state.id = Date.now().toString(36) + Math.random().toString(36).slice(2);
    send();
  };

  const challenged =
    /just a moment|attention required|checking your browser|verify(ing)? you are human|one more step/i
      .test(document.title) ||
    !!document.querySelector(
      '#challenge-form, #challenge-running, #cf-challenge-running, .cf-browser-verification, #cf-please-wait, [name="cf-turnstile-response"]'
    );
  if (challenged) {
    state.last = -1; state.steady = 0;
    if (__GIVE_UP__) report(true, '');
    return;
  }

  const length = (document.body && document.body.textContent || '').length;
  state.steady = length > 0 && length === state.last ? state.steady + 1 : 0;
  state.last = length;
  if (state.steady < 2 && !__SETTLE_NOW__) return;

  const BLOCKS = 'p, h1, h2, h3, h4, h5, h6';
  const clean = (s) => (s || '').replace(/\s+/g, ' ').trim();
  const blocks = (roots) => roots.flatMap((root) =>
    Array.from(root.querySelectorAll(BLOCKS)).map((el) => clean(el.textContent)).filter(Boolean));
  const size = (parts) => parts.reduce((n, p) => n + p.length, 0);
  const article = blocks(Array.from(document.querySelectorAll('article')));
  const page = blocks([document]);
  const parts = article.length > 0 && size(article) * 3 >= size(page) ? article : page;
  report(false, parts.join('\n\n'));
})();
"#;

#[cfg(test)]
mod tests;
