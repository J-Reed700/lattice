//! Stealth HTTP utilities for anti-bot evasion.
//!
//! Provides browser-like request profiles, user-agent rotation, proxy support,
//! random delays, and FlareSolverr integration to avoid detection.
//!
//! # Configuration (Environment Variables)
//!
//! - `RECALL_PROXY_URL`: Single residential proxy URL (e.g., `http://user:pass@proxy:8080`)
//! - `RECALL_PROXY_URLS`: Comma-separated proxy URLs for rotation
//! - `RECALL_FLARESOLVERR_URL`: FlareSolverr endpoint (e.g., `http://localhost:8191`)
//! - `RECALL_STEALTH_MIN_DELAY_MS`: Minimum inter-request delay (default: 500)
//! - `RECALL_STEALTH_MAX_DELAY_MS`: Maximum inter-request delay (default: 2000)

use once_cell::sync::Lazy;
use rand::Rng;
use reqwest::header::{
    HeaderMap, HeaderName, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, CACHE_CONTROL, CONNECTION,
    REFERER, USER_AGENT,
};
use serde::Deserialize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, warn};

/// Consistent browser profile pairing User-Agent with matching Sec-CH-UA headers.
/// Anti-bot systems flag mismatched UA/client-hint combinations.
pub struct BrowserProfile {
    pub user_agent: &'static str,
    pub sec_ch_ua: Option<&'static str>,
    pub sec_ch_ua_mobile: &'static str,
    pub sec_ch_ua_platform: &'static str,
    pub accept: &'static str,
    pub accept_language: &'static str,
}

const CHROME_ACCEPT: &str = "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7";
const FIREFOX_ACCEPT: &str =
    "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8";
const SAFARI_ACCEPT: &str = "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8";

const BROWSER_PROFILES: &[BrowserProfile] = &[
    BrowserProfile {
        user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36",
        sec_ch_ua: Some("\"Chromium\";v=\"122\", \"Not(A:Brand\";v=\"24\", \"Google Chrome\";v=\"122\""),
        sec_ch_ua_mobile: "?0",
        sec_ch_ua_platform: "\"macOS\"",
        accept: CHROME_ACCEPT,
        accept_language: "en-US,en;q=0.9",
    },
    BrowserProfile {
        user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/121.0.0.0 Safari/537.36",
        sec_ch_ua: Some("\"Not A(Brand\";v=\"99\", \"Google Chrome\";v=\"121\", \"Chromium\";v=\"121\""),
        sec_ch_ua_mobile: "?0",
        sec_ch_ua_platform: "\"Windows\"",
        accept: CHROME_ACCEPT,
        accept_language: "en-US,en;q=0.9",
    },
    BrowserProfile {
        user_agent: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36",
        sec_ch_ua: Some("\"Chromium\";v=\"122\", \"Not(A:Brand\";v=\"24\", \"Google Chrome\";v=\"122\""),
        sec_ch_ua_mobile: "?0",
        sec_ch_ua_platform: "\"Linux\"",
        accept: CHROME_ACCEPT,
        accept_language: "en-US,en;q=0.9",
    },
    BrowserProfile {
        user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        sec_ch_ua: Some("\"Not_A Brand\";v=\"8\", \"Chromium\";v=\"120\", \"Google Chrome\";v=\"120\""),
        sec_ch_ua_mobile: "?0",
        sec_ch_ua_platform: "\"Windows\"",
        accept: CHROME_ACCEPT,
        accept_language: "en-US,en;q=0.9",
    },
    BrowserProfile {
        user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:123.0) Gecko/20100101 Firefox/123.0",
        sec_ch_ua: None,
        sec_ch_ua_mobile: "?0",
        sec_ch_ua_platform: "\"macOS\"",
        accept: FIREFOX_ACCEPT,
        accept_language: "en-US,en;q=0.5",
    },
    BrowserProfile {
        user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:123.0) Gecko/20100101 Firefox/123.0",
        sec_ch_ua: None,
        sec_ch_ua_mobile: "?0",
        sec_ch_ua_platform: "\"Windows\"",
        accept: FIREFOX_ACCEPT,
        accept_language: "en-US,en;q=0.5",
    },
    BrowserProfile {
        user_agent: "Mozilla/5.0 (X11; Linux x86_64; rv:123.0) Gecko/20100101 Firefox/123.0",
        sec_ch_ua: None,
        sec_ch_ua_mobile: "?0",
        sec_ch_ua_platform: "\"Linux\"",
        accept: FIREFOX_ACCEPT,
        accept_language: "en-US,en;q=0.5",
    },
    BrowserProfile {
        user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.3 Safari/605.1.15",
        sec_ch_ua: None,
        sec_ch_ua_mobile: "?0",
        sec_ch_ua_platform: "\"macOS\"",
        accept: SAFARI_ACCEPT,
        accept_language: "en-US,en;q=0.9",
    },
    BrowserProfile {
        user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36 Edg/122.0.0.0",
        sec_ch_ua: Some("\"Chromium\";v=\"122\", \"Not(A:Brand\";v=\"24\", \"Microsoft Edge\";v=\"122\""),
        sec_ch_ua_mobile: "?0",
        sec_ch_ua_platform: "\"Windows\"",
        accept: CHROME_ACCEPT,
        accept_language: "en-US,en;q=0.9",
    },
    BrowserProfile {
        user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36 Edg/122.0.0.0",
        sec_ch_ua: Some("\"Chromium\";v=\"122\", \"Not(A:Brand\";v=\"24\", \"Microsoft Edge\";v=\"122\""),
        sec_ch_ua_mobile: "?0",
        sec_ch_ua_platform: "\"macOS\"",
        accept: CHROME_ACCEPT,
        accept_language: "en-US,en;q=0.9",
    },
];

// ─── Profile Rotation ───────────────────────────────────────────────────────

pub struct ProfileRotator {
    counter: AtomicUsize,
}

impl ProfileRotator {
    pub const fn new() -> Self {
        Self {
            counter: AtomicUsize::new(0),
        }
    }

    #[allow(clippy::indexing_slicing)]
    pub fn next_profile(&self) -> &'static BrowserProfile {
        let idx = self.counter.fetch_add(1, Ordering::Relaxed) % BROWSER_PROFILES.len();
        &BROWSER_PROFILES[idx]
    }

    #[allow(clippy::indexing_slicing)]
    pub fn random_profile(&self) -> &'static BrowserProfile {
        let mut rng = rand::thread_rng();
        let idx = rng.gen_range(0..BROWSER_PROFILES.len());
        &BROWSER_PROFILES[idx]
    }
}

static PROFILE_ROTATOR: Lazy<ProfileRotator> = Lazy::new(ProfileRotator::new);
static PROXY_POOL: Lazy<Option<Vec<String>>> = Lazy::new(load_proxy_pool);
static FLARESOLVERR: Lazy<Option<FlareSolverrClient>> = Lazy::new(FlareSolverrClient::from_env);

pub fn next_profile() -> &'static BrowserProfile {
    PROFILE_ROTATOR.next_profile()
}

pub fn random_profile() -> &'static BrowserProfile {
    PROFILE_ROTATOR.random_profile()
}

// ─── Header Builders ────────────────────────────────────────────────────────

/// Full set of browser-like headers for a navigation request.
///
/// When `referer` is supplied, `Sec-Fetch-Site` is set to `cross-site`
/// (mimicking a link click). Otherwise it is `none` (direct navigation).
pub fn browser_headers(profile: &BrowserProfile, referer: Option<&str>) -> HeaderMap {
    let mut headers = HeaderMap::with_capacity(14);

    headers.insert(USER_AGENT, HeaderValue::from_static(profile.user_agent));
    headers.insert(ACCEPT, HeaderValue::from_static(profile.accept));
    headers.insert(
        ACCEPT_LANGUAGE,
        HeaderValue::from_static(profile.accept_language),
    );
    headers.insert(
        HeaderName::from_static("dnt"),
        HeaderValue::from_static("1"),
    );
    headers.insert(CONNECTION, HeaderValue::from_static("keep-alive"));
    headers.insert(
        HeaderName::from_static("upgrade-insecure-requests"),
        HeaderValue::from_static("1"),
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-dest"),
        HeaderValue::from_static("document"),
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-mode"),
        HeaderValue::from_static("navigate"),
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-user"),
        HeaderValue::from_static("?1"),
    );

    if let Some(ch_ua) = profile.sec_ch_ua {
        headers.insert(
            HeaderName::from_static("sec-ch-ua"),
            HeaderValue::from_static(ch_ua),
        );
        headers.insert(
            HeaderName::from_static("sec-ch-ua-mobile"),
            HeaderValue::from_static(profile.sec_ch_ua_mobile),
        );
        headers.insert(
            HeaderName::from_static("sec-ch-ua-platform"),
            HeaderValue::from_static(profile.sec_ch_ua_platform),
        );
    }

    if let Some(ref_url) = referer {
        if let Ok(val) = HeaderValue::from_str(ref_url) {
            headers.insert(REFERER, val);
            headers.insert(
                HeaderName::from_static("sec-fetch-site"),
                HeaderValue::from_static("cross-site"),
            );
        } else {
            headers.insert(
                HeaderName::from_static("sec-fetch-site"),
                HeaderValue::from_static("none"),
            );
        }
    } else {
        headers.insert(
            HeaderName::from_static("sec-fetch-site"),
            HeaderValue::from_static("none"),
        );
    }

    headers
}

/// Search-engine request headers: adds `Cache-Control: no-cache` and uses
/// `same-origin` for Sec-Fetch-Site when a search-engine referer is given.
pub fn search_headers(profile: &BrowserProfile, search_engine_referer: Option<&str>) -> HeaderMap {
    let mut headers = browser_headers(profile, None);

    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert(
        HeaderName::from_static("pragma"),
        HeaderValue::from_static("no-cache"),
    );

    if let Some(ref_url) = search_engine_referer {
        if let Ok(val) = HeaderValue::from_str(ref_url) {
            headers.insert(REFERER, val);
            headers.insert(
                HeaderName::from_static("sec-fetch-site"),
                HeaderValue::from_static("same-origin"),
            );
        }
    }

    headers
}

// ─── Random Delay ───────────────────────────────────────────────────────────

/// Random delay between requests to mimic human browsing cadence.
/// Bounds can be overridden via `RECALL_STEALTH_MIN_DELAY_MS` / `RECALL_STEALTH_MAX_DELAY_MS`.
pub async fn random_delay(default_min_ms: u64, default_max_ms: u64) {
    let delay_ms = {
        let min_ms = std::env::var("RECALL_STEALTH_MIN_DELAY_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default_min_ms);
        let max_ms = std::env::var("RECALL_STEALTH_MAX_DELAY_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default_max_ms)
            .max(min_ms);
        rand::thread_rng().gen_range(min_ms..=max_ms)
    };
    debug!(delay_ms, "Stealth: inter-request delay");
    sleep(Duration::from_millis(delay_ms)).await;
}

// ─── Proxy Pool ─────────────────────────────────────────────────────────────

fn load_proxy_pool() -> Option<Vec<String>> {
    if let Ok(url) = std::env::var("RECALL_PROXY_URL") {
        let url = url.trim().to_string();
        if !url.is_empty() {
            info!("Stealth: loaded single proxy from RECALL_PROXY_URL");
            return Some(vec![url]);
        }
    }
    if let Ok(urls) = std::env::var("RECALL_PROXY_URLS") {
        let proxies: Vec<String> = urls
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !proxies.is_empty() {
            info!(
                count = proxies.len(),
                "Stealth: loaded proxy pool from RECALL_PROXY_URLS"
            );
            return Some(proxies);
        }
    }
    debug!("Stealth: no proxy configured");
    None
}

/// Pick a random proxy URL from the configured pool (if any).
#[allow(clippy::indexing_slicing)]
pub fn random_proxy_url() -> Option<&'static str> {
    PROXY_POOL.as_ref().map(|proxies| {
        let mut rng = rand::thread_rng();
        let idx = rng.gen_range(0..proxies.len());
        proxies[idx].as_str()
    })
}

/// Get the FlareSolverr client (if `RECALL_FLARESOLVERR_URL` is set).
pub fn flaresolverr() -> Option<&'static FlareSolverrClient> {
    FLARESOLVERR.as_ref()
}

// ─── FlareSolverr Client ────────────────────────────────────────────────────

/// Proxy server that solves Cloudflare / JS anti-bot challenges using a real
/// headless browser. See <https://github.com/FlareSolverr/FlareSolverr>.
pub struct FlareSolverrClient {
    endpoint: String,
    client: reqwest::Client,
    timeout_ms: u64,
}

#[derive(Deserialize)]
struct FlareSolverrResponse {
    status: String,
    solution: Option<FlareSolverrSolution>,
}

#[derive(Deserialize)]
struct FlareSolverrSolution {
    response: String,
    #[allow(dead_code)]
    status: u16,
    #[allow(dead_code)]
    url: String,
}

impl FlareSolverrClient {
    fn from_env() -> Option<Self> {
        let endpoint = std::env::var("RECALL_FLARESOLVERR_URL").ok()?;
        let endpoint = endpoint.trim().to_string();
        if endpoint.is_empty() {
            return None;
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .ok()?;

        info!(endpoint = %endpoint, "Stealth: FlareSolverr client initialized");
        Some(Self {
            endpoint,
            client,
            timeout_ms: 60000,
        })
    }

    /// Solve a JS challenge by delegating to FlareSolverr. Returns rendered HTML.
    pub async fn solve(&self, url: &str) -> std::result::Result<String, String> {
        debug!(url, "FlareSolverr: solving challenge");

        let body = serde_json::json!({
            "cmd": "request.get",
            "url": url,
            "maxTimeout": self.timeout_ms
        });

        let response = self
            .client
            .post(&format!("{}/v1", self.endpoint.trim_end_matches('/')))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("FlareSolverr request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("FlareSolverr HTTP {}", response.status()));
        }

        let result: FlareSolverrResponse = response
            .json()
            .await
            .map_err(|e| format!("FlareSolverr parse failed: {}", e))?;

        if result.status != "ok" {
            return Err(format!("FlareSolverr status: {}", result.status));
        }

        result
            .solution
            .map(|s| s.response)
            .ok_or_else(|| "FlareSolverr returned no solution".to_string())
    }
}

// ─── Stealth Client Builder ─────────────────────────────────────────────────

/// Reqwest `ClientBuilder` with stealth features:
/// - Cookie jar for session persistence
/// - Proxy rotation (when configured)
///
/// Does **not** set `User-Agent` — use [`browser_headers`] per-request for rotation.
pub fn stealth_client_builder() -> reqwest::ClientBuilder {
    let mut builder = super::http_client::reqwest_client_builder().cookie_store(true);

    if let Some(proxy_url) = random_proxy_url() {
        match reqwest::Proxy::all(proxy_url) {
            Ok(proxy) => {
                debug!(proxy = %proxy_url, "Stealth: using proxy");
                builder = builder.proxy(proxy);
            }
            Err(e) => {
                warn!(proxy = %proxy_url, error = %e, "Stealth: invalid proxy URL, skipping");
            }
        }
    }

    builder
}

/// Infer the search-engine origin from a request URL for the Referer header.
pub fn search_referer_for_url(url: &str) -> Option<&'static str> {
    if url.contains("duckduckgo.com") {
        Some("https://duckduckgo.com/")
    } else if url.contains("bing.com") {
        Some("https://www.bing.com/")
    } else if url.contains("wikipedia.org") {
        Some("https://en.wikipedia.org/")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_rotation_wraps_around() {
        let rotator = ProfileRotator::new();
        let first = rotator.next_profile().user_agent;
        for _ in 1..BROWSER_PROFILES.len() {
            let _ = rotator.next_profile();
        }
        let wrapped = rotator.next_profile().user_agent;
        assert_eq!(first, wrapped);
    }

    #[test]
    fn test_browser_headers_no_referer() {
        let profile = &BROWSER_PROFILES[0];
        let headers = browser_headers(profile, None);
        assert!(headers.get(USER_AGENT).is_some());
        assert_eq!(
            headers
                .get(HeaderName::from_static("sec-fetch-site"))
                .and_then(|v| v.to_str().ok()),
            Some("none")
        );
        assert!(headers.get(REFERER).is_none());
    }

    #[test]
    fn test_browser_headers_with_referer() {
        let profile = &BROWSER_PROFILES[0];
        let headers = browser_headers(profile, Some("https://www.google.com/"));
        assert_eq!(
            headers.get(REFERER).and_then(|v| v.to_str().ok()),
            Some("https://www.google.com/")
        );
        assert_eq!(
            headers
                .get(HeaderName::from_static("sec-fetch-site"))
                .and_then(|v| v.to_str().ok()),
            Some("cross-site")
        );
    }

    #[test]
    fn test_search_headers_same_origin() {
        let profile = &BROWSER_PROFILES[0];
        let headers = search_headers(profile, Some("https://duckduckgo.com/"));
        assert_eq!(
            headers
                .get(HeaderName::from_static("sec-fetch-site"))
                .and_then(|v| v.to_str().ok()),
            Some("same-origin")
        );
        assert!(headers.get(CACHE_CONTROL).is_some());
    }

    #[test]
    fn test_search_referer_detection() {
        assert_eq!(
            search_referer_for_url("https://html.duckduckgo.com/html/?q=test"),
            Some("https://duckduckgo.com/")
        );
        assert_eq!(
            search_referer_for_url("https://www.bing.com/search?q=test"),
            Some("https://www.bing.com/")
        );
        assert_eq!(search_referer_for_url("https://example.com"), None);
    }

    #[test]
    fn test_profiles_have_consistent_headers() {
        for profile in BROWSER_PROFILES {
            let ua_lower = profile.user_agent.to_ascii_lowercase();
            if ua_lower.contains("chrome") && !ua_lower.contains("firefox") {
                assert!(
                    profile.sec_ch_ua.is_some(),
                    "Chrome-based UA should have Sec-CH-UA: {}",
                    profile.user_agent
                );
            }
            if ua_lower.contains("firefox") {
                assert!(
                    profile.sec_ch_ua.is_none(),
                    "Firefox UA should not have Sec-CH-UA: {}",
                    profile.user_agent
                );
            }
        }
    }
}
