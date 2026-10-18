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

/// What part of a page one result gave the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::features::conversation::chat) enum Delivery {
    /// Everything that was extracted.
    Whole,
    /// Characters `start..end` of the page's text: as much as the result had
    /// room for, from where the model asked to start.
    Part { start: usize, end: usize },
}

/// What a repeat request for a page this turn already read is answered with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::features::conversation::chat) enum Recall {
    /// The model already has all of it. There is nothing to send.
    AlreadyWhole { word_count: usize },
    /// A part of the page: `text` starts `shown_chars` characters in, as much
    /// as the result's allowance holds. `next` is where the part after it
    /// starts, when the page goes on; `unread` is the first character the
    /// model has still not been given anywhere in the page, if there is one.
    Remainder {
        text: String,
        shown_chars: usize,
        next: Option<usize>,
        unread: Option<usize>,
    },
    /// The page's text is not held here (a resumed research turn keeps only
    /// what was delivered, not the text): read it again, from the
    /// conversation's archive, starting at `offset`.
    Reread { offset: usize },
    /// The offset asked for is at or past the end of the page, which is
    /// `chars` characters long. `unread` is the first stretch still not
    /// given, if there is one.
    PastTheEnd {
        offset: usize,
        chars: usize,
        unread: Option<usize>,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RememberedPage {
    /// The page's extracted text. Not saved with a research checkpoint: every
    /// page read is in the conversation's archive, and a checkpoint written
    /// every round should not carry up to a turn's worth of pages each time.
    #[serde(skip)]
    text: String,
    chars: usize,
    words: usize,
    /// The stretches of the text the model has been given, as `start..end`
    /// character ranges, sorted and merged.
    delivered: Vec<(usize, usize)>,
}

impl RememberedPage {
    fn hold(&mut self, text: &str) {
        if self.text.is_empty() && !text.is_empty() {
            self.text = text.to_string();
            self.chars = text.chars().count();
            self.words = text.split_whitespace().count();
        }
    }

    fn deliver(&mut self, delivery: Delivery) {
        let (start, end) = match delivery {
            Delivery::Whole => (0, self.chars),
            Delivery::Part { start, end } => (start.min(self.chars), end.min(self.chars)),
        };
        if start >= end {
            return;
        }
        self.delivered.push((start, end));
        self.delivered.sort_unstable();
        let mut merged: Vec<(usize, usize)> = Vec::with_capacity(self.delivered.len());
        for (start, end) in self.delivered.drain(..) {
            match merged.last_mut() {
                Some(last) if start <= last.1 => last.1 = last.1.max(end),
                _ => merged.push((start, end)),
            }
        }
        self.delivered = merged;
    }

    /// The first character at or after `from` the model has not been given.
    fn unread_from(&self, from: usize) -> Option<usize> {
        let mut at = from;
        for &(start, end) in &self.delivered {
            if start > at {
                break;
            }
            at = at.max(end);
        }
        (at < self.chars).then_some(at)
    }
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

    /// Note that `url` was read, and which part of `text` the model was given.
    /// The first text held for a URL wins: it is the text the offsets the
    /// model was shown refer to.
    pub(in crate::features::conversation::chat) fn record_page(
        &mut self,
        url: &str,
        text: &str,
        delivery: Delivery,
    ) {
        let key = normalize(url);
        if key.is_empty() {
            return;
        }
        if !self.pages.contains_key(&key) && self.pages.len() >= MAX_REMEMBERED_PAGES {
            return;
        }
        let page = self.pages.entry(key).or_default();
        page.hold(text);
        page.deliver(delivery);
    }

    /// Answer a repeat request for `url` from memory, if this turn read it.
    ///
    /// A request that names an `offset` is always given the part starting
    /// there, as much as `room` holds: the model may have lost an earlier part
    /// to condensing, and only it knows what it needs. Without an offset the
    /// request gets the first part the model has not been given, wherever that
    /// is, and a page given whole in any order of parts is never sent again.
    /// Every part says where the next begins and what is still unread, so
    /// nothing in the middle of a page becomes unreachable.
    pub(in crate::features::conversation::chat) fn recall(
        &mut self,
        url: &str,
        offset: Option<usize>,
        room: usize,
    ) -> Option<Recall> {
        let page = self.pages.get_mut(&normalize(url))?;
        let start = match offset {
            Some(offset) => offset,
            None => match page.unread_from(0) {
                Some(gap) => gap,
                None => {
                    return Some(Recall::AlreadyWhole {
                        word_count: page.words,
                    })
                }
            },
        };
        if page.text.is_empty() {
            return Some(Recall::Reread { offset: start });
        }
        if start >= page.chars {
            return Some(Recall::PastTheEnd {
                offset: start,
                chars: page.chars,
                unread: page.unread_from(0),
            });
        }
        let end = start.saturating_add(room.max(1)).min(page.chars);
        let text: String = page
            .text
            .chars()
            .skip(start)
            .take(end.saturating_sub(start))
            .collect();
        page.deliver(Delivery::Part { start, end });
        Some(Recall::Remainder {
            text,
            shown_chars: start,
            next: (end < page.chars).then_some(end),
            unread: page.unread_from(0),
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
            if !self.pages.contains_key(&url) && self.pages.len() >= MAX_REMEMBERED_PAGES {
                continue;
            }
            let held = self.pages.entry(url).or_default();
            held.hold(&page.text);
            if held.chars == 0 {
                held.chars = page.chars;
                held.words = page.words;
            }
            for (start, end) in page.delivered {
                held.deliver(Delivery::Part { start, end });
            }
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

    fn first(n: usize) -> Delivery {
        Delivery::Part { start: 0, end: n }
    }

    /// A clipped page hands over exactly the part the prompt had no room for,
    /// without going back to the network for it.
    #[test]
    fn a_clipped_page_hands_over_only_the_part_that_was_cut() {
        let mut memory = FetchMemory::default();
        memory.record_page("https://example.com/recap", "0123456789", first(4));
        assert_eq!(
            memory.recall("https://example.com/recap", None, 100),
            Some(Recall::Remainder {
                text: "456789".to_string(),
                shown_chars: 4,
                next: None,
                unread: None,
            })
        );
    }

    /// Once the remainder is handed over the model has the whole page. A third
    /// request must not be sent the same text a second time.
    #[test]
    fn the_remainder_is_handed_over_once() {
        let mut memory = FetchMemory::default();
        memory.record_page("https://example.com/recap", "0123456789", first(4));
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
        memory.record_page("https://example.com/recap", "héllo wörld", first(2));
        assert_eq!(
            memory.recall("https://example.com/recap", None, 100),
            Some(Recall::Remainder {
                text: "llo wörld".to_string(),
                shown_chars: 2,
                next: None,
                unread: None,
            })
        );
    }

    /// Reading a late part first used to mark the whole page read, so the
    /// middle could never be asked for. Only every character delivered, in
    /// whatever order, makes a page whole; until then a request without an
    /// offset gets the first stretch still unread.
    #[test]
    fn a_late_part_read_first_leaves_the_middle_reachable() {
        let text: String = ('a'..='z').collect();
        let mut memory = FetchMemory::default();
        memory.record_page(
            "https://example.com/long",
            &text,
            Delivery::Part { start: 20, end: 26 },
        );
        assert_eq!(
            memory.recall("https://example.com/long", None, 10),
            Some(Recall::Remainder {
                text: "abcdefghij".to_string(),
                shown_chars: 0,
                next: Some(10),
                unread: Some(10),
            })
        );
        assert_eq!(
            memory.recall("https://example.com/long", None, 100),
            Some(Recall::Remainder {
                text: "klmnopqrstuvwxyz".to_string(),
                shown_chars: 10,
                next: None,
                unread: None,
            })
        );
        assert!(matches!(
            memory.recall("https://example.com/long", None, 100),
            Some(Recall::AlreadyWhole { .. })
        ));
    }

    /// A named offset is always served, even on a page given whole: a part
    /// condensed out of the context is only reachable that way.
    #[test]
    fn a_named_offset_is_served_even_on_a_whole_page() {
        let mut memory = FetchMemory::default();
        memory.record_page("https://example.com/p", "0123456789", Delivery::Whole);
        assert_eq!(
            memory.recall("https://example.com/p", Some(3), 2),
            Some(Recall::Remainder {
                text: "34".to_string(),
                shown_chars: 3,
                next: Some(5),
                unread: None,
            })
        );
    }

    /// A research checkpoint is written every round; it keeps what was
    /// delivered, not each page's text. A resumed turn reads the text again
    /// from the archive, from the first part the model was not given.
    #[test]
    fn a_saved_memory_keeps_what_was_delivered_but_not_the_text() {
        let text = "word ".repeat(2_000);
        let mut memory = FetchMemory::default();
        memory.record_page("https://example.com/big", text.trim(), first(300));
        let saved = serde_json::to_string(&memory).unwrap();
        assert!(saved.len() < 400, "{} bytes saved", saved.len());
        let mut resumed: FetchMemory = serde_json::from_str(&saved).unwrap();
        assert_eq!(
            resumed.recall("https://example.com/big", None, 100),
            Some(Recall::Reread { offset: 300 })
        );
        resumed.record_page(
            "https://example.com/big",
            text.trim(),
            Delivery::Part {
                start: 300,
                end: 600,
            },
        );
        assert!(matches!(
            resumed.recall("https://example.com/big", None, 100),
            Some(Recall::Remainder {
                shown_chars: 600,
                ..
            })
        ));
    }

    /// A remainder longer than one result can hold arrives in parts, each
    /// saying where the next starts, until the last character is out. A whole
    /// remainder in one result could outgrow the model's window.
    #[test]
    fn a_long_remainder_is_handed_over_in_parts_that_fit_the_room() {
        let text: String = ('a'..='z').cycle().take(100).collect();
        let mut memory = FetchMemory::default();
        memory.record_page("https://example.com/long", &text, first(10));
        let mut received = text.chars().take(10).collect::<String>();
        let mut parts = 0;
        while let Some(Recall::Remainder {
            text: part,
            shown_chars,
            next,
            ..
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
        memory.record_page("https://example.com/long", "0123456789", first(2));
        assert_eq!(
            memory.recall("https://example.com/long", Some(6), 2),
            Some(Recall::Remainder {
                text: "67".to_string(),
                shown_chars: 6,
                next: Some(8),
                unread: Some(2),
            })
        );
    }

    /// An offset at or past the end used to come back as an empty part with
    /// no word of why; it says how long the page is, and what is unread.
    #[test]
    fn an_offset_past_the_end_says_how_long_the_page_is() {
        let mut memory = FetchMemory::default();
        memory.record_page("https://example.com/long", "0123456789", first(2));
        assert_eq!(
            memory.recall("https://example.com/long", Some(10), 5),
            Some(Recall::PastTheEnd {
                offset: 10,
                chars: 10,
                unread: Some(2),
            })
        );
        assert!(matches!(
            memory.recall("https://example.com/long", Some(400), 5),
            Some(Recall::PastTheEnd { offset: 400, .. })
        ));
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
