//! The two HTTP identities Lattice presents (design §3.9).
//!
//! - **Browser-like** for HTML search engines and ordinary pages, where a
//!   browser is what the site expects of a user's reading client: consistent
//!   browser profiles (User-Agent with matching client hints) and navigation
//!   headers.
//! - **Declared client** for every API: `Lattice/<version> (+<project URL>)`,
//!   plus the user's contact when they set one for polite pools. Wikimedia,
//!   crates.io, Crossref, SEC and arXiv require an honest agent from bots, and
//!   a browser disguise on an API is what gets a client blocked.
//!
//! There is no proxy pool and no challenge solver: anonymity comes from
//! sending only the query, never from routing through someone else's
//! machine (decided 2026-09-23).

use rand::Rng;
use reqwest::header::{
    HeaderMap, HeaderName, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, CACHE_CONTROL, CONNECTION,
    REFERER, USER_AGENT,
};
use std::time::Duration;
use tokio::time::sleep;
use tracing::debug;

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

// ─── Profile Selection ──────────────────────────────────────────────────────

#[allow(clippy::indexing_slicing)] // gen_range stays below BROWSER_PROFILES.len()
pub fn random_profile() -> &'static BrowserProfile {
    let idx = rand::thread_rng().gen_range(0..BROWSER_PROFILES.len());
    &BROWSER_PROFILES[idx]
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

// ─── Declared Client ────────────────────────────────────────────────────────

/// Where the project lives: the contact half of the declared agent, which
/// Wikimedia's User-Agent policy asks every bot to carry. A project page,
/// never an email: one shared address would make every user look like one
/// client to the services that pool by contact.
pub const PROJECT_URL: &str = "https://github.com/J-Reed700/lattice";

/// The declared agent: `Lattice/<version> (+<project URL>)`, with the user's
/// own contact appended when they set one for polite pools.
pub fn declared_user_agent(contact: Option<&str>) -> String {
    let version = env!("CARGO_PKG_VERSION");
    match contact.map(str::trim).filter(|contact| !contact.is_empty()) {
        Some(contact) => format!("Lattice/{version} (+{PROJECT_URL}; {contact})"),
        None => format!("Lattice/{version} (+{PROJECT_URL})"),
    }
}

/// Headers for an API call: the declared agent and what the caller accepts.
/// No browser profile, no client hints, no `Sec-Fetch-*`.
pub fn declared_client_headers(contact: Option<&str>, accept: &'static str) -> HeaderMap {
    let mut headers = HeaderMap::with_capacity(2);
    if let Ok(agent) = HeaderValue::from_str(&declared_user_agent(contact)) {
        headers.insert(USER_AGENT, agent);
    }
    headers.insert(ACCEPT, HeaderValue::from_static(accept));
    headers
}

// ─── Client Builder ─────────────────────────────────────────────────────────

/// The base `ClientBuilder` for reading the public web. No cookie jar: one
/// client serves every turn and conversation, so a jar would replay a search
/// engine's cookies on every later query and let it link searches from
/// unrelated conversations. The hidden browser reader is incognito for the
/// same reason.
///
/// Does **not** set `User-Agent`: each request carries its identity's headers
/// ([`browser_headers`], [`search_headers`] or [`declared_client_headers`]).
pub fn client_builder() -> reqwest::ClientBuilder {
    lattice_core::shared::http::reqwest_client_builder().cookie_store(false)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn an_api_call_declares_lattice_and_never_a_browser() {
        let headers = declared_client_headers(None, "application/json");
        let agent = headers
            .get(USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .unwrap();
        assert!(agent.starts_with("Lattice/"), "{agent}");
        assert!(agent.contains(PROJECT_URL), "{agent}");
        assert!(!agent.contains("Mozilla"), "{agent}");
        assert!(headers.get(HeaderName::from_static("sec-ch-ua")).is_none());

        let with_contact = declared_user_agent(Some(" me@example.org "));
        assert!(
            with_contact.ends_with("; me@example.org)"),
            "{with_contact}"
        );
        assert_eq!(declared_user_agent(Some("  ")), declared_user_agent(None));
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
