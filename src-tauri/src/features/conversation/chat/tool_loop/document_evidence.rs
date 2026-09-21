//! Bounded evidence of document excerpts delivered during this turn.
//!
//! This is prompt deduplication, not a source cache: callers must execute the
//! scoped read first. A digest of the fresh result detects changes even outside
//! the displayed excerpt. Reuse also requires the exact excerpt to remain in
//! the active request. Eviction or compaction therefore restores the text on
//! the next read instead of referring to evidence the model no longer has.

use crate::features::function_calling::domain::FunctionResult;
use sha2::{Digest, Sha256};
use std::collections::VecDeque;

const MAX_ENTRIES: usize = 8;
const REUSE_NOTICE: &str = "This document read is unchanged. The exact excerpt is already in an earlier tool result in your current context; use that evidence. This does not imply the whole document was read.";

#[derive(Default)]
pub(super) struct DocumentEvidence {
    // Fixed-size fingerprints only: document text stays in the existing prompt.
    recent: VecDeque<([u8; 32], [u8; 32])>,
}

impl DocumentEvidence {
    pub(super) fn render(
        &mut self,
        tool: &str,
        result: &FunctionResult,
        output: String,
        visible: impl FnOnce(&str) -> bool,
    ) -> String {
        if tool != "get_document" || !result.success || output.len() <= REUSE_NOTICE.len() {
            return output;
        }
        let Some(data) = &result.data else {
            return output;
        };
        let Ok(bytes) = serde_json::to_vec(data) else {
            return output;
        };
        let fingerprint = (
            Sha256::digest(bytes).into(),
            Sha256::digest(output.as_bytes()).into(),
        );
        let remembered = self.recent.contains(&fingerprint);
        self.recent.retain(|entry| entry != &fingerprint);
        self.recent.push_back(fingerprint);
        while self.recent.len() > MAX_ENTRIES {
            self.recent.pop_front();
        }
        if remembered && visible(&output) {
            REUSE_NOTICE.to_string()
        } else {
            output
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn result(page: usize, text: &str) -> FunctionResult {
        FunctionResult::success(json!({"document_id":"doc", "page":page, "content":text}))
    }

    #[test]
    fn unchanged_visible_evidence_is_reused_but_evicted_evidence_is_restored() {
        let mut memo = DocumentEvidence::default();
        let excerpt = "Exact document excerpt. ".repeat(30);
        let source = result(1, &excerpt);
        assert_eq!(
            memo.render("get_document", &source, excerpt.clone(), |_| false),
            excerpt
        );
        assert_eq!(
            memo.render("get_document", &source, excerpt.clone(), |s| s == excerpt),
            REUSE_NOTICE
        );
        assert_eq!(
            memo.render("get_document", &source, excerpt.clone(), |_| false),
            excerpt
        );
    }

    #[test]
    fn changed_source_pages_and_failures_never_reuse_stale_evidence() {
        let mut memo = DocumentEvidence::default();
        let excerpt = "Visible prefix. ".repeat(40);
        memo.render(
            "get_document",
            &result(1, "version one"),
            excerpt.clone(),
            |_| true,
        );
        for source in [
            result(1, "changed beyond excerpt"),
            result(2, "version one"),
            FunctionResult::error("denied", "scope changed"),
        ] {
            assert_eq!(
                memo.render("get_document", &source, excerpt.clone(), |_| true),
                excerpt
            );
        }
        assert_eq!(
            memo.render(
                "semantic_search",
                &result(1, "version one"),
                excerpt.clone(),
                |_| true
            ),
            excerpt
        );
    }

    #[test]
    fn rolling_window_is_bounded_and_does_not_claim_an_evicted_read() {
        let mut memo = DocumentEvidence::default();
        let excerpt = "Exact document excerpt. ".repeat(30);
        for page in 0..100 {
            memo.render(
                "get_document",
                &result(page, "text"),
                excerpt.clone(),
                |_| true,
            );
            assert!(memo.recent.len() <= MAX_ENTRIES);
        }
        assert_eq!(
            memo.render("get_document", &result(0, "text"), excerpt.clone(), |_| {
                true
            }),
            excerpt
        );
        assert_eq!(
            memo.render("get_document", &result(99, "text"), excerpt.clone(), |_| {
                true
            }),
            REUSE_NOTICE
        );
    }
}
