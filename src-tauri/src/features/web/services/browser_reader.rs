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
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Deserialize;
use tauri::{AppHandle, WebviewUrl, WebviewWindowBuilder, Wry};
use tokio::sync::{oneshot, Semaphore};
use tracing::{debug, info, warn};
use url::{Host, Url};

/// The reserved address the page reports back through. `.invalid` can never
/// resolve (RFC 2606), so a report that somehow escaped would go nowhere.
const REPORT_HOST: &str = "lattice-page-reader.invalid";

/// How long one read may take, challenge included. A Cloudflare check clears
/// in a few seconds when it clears at all.
pub const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(25);

/// Browser windows open at once. Each is a whole web process; a deep-research
/// round that hits several blocked sites should not open a dozen.
const MAX_CONCURRENT_READS: usize = 2;

static APP: OnceLock<AppHandle<Wry>> = OnceLock::new();
static SLOTS: Semaphore = Semaphore::const_new(MAX_CONCURRENT_READS);
static NEXT_LABEL: AtomicU64 = AtomicU64::new(1);

/// Give the reader the running app. Until this is called — in tests, or before
/// setup — every read fails fast and callers keep the HTTP result.
pub fn install(app: AppHandle<Wry>) {
    let _ = APP.set(app);
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

#[derive(Deserialize)]
struct Report {
    url: String,
    title: String,
    text: String,
    /// The page was still behind its check when the script gave up: one that
    /// wants a person to click, which the reader does not do.
    #[serde(default)]
    blocked: bool,
}

/// Load `url` in a hidden browser window and return its text.
///
/// Fails when no app is installed, the URL is not a public web address, the
/// page never gets past its check within `timeout`, or the window cannot be
/// created. The caller decides what a failure means; nothing is cached here.
pub async fn read(url: &str, timeout: Duration) -> Result<BrowserPage, String> {
    let app = APP
        .get()
        .ok_or_else(|| "no browser is available to read pages".to_string())?;
    let target = Url::parse(url).map_err(|e| format!("not a URL: {e}"))?;
    if !navigation_allowed(&target) {
        return Err("not a public web address".to_string());
    }

    let _slot = SLOTS
        .acquire()
        .await
        .map_err(|_| "the page reader is shutting down".to_string())?;

    let label = format!(
        "{LABEL_PREFIX}{}",
        NEXT_LABEL.fetch_add(1, Ordering::Relaxed)
    );
    let (sender, receiver) = oneshot::channel::<Report>();
    let sender = Arc::new(Mutex::new(Some(sender)));

    let reporter = Arc::clone(&sender);
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
                if let Some(report) = decode_report(next) {
                    if let Some(sender) = reporter.lock().ok().and_then(|mut slot| slot.take()) {
                        let _ = sender.send(report);
                    }
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
        .map_err(|e| format!("could not open a browser window: {e}"))?;

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
                _ = ticker.tick() => {
                    let elapsed = started.elapsed();
                    let script = check_script(elapsed >= GIVE_UP_ON_CHECK, elapsed >= SETTLE_LIMIT);
                    if let Err(error) = window.eval(script) {
                        debug!(%error, "Page reader could not run its check");
                    }
                }
            }
        }
    };
    let outcome = tokio::time::timeout(timeout, poll).await;
    if let Err(error) = window.destroy() {
        warn!(%error, label, "Page reader window could not be destroyed");
    }
    drop(sender);

    let report = match outcome {
        Ok(Ok(report)) => report,
        Ok(Err(_)) => return Err("the browser window closed before the page settled".into()),
        Err(_) => {
            return Err(format!(
                "the page did not get past its check within {}s",
                timeout.as_secs()
            ))
        }
    };
    if report.blocked {
        return Err("the page is behind a check that asks a person to click".into());
    }
    info!(
        url,
        final_url = report.url.as_str(),
        chars = report.text.len(),
        "Page read through the browser"
    );
    Ok(BrowserPage {
        final_url: report.url,
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
        Some(Host::Ipv4(ip)) => {
            !(ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || ip.is_documentation())
        }
        Some(Host::Ipv6(ip)) => {
            let segments = ip.segments();
            !(ip.is_loopback()
                || ip.is_unspecified()
                // Unique local (fc00::/7) and link-local (fe80::/10).
                || (segments[0] & 0xfe00) == 0xfc00
                || (segments[0] & 0xffc0) == 0xfe80
                || ip.to_ipv4_mapped().is_some())
        }
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
}

const CHECK_SCRIPT: &str = r#"
(() => {
  if (!/^https?:$/.test(location.protocol) || document.readyState === 'loading') return;
  const state = window.latticePageReaderState = window.latticePageReaderState || { last: -1, steady: 0, sent: false };
  if (state.sent) return;

  const report = (blocked, text) => {
    state.sent = true;
    const payload = JSON.stringify({ url: location.href, title: document.title || '', text, blocked });
    location.href = 'https://lattice-page-reader.invalid/#' + encodeURIComponent(payload);
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
  report(false, parts.join('\n\n').slice(0, 60000));
})();
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed(url: &str) -> bool {
        navigation_allowed(&Url::parse(url).unwrap())
    }

    #[test]
    fn only_the_public_web_is_reachable() {
        assert!(allowed(
            "https://www.thespruce.com/best-houseplants-for-sun-4147670"
        ));
        assert!(allowed("http://example.com/"));
        assert!(allowed("https://93.184.216.34/"));
        assert!(allowed("about:blank"));
        assert!(allowed("about:srcdoc"));

        for blocked in [
            "file:///etc/passwd",
            "ftp://example.com/",
            "tauri://localhost/",
            "javascript:alert(1)",
            "http://localhost:1420/",
            "http://app.localhost/",
            "http://127.0.0.1:8080/",
            "http://10.0.0.1/",
            "http://192.168.1.1/",
            "http://169.254.169.254/latest/meta-data/",
            "http://[::1]/",
            "http://[fd00::1]/",
            "http://[::ffff:127.0.0.1]/",
            "http://printer.local/",
            "http://router/",
        ] {
            assert!(!allowed(blocked), "{blocked} should be refused");
        }
    }

    #[test]
    fn a_report_is_decoded_from_the_fragment() {
        let payload = serde_json::json!({
            "url": "https://example.com/a",
            "title": "A — page",
            "text": "First paragraph.\n\nSecond, with #hash & ünïcode."
        })
        .to_string();
        let url = Url::parse(&format!(
            "https://{REPORT_HOST}/#{}",
            urlencoding::encode(&payload)
        ))
        .unwrap();
        let report = decode_report(&url).expect("decodes");
        assert_eq!(report.url, "https://example.com/a");
        assert_eq!(report.title, "A — page");
        assert!(report.text.contains("ünïcode"));
    }

    #[test]
    fn the_check_script_carries_its_deadlines_and_no_placeholders() {
        let early = check_script(false, false);
        assert!(early.contains("if (false) report(true, '')"));
        let late = check_script(true, true);
        assert!(late.contains("if (true) report(true, '')"));
        assert!(!late.contains("__"));
        assert!(late.contains(REPORT_HOST));
    }

    #[tokio::test]
    async fn without_an_app_a_read_fails_at_once() {
        let error = read("https://example.com/", Duration::from_secs(1))
            .await
            .unwrap_err();
        assert!(error.contains("no browser"));
    }
}
