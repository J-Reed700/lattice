//! What one turn has already learned about the pages it asked for.
//!
//! A tool round costs one full model generation. On a slow or remote model that
//! is minutes, so a round spent re-fetching a page the turn already has — or one
//! that already failed — is the most expensive kind of nothing. The model has no
//! memory between rounds beyond the transcript we build for it, and a bare
//! "Tool failed: HTTP 403" buried among other results is easy for it to read
//! past — in practice it asks for the same blocked page again a round later.
//!
//! Pages are read in two places: the retrieval pipeline opens the top search
//! results before the model runs at all, and the tool loop fetches whatever the
//! model asks for after that. This is the one record both share, so the second
//! never pays the network — or a generation — for what the first already did.
//! It answers a repeat from memory and states the dead list each round so the
//! model can route around it.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Enough room for a turn's worth of dead links without letting a model that
/// invents URLs grow the map without bound.
const MAX_REMEMBERED_FAILURES: usize = 64;

/// Pages held at once. A page is whole extracted text, bounded by the
/// fetcher's decoded-body limit, so this bounds a turn's memory however many
/// pages it reads. A page past it is not lost: it is in the conversation's
/// archive, and a request for it reads it from there.
const MAX_REMEMBERED_PAGES: usize = 32;

/// How much of a page the model has been given so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::features::conversation::chat) enum Delivery {
    /// Everything that was extracted is already in front of the model.
    Whole,
    /// The prompt only had room for the first `shown_chars`; the rest is held
    /// here so asking for it costs no request.
    Clipped { shown_chars: usize },
}

/// What a repeat request for a page this turn already read is answered with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::features::conversation::chat) enum Recall {
    /// The model already has all of it. There is nothing to send.
    AlreadyWhole { word_count: usize },
    /// The next part of a page: `text` starts `shown_chars` characters in, as
    /// much as the result's allowance holds. `next` is where the part after
    /// it starts, when there is one.
    Remainder {
        text: String,
        shown_chars: usize,
        next: Option<usize>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RememberedPage {
    text: String,
    delivery: Delivery,
}

/// Every page this turn tried to read: the ones it could not, and why, and the
/// ones it could, and how much of each the model has seen. A research turn
/// saves it with each round, so a resumed turn still knows what it read.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(in crate::features::conversation::chat) struct FetchMemory {
    failures: BTreeMap<String, String>,
    pages: BTreeMap<String, RememberedPage>,
}

impl FetchMemory {
    /// Note that `url` could not be read. The first reason recorded for a URL
    /// wins: it is the one that actually cost a request.
    pub(in crate::features::conversation::chat) fn record_failure(
        &mut self,
        url: &str,
        reason: &str,
    ) {
        let key = normalize(url);
        if key.is_empty() || self.failures.len() >= MAX_REMEMBERED_FAILURES {
            return;
        }
        self.failures
            .entry(key)
            .or_insert_with(|| summarize(reason));
    }

    /// Why `url` failed earlier this turn, if it did.
    pub(in crate::features::conversation::chat) fn previous_failure(
        &self,
        url: &str,
    ) -> Option<&str> {
        self.failures.get(&normalize(url)).map(String::as_str)
    }

    /// Note that `url` was read, and how much of `text` the model was given.
    /// The first read of a URL wins: that is the text the model is looking at.
    pub(in crate::features::conversation::chat) fn record_page(
        &mut self,
        url: &str,
        text: &str,
        delivery: Delivery,
    ) {
        let key = normalize(url);
        if key.is_empty() || self.pages.len() >= MAX_REMEMBERED_PAGES {
            return;
        }
        self.pages.entry(key).or_insert_with(|| RememberedPage {
            text: text.to_string(),
            delivery,
        });
    }

    /// Answer a repeat request for `url` from memory, if this turn read it.
    ///
    /// A page the model has whole is never sent again. Otherwise the request
    /// gets the next part, starting at `offset` when the model named one and
    /// where the model's copy stops when it did not, and at most `room`
    /// characters of it: a page longer than one result can hold is handed over
    /// in parts, each saying where the next begins, so its tail is reachable
    /// and no result outgrows the window. Once the last part is out the page is
    /// marked whole.
    pub(in crate::features::conversation::chat) fn recall(
        &mut self,
        url: &str,
        offset: Option<usize>,
        room: usize,
    ) -> Option<Recall> {
        let page = self.pages.get_mut(&normalize(url))?;
        let already_whole = |text: &str| Recall::AlreadyWhole {
            word_count: text.split_whitespace().count(),
        };
        let start = match page.delivery {
            Delivery::Whole => return Some(already_whole(&page.text)),
            Delivery::Clipped { shown_chars } => offset.unwrap_or(shown_chars),
        };
        let total = page.text.chars().count();
        let end = start.saturating_add(room.max(1)).min(total);
        let text: String = page
            .text
            .chars()
            .skip(start)
            .take(end.saturating_sub(start))
            .collect();
        if text.trim().is_empty() {
            page.delivery = Delivery::Whole;
            return Some(already_whole(&page.text));
        }
        let next = (end < total).then_some(end);
        page.delivery = match next {
            Some(next) => Delivery::Clipped { shown_chars: next },
            None => Delivery::Whole,
        };
        Some(Recall::Remainder {
            text,
            shown_chars: start,
            next,
        })
    }

    /// Fold what another part of the turn learned into this record.
    pub(in crate::features::conversation::chat) fn absorb(&mut self, other: FetchMemory) {
        for (url, reason) in other.failures {
            if self.failures.len() >= MAX_REMEMBERED_FAILURES {
                break;
            }
            self.failures.entry(url).or_insert(reason);
        }
        for (url, page) in other.pages {
            if self.pages.len() >= MAX_REMEMBERED_PAGES {
                break;
            }
            self.pages.entry(url).or_insert(page);
        }
    }
}

/// The `url` argument of a fetch call, if the call has a usable one.
pub(in crate::features::conversation::chat) fn fetch_target(
    arguments: &serde_json::Value,
) -> Option<&str> {
    arguments
        .get("url")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
}

/// The `offset` argument of a fetch call: the character a model reading a
/// long page on wants the next part to start at.
pub(in crate::features::conversation::chat) fn fetch_offset(
    arguments: &serde_json::Value,
) -> Option<usize> {
    arguments
        .get("offset")
        .and_then(serde_json::Value::as_u64)
        .and_then(|offset| usize::try_from(offset).ok())
}

fn normalize(url: &str) -> String {
    lattice_core::shared::http::url_identity::identity(url)
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

    /// Fragments and host/scheme case are cosmetic; path case and slash are not.
    #[test]
    fn a_url_that_differs_only_cosmetically_is_the_same_url() {
        let mut memory = FetchMemory::default();
        memory.record_failure("https://Example.com/a/b", "HTTP 403 Forbidden");
        for variant in [
            "https://example.com/a/b",
            "https://EXAMPLE.com/a/b#section",
            "  https://example.com/a/b  ",
        ] {
            assert!(
                memory.previous_failure(variant).is_some(),
                "{variant} should match the recorded failure"
            );
        }
        for distinct in ["https://example.com/a/b/", "https://example.com/A/b"] {
            assert!(
                memory.previous_failure(distinct).is_none(),
                "{distinct} is a distinct resource"
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

    /// The case that cost a whole generation round: the pipeline had already
    /// read the page in full, and the model asked for it again anyway.
    #[test]
    fn a_page_already_given_whole_has_nothing_more_to_send() {
        let mut memory = FetchMemory::default();
        memory.record_page(
            "https://example.com/recap/",
            "one two three four",
            Delivery::Whole,
        );
        assert_eq!(memory.recall("https://example.com/recap", None, 100), None);
        assert_eq!(
            memory.recall("https://example.com/recap/", None, 100),
            Some(Recall::AlreadyWhole { word_count: 4 })
        );
    }

    /// A clipped page hands over exactly the part the prompt had no room for,
    /// without going back to the network for it.
    #[test]
    fn a_clipped_page_hands_over_only_the_part_that_was_cut() {
        let mut memory = FetchMemory::default();
        memory.record_page(
            "https://example.com/recap",
            "0123456789",
            Delivery::Clipped { shown_chars: 4 },
        );
        assert_eq!(
            memory.recall("https://example.com/recap", None, 100),
            Some(Recall::Remainder {
                text: "456789".to_string(),
                shown_chars: 4,
                next: None,
            })
        );
    }

    /// Once the remainder is handed over the model has the whole page. A third
    /// request must not be sent the same text a second time.
    #[test]
    fn the_remainder_is_handed_over_once() {
        let mut memory = FetchMemory::default();
        memory.record_page(
            "https://example.com/recap",
            "0123456789",
            Delivery::Clipped { shown_chars: 4 },
        );
        assert!(matches!(
            memory.recall("https://example.com/recap", None, 100),
            Some(Recall::Remainder { .. })
        ));
        assert!(matches!(
            memory.recall("https://example.com/recap", None, 100),
            Some(Recall::AlreadyWhole { .. })
        ));
    }

    /// The cut is counted in characters, so it can never land inside one.
    #[test]
    fn a_clip_point_inside_multibyte_text_does_not_split_a_character() {
        let mut memory = FetchMemory::default();
        memory.record_page(
            "https://example.com/recap",
            "héllo wörld",
            Delivery::Clipped { shown_chars: 2 },
        );
        assert_eq!(
            memory.recall("https://example.com/recap", None, 100),
            Some(Recall::Remainder {
                text: "llo wörld".to_string(),
                shown_chars: 2,
                next: None,
            })
        );
    }

    /// A clip that happened to land at the very end left nothing behind.
    #[test]
    fn a_clip_that_cut_nothing_is_reported_as_whole() {
        let mut memory = FetchMemory::default();
        memory.record_page(
            "https://example.com/recap",
            "abcd   ",
            Delivery::Clipped { shown_chars: 4 },
        );
        assert!(matches!(
            memory.recall("https://example.com/recap", None, 100),
            Some(Recall::AlreadyWhole { .. })
        ));
    }

    /// A remainder longer than one result can hold arrives in parts, each
    /// saying where the next starts, until the last character is out. A whole
    /// remainder in one result could outgrow the model's window.
    #[test]
    fn a_long_remainder_is_handed_over_in_parts_that_fit_the_room() {
        let text: String = ('a'..='z').cycle().take(100).collect();
        let mut memory = FetchMemory::default();
        memory.record_page(
            "https://example.com/long",
            &text,
            Delivery::Clipped { shown_chars: 10 },
        );
        let mut received = text.chars().take(10).collect::<String>();
        let mut parts = 0;
        while let Some(Recall::Remainder {
            text: part,
            shown_chars,
            next,
        }) = memory.recall("https://example.com/long", None, 30)
        {
            assert_eq!(shown_chars, received.chars().count());
            assert!(part.chars().count() <= 30);
            received.push_str(&part);
            parts += 1;
            if next.is_none() {
                break;
            }
            assert_eq!(next, Some(received.chars().count()));
        }
        assert_eq!(parts, 3);
        assert_eq!(received, text);
        assert!(matches!(
            memory.recall("https://example.com/long", None, 30),
            Some(Recall::AlreadyWhole { .. })
        ));
    }

    /// The model may name where to read from, as the clipped-page notice tells
    /// it to.
    #[test]
    fn a_named_offset_is_where_the_part_starts() {
        let mut memory = FetchMemory::default();
        memory.record_page(
            "https://example.com/long",
            "0123456789",
            Delivery::Clipped { shown_chars: 2 },
        );
        assert_eq!(
            memory.recall("https://example.com/long", Some(6), 2),
            Some(Recall::Remainder {
                text: "67".to_string(),
                shown_chars: 6,
                next: Some(8),
            })
        );
    }

    #[test]
    fn a_page_this_turn_never_read_is_not_recalled() {
        let mut memory = FetchMemory::default();
        assert_eq!(memory.recall("https://example.com/never", None, 100), None);
    }

    /// What the pipeline learned before the model ran must be what the tool
    /// loop knows when the model starts asking.
    #[test]
    fn absorbing_carries_over_both_pages_and_failures() {
        let mut from_pipeline = FetchMemory::default();
        from_pipeline.record_failure("https://blocked.test/a", "HTTP 403 Forbidden");
        from_pipeline.record_page("https://read.test/b", "some text", Delivery::Whole);

        let mut tool_loop = FetchMemory::default();
        tool_loop.absorb(from_pipeline);

        assert_eq!(
            tool_loop.previous_failure("https://blocked.test/a"),
            Some("HTTP 403 Forbidden")
        );
        assert!(tool_loop.recall("https://read.test/b", None, 100).is_some());
    }

    #[test]
    fn the_page_memory_stops_growing_at_the_cap() {
        let mut memory = FetchMemory::default();
        for index in 0..(MAX_REMEMBERED_PAGES + 20) {
            memory.record_page(
                &format!("https://example.com/{index}"),
                "text",
                Delivery::Whole,
            );
        }
        assert_eq!(memory.pages.len(), MAX_REMEMBERED_PAGES);
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
