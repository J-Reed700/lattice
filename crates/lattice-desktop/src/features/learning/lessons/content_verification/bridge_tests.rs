//! Pass A reads a claim part by part against the passages its sentence
//! cites, and a page retrieved from the course is read by the plain strict
//! judge (claim classes design §3.3): the check, its record and its receipt.
use super::evidence_paging_tests::{inventory, marker_collection, reference, CLAIM};
use super::*;
use crate::features::learning::lesson_drafts;
use lattice_core::application::services::claim_verification::{PartKind, BRIDGE_SYSTEM};
use std::collections::VecDeque;

/// A statement of the shape the design's third section takes from a real
/// lesson: its source states the first half, and what a lexer does is common
/// knowledge.
const STATEMENT: &str = "Quillet is a lexer that turns characters into tokens.";
const STATED: &str = "Quillet is a lexer shipped with the toolchain.";

/// Answers a part-wise check with the next of its replies and any other check
/// as the strict judge that finds nothing, and records the system prompt of
/// every request.
struct Reading {
    replies: Mutex<VecDeque<String>>,
    systems: Mutex<Vec<String>>,
}

impl Reading {
    fn new(replies: Vec<String>) -> Self {
        Self {
            replies: Mutex::new(replies.into()),
            systems: Mutex::new(Vec::new()),
        }
    }

    /// How many requests were part-wise checks, and how many were not.
    fn requests(&self) -> (usize, usize) {
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
fn sources() -> Vec<LearningSourceDto> {
    vec![reference("quillet".into(), STATED.into())]
}

/// Unit 0 cites the passage that states half of the statement.
fn citing(references: &ReferenceCollection<'_>) -> Result<Citations> {
    Ok(Citations::new(
        HashMap::from([(0, references.cited_passages(&[STATED])?)]),
        HashMap::new(),
    ))
}

fn parts_reply(parts: Value, verdict: &str) -> String {
    json!({"parts": parts, "verdict": verdict, "reason": "The passage says Quillet is a lexer."})
        .to_string()
}

fn half_stated() -> String {
    parts_reply(
        json!([
            {"part": "Quillet is a lexer", "kind": "stated", "passage": "passage-0"},
            {"part": "a lexer turns characters into tokens", "kind": "common", "passage": null},
        ]),
        "supported_with_common",
    )
}

#[tokio::test]
async fn a_statement_whose_cited_passage_states_half_of_it_passes_with_the_rest_as_common_knowledge(
) -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson) = batch_scheduler_tests::setup(&pool).await?;
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let citations = citing(&references)?;
    let model = Reading::new(vec![half_stated()]);
    lesson_drafts::run(&repo, &job, &lesson, async {
        let results = evidence_checks::check_with_citations(
            &model,
            &references,
            &inventory(&[STATEMENT.into()]),
            &[],
            &ClaimChecks::default(),
            &citations,
        )
        .await?;
        let finding = &results[0].2;
        assert_eq!(finding.verdict, ClaimVerdict::Supported);
        assert_eq!(finding.two_pass.pass, Some(Pass::Attribution));
        assert!(finding
            .supporting_quote
            .as_deref()
            .is_some_and(|quote| quote.contains(STATED)));
        assert_eq!(
            finding.two_pass.parts,
            vec![
                CheckedPart {
                    part: "Quillet is a lexer".into(),
                    kind: PartKind::Stated,
                    source_id: Some("quillet".into()),
                },
                CheckedPart {
                    part: "a lexer turns characters into tokens".into(),
                    kind: PartKind::Common,
                    source_id: None,
                },
            ]
        );
        assert!(finding.reason.contains(
            "Common knowledge, not in the passages: a lexer turns characters into tokens."
        ));
        let (part_wise, _) = model.requests();
        assert_eq!(part_wise, 1, "the one call the statement's check costs");

        // A later pass restores the check, parts and all, and asks nothing.
        let again = evidence_checks::check_with_citations(
            &model,
            &references,
            &inventory(&[STATEMENT.into()]),
            &[],
            &ClaimChecks::default(),
            &citations,
        )
        .await?;
        assert_eq!(again[0].2.verdict, ClaimVerdict::Supported);
        assert_eq!(again[0].2.two_pass.parts, finding.two_pass.parts);
        assert_eq!(model.requests().0, 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_specific_the_cited_passage_lacks_fails_the_statement_beside_a_stated_part() -> Result<()>
{
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson) = batch_scheduler_tests::setup(&pool).await?;
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let citations = citing(&references)?;
    let model = Reading::new(vec![parts_reply(
        json!([
            {"part": "Quillet is a lexer", "kind": "stated", "passage": "passage-0"},
            {"part": "it was written in 2004", "kind": "specific_missing", "passage": null},
        ]),
        "supported",
    )]);
    lesson_drafts::run(&repo, &job, &lesson, async {
        let results = evidence_checks::check_with_citations(
            &model,
            &references,
            &inventory(&["Quillet is a lexer written in 2004.".into()]),
            &[],
            &ClaimChecks::default(),
            &citations,
        )
        .await?;
        let finding = &results[0].2;
        assert_eq!(finding.verdict, ClaimVerdict::Unsupported);
        assert!(finding
            .reason
            .contains("Not given by the passages: it was written in 2004."));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_part_wise_answer_that_cannot_be_read_leaves_the_statement_unjudged() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson) = batch_scheduler_tests::setup(&pool).await?;
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let citations = citing(&references)?;
    let model = Reading::new(vec!["not json".into(), "still not json".into()]);
    lesson_drafts::run(&repo, &job, &lesson, async {
        let results = evidence_checks::check_with_citations(
            &model,
            &references,
            &inventory(&[STATEMENT.into()]),
            &[],
            &ClaimChecks::default(),
            &citations,
        )
        .await?;
        // Asked once more, then neither failed nor supported, and no receipt.
        let finding = &results[0].2;
        assert_eq!(finding.verdict, ClaimVerdict::Unverified);
        assert!(finding.two_pass.parts.is_empty());
        assert_eq!(model.requests().0, 2);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_page_retrieved_from_the_course_is_read_by_the_plain_strict_judge() -> Result<()> {
    let references = ReferenceCollection::lexical(&marker_collection())?;
    // Nothing is cited: the claim meets the course's first page for it.
    let model = Reading::new(vec![half_stated()]);
    let results = evidence_checks::check_with_citations(
        &model,
        &references,
        &inventory(&[CLAIM.into()]),
        &[],
        &ClaimChecks::default(),
        &Citations::default(),
    )
    .await?;
    let (part_wise, strict) = model.requests();
    assert_eq!(part_wise, 0);
    assert!(strict > 0, "the page was judged");
    assert_eq!(results[0].2.verdict, ClaimVerdict::Unsupported);
    assert!(results[0].2.two_pass.parts.is_empty());
    Ok(())
}
