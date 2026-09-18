//! What one turn has already learned about pages it cannot read.
//!
//! A tool round costs one full model generation. On a slow or remote model that
//! is minutes, so a round spent re-fetching a URL that already failed is the
//! most expensive kind of nothing. The model has no memory between rounds
//! beyond the transcript we build for it, and a bare "Tool failed: HTTP 403"
//! buried among other results is easy for it to read past — in practice it
//! asks for the same blocked page again a round later.
//!
//! This remembers the failures for the length of the turn, answers a repeat
//! from memory instead of going back out to the network, and states the whole
//! list each round so the model can route around it.

use std::collections::BTreeMap;

/// Enough room for a turn's worth of dead links without letting a model that
/// invents URLs grow the map without bound.
const MAX_REMEMBERED_FAILURES: usize = 64;

/// URLs this turn tried and could not read, and why.
#[derive(Debug, Default)]
pub(super) struct FetchMemory {
    failures: BTreeMap<String, String>,
}

impl FetchMemory {
    /// Note that `url` could not be read. The first reason recorded for a URL
    /// wins: it is the one that actually cost a request.
    pub(super) fn record_failure(&mut self, url: &str, reason: &str) {
        let key = normalize(url);
        if key.is_empty() || self.failures.len() >= MAX_REMEMBERED_FAILURES {
            return;
        }
        self.failures
            .entry(key)
            .or_insert_with(|| summarize(reason));
    }

    /// Why `url` failed earlier this turn, if it did.
    pub(super) fn previous_failure(&self, url: &str) -> Option<&str> {
        self.failures.get(&normalize(url)).map(String::as_str)
    }

    /// One line per dead URL, for the transcript. Empty when nothing failed,
    /// so a turn that is going well carries no extra prompt weight.
    pub(super) fn advisory(&self) -> Option<String> {
        if self.failures.is_empty() {
            return None;
        }
        let mut lines = String::from(
            "These URLs already failed this turn and will not be retried. Do not request them again; choose different sources.",
        );
        for (url, reason) in &self.failures {
            lines.push_str(&format!("\n- {url} ({reason})"));
        }
        Some(lines)
    }
}

/// The `url` argument of a fetch call, if the call has a usable one.
pub(super) fn fetch_target(arguments: &serde_json::Value) -> Option<&str> {
    arguments
        .get("url")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
}

/// Compare URLs the way a server would: the scheme and host are
/// case-insensitive and a trailing slash on the path is not a different page,
/// so `HTTPS://Example.com/a/` must not read as new after `https://example.com/a`.
fn normalize(url: &str) -> String {
    let trimmed = url.trim();
    let Ok(mut parsed) = url::Url::parse(trimmed) else {
        return trimmed.to_ascii_lowercase();
    };
    parsed.set_fragment(None);
    let normalized = parsed.as_str().trim_end_matches('/').to_string();
    normalized.to_ascii_lowercase()
}

/// Keep the reason short — it is repeated in the prompt every round.
fn summarize(reason: &str) -> String {
    const MAX: usize = 120;
    let cleaned = reason.trim().replace('\n', " ");
    if cleaned.chars().count() <= MAX {
        return cleaned;
    }
    let cut: String = cleaned.chars().take(MAX).collect();
    format!("{cut}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_url_that_failed_is_recognised_when_it_comes_back() {
        let mut memory = FetchMemory::default();
        memory.record_failure(
            "https://www.thereviewgeek.com/silo-s1e2review/",
            "HTTP 403 Forbidden",
        );
        assert_eq!(
            memory.previous_failure("https://www.thereviewgeek.com/silo-s1e2review/"),
            Some("HTTP 403 Forbidden")
        );
        assert_eq!(memory.previous_failure("https://example.com/other"), None);
    }

    /// The model rarely echoes a URL back byte for byte — it drops a trailing
    /// slash or a fragment. Those are the same page and must not buy a retry.
    #[test]
    fn a_url_that_differs_only_cosmetically_is_the_same_url() {
        let mut memory = FetchMemory::default();
        memory.record_failure("https://Example.com/a/b/", "HTTP 403 Forbidden");
        for variant in [
            "https://example.com/a/b",
            "https://example.com/a/b/",
            "https://EXAMPLE.com/a/b#section",
            "  https://example.com/a/b  ",
        ] {
            assert!(
                memory.previous_failure(variant).is_some(),
                "{variant} should match the recorded failure"
            );
        }
    }

    /// A different path on the same host is a different page.
    #[test]
    fn a_different_page_on_the_same_host_is_not_blocked() {
        let mut memory = FetchMemory::default();
        memory.record_failure("https://example.com/a", "HTTP 403 Forbidden");
        assert_eq!(memory.previous_failure("https://example.com/b"), None);
    }

    #[test]
    fn the_first_reason_recorded_is_the_one_kept() {
        let mut memory = FetchMemory::default();
        memory.record_failure("https://example.com/a", "HTTP 403 Forbidden");
        memory.record_failure("https://example.com/a", "something else");
        assert_eq!(
            memory.previous_failure("https://example.com/a"),
            Some("HTTP 403 Forbidden")
        );
    }

    #[test]
    fn a_quiet_turn_adds_nothing_to_the_prompt() {
        assert_eq!(FetchMemory::default().advisory(), None);
    }

    #[test]
    fn the_advisory_names_every_dead_url() {
        let mut memory = FetchMemory::default();
        memory.record_failure("https://example.com/a", "HTTP 403 Forbidden");
        memory.record_failure("https://example.com/b", "HTTP 404 Not Found");
        let advisory = memory.advisory().expect("failures were recorded");
        assert!(advisory.contains("https://example.com/a"));
        assert!(advisory.contains("HTTP 403 Forbidden"));
        assert!(advisory.contains("https://example.com/b"));
        assert!(advisory.contains("HTTP 404 Not Found"));
    }

    /// A model that invents URLs must not be able to grow the map forever.
    #[test]
    fn the_memory_stops_growing_at_the_cap() {
        let mut memory = FetchMemory::default();
        for index in 0..(MAX_REMEMBERED_FAILURES + 50) {
            memory.record_failure(
                &format!("https://example.com/{index}"),
                "HTTP 403 Forbidden",
            );
        }
        assert_eq!(memory.failures.len(), MAX_REMEMBERED_FAILURES);
    }

    #[test]
    fn a_long_reason_is_cut_down_before_it_reaches_the_prompt() {
        let mut memory = FetchMemory::default();
        memory.record_failure("https://example.com/a", &"x".repeat(400));
        let reason = memory.previous_failure("https://example.com/a").unwrap();
        assert!(
            reason.chars().count() <= 121,
            "got {} chars",
            reason.chars().count()
        );
    }

    #[test]
    fn a_fetch_call_without_a_usable_url_has_no_target() {
        assert_eq!(
            fetch_target(&serde_json::json!({"url": "https://a.test/x"})),
            Some("https://a.test/x")
        );
        assert_eq!(fetch_target(&serde_json::json!({"url": "   "})), None);
        assert_eq!(fetch_target(&serde_json::json!({"query": "silo"})), None);
        assert_eq!(fetch_target(&serde_json::json!({"url": 7})), None);
    }
}
