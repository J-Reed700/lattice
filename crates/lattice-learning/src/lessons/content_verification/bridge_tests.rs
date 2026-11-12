//! The fixtures of the part-wise reading (claim classes design §3.3): a
//! statement is read part by part against the passages its sentence cites,
//! and a page retrieved from the course is read by the plain strict judge.
use super::evidence_paging_tests::{inventory, marker_collection, reference, CLAIM};
use super::readings_tests::paged;
use super::*;
use lattice_core::application::services::claim_verification::BRIDGE_SYSTEM;
use std::collections::VecDeque;

/// A statement of the shape the design's third section takes from a real
/// lesson: its source states the first half, and what a lexer does is common
/// knowledge.
pub(super) const STATEMENT: &str = "Quillet is a lexer that turns characters into tokens.";
pub(super) const STATED: &str = "Quillet is a lexer shipped with the toolchain.";

/// Answers a part-wise check with the next of its replies and any other check
/// as the strict judge that finds nothing, and records the system prompt of
/// every request.
pub(super) struct Reading {
    replies: Mutex<VecDeque<String>>,
    systems: Mutex<Vec<String>>,
}

impl Reading {
    pub(super) fn new(replies: Vec<String>) -> Self {
        Self {
            replies: Mutex::new(replies.into()),
            systems: Mutex::new(Vec::new()),
        }
    }

    /// How many requests were part-wise checks, and how many were not.
    pub(super) fn requests(&self) -> (usize, usize) {
        let systems = self.systems.lock().unwrap();
        let part_wise = systems
            .iter()
            .filter(|system| *system == BRIDGE_SYSTEM)
            .count();
        (part_wise, systems.len() - part_wise)
    }
}

#[async_trait::async_trait]
impl LLMPort for Reading {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let (system, _) = fixture_messages(request);
        self.systems.lock().unwrap().push(system.to_owned());
        if system == BRIDGE_SYSTEM {
            let reply = self.replies.lock().unwrap().pop_front().unwrap_or_default();
            return Ok(CompletionResponse::from_text(reply));
        }
        Ok(CompletionResponse::from_text(
            "Reason: The passages do not state it.\nSource passage: none\nVerdict: unsupported",
        ))
    }
    fn model_name(&self) -> &str {
        "reading-fixture"
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    fn max_context_tokens(&self) -> usize {
        128_000
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// The one source the statement cites: nothing is left for a second page.
pub(super) fn sources() -> Vec<LearningSourceDto> {
    vec![reference("quillet".into(), STATED.into())]
}

/// Unit 0 cites the passage that states half of the statement.
pub(super) fn citing(references: &ReferenceCollection<'_>) -> Result<Citations> {
    Ok(Citations::new(
        HashMap::from([(0, references.cited_passages(&[STATED])?)]),
        HashMap::new(),
    ))
}

pub(super) fn parts_reply(parts: Value, verdict: &str) -> String {
    json!({"parts": parts, "verdict": verdict, "reason": "The passage says Quillet is a lexer."})
        .to_string()
}

pub(super) fn half_stated() -> String {
    parts_reply(
        json!([
            {"part": "Quillet is a lexer", "kind": "stated", "passage": "passage-0"},
            {"part": "a lexer turns characters into tokens", "kind": "common", "passage": null},
        ]),
        "supported_with_common",
    )
}

#[tokio::test]
async fn a_page_retrieved_from_the_course_is_read_by_the_plain_strict_judge() -> Result<()> {
    let references = ReferenceCollection::lexical(&marker_collection())?;
    // Nothing is cited: the claim meets the course's first page for it.
    let model = Reading::new(vec![half_stated()]);
    let reads = paged(
        &model,
        &references,
        &inventory(&[CLAIM.into()]),
        &ClaimChecks::default(),
    )
    .await?;
    let (part_wise, strict) = model.requests();
    assert_eq!(part_wise, 0);
    assert!(strict > 0, "the page was judged");
    assert_eq!(reads[0].finding().verdict, ClaimVerdict::Unsupported);
    assert!(reads[0].finding().two_pass.parts.is_empty());
    Ok(())
}
