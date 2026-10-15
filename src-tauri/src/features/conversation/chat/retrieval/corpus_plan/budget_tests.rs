//! The planner on a vault too large to show whole: which documents it is
//! shown, how its prompt is sized, and what a timeout costs later turns.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use super::health::{
    PlannerHealth, PLANNER_PROMPT_FLOOR, PLANNER_PROMPT_TOKENS, PLANNER_STAND_DOWN,
};
use super::prompt::PLANNER_OUTPUT_TOKENS;
use super::shortlist::{shortlist, SHORTLIST_CAP};
use super::*;
use crate::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use crate::application::ports::LLMPort;
use crate::domain::value_objects::source_context::{SourceContext, SourceGroup, StructureMode};
use std::collections::HashMap;
use tokio_util::sync::CancellationToken;

const PLAN: &str =
    r#"{"queries":["garden leave notice"],"opening_document_ids":[],"start_at_beginning":false}"#;

/// What llama-server's tokenizer charges for catalog JSON: closer to three and
/// a half characters a token than the four the estimate assumes.
fn server_tokens(text: &str) -> usize {
    (text.len() * 2).div_ceil(7)
}

/// A utility model that budgets on a four-characters-a-token estimate, can be
/// asked for the server's real count, and answers each call after an optional
/// delay with the next canned reply.
struct UtilityStub {
    window: usize,
    exact: bool,
    replies: Mutex<VecDeque<(Duration, Result<String>)>>,
    prompts: Mutex<Vec<String>>,
    output_caps: Mutex<Vec<Option<u32>>>,
    exact_counts: AtomicUsize,
}

impl UtilityStub {
    fn new(window: usize, exact: bool) -> Self {
        Self {
            window,
            exact,
            replies: Mutex::new(VecDeque::new()),
            prompts: Mutex::new(Vec::new()),
            output_caps: Mutex::new(Vec::new()),
            exact_counts: AtomicUsize::new(0),
        }
    }

    fn then(self, delay: Duration, reply: Result<String>) -> Self {
        self.replies.lock().unwrap().push_back((delay, reply));
        self
    }

    fn calls(&self) -> usize {
        self.prompts.lock().unwrap().len()
    }

    fn last_prompt(&self) -> String {
        self.prompts.lock().unwrap().last().cloned().unwrap()
    }

    /// The whole request as the server counts it.
    fn last_request_tokens(&self) -> usize {
        server_tokens(PLANNER_SYSTEM) + server_tokens(&self.last_prompt())
    }
}

#[async_trait::async_trait]
impl LLMPort for UtilityStub {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.prompts
            .lock()
            .unwrap()
            .push(request.user_text().to_string());
        self.output_caps
            .lock()
            .unwrap()
            .push(request.max_output_tokens);
        let (delay, reply) = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or((Duration::ZERO, Ok(PLAN.to_string())));
        tokio::time::sleep(delay).await;
        reply.map(CompletionResponse::from_text)
    }

    fn model_name(&self) -> &str {
        "utility-stub"
    }

    fn max_context_tokens(&self) -> usize {
        self.window
    }

    fn count_tokens(&self, text: &str) -> usize {
        text.len().div_ceil(4)
    }

    async fn count_tokens_exact(&self, text: &str) -> Result<usize> {
        self.exact_counts.fetch_add(1, Ordering::Relaxed);
        Ok(server_tokens(text))
    }

    fn counts_tokens_exactly(&self) -> bool {
        self.exact
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

fn uuid(index: usize) -> String {
    format!("9c262a35-db4f-4b23-81d1-{index:012}")
}

/// A vault the size of the 2026-10-13 baseline: 379 standalone documents with
/// UUIDs, each with a few hundred characters of opening and some headings.
fn vault() -> Vec<CorpusDocument> {
    (0..379)
        .map(|index| CorpusDocument {
            id: uuid(index),
            name: format!("Record {index:03} - operations notes.md"),
            opening: format!(
                "# Record {index:03}\n\n{}",
                "Replaced the filter, checked the boiler, noted the stiff stopcock. ".repeat(10)
            ),
            chapter_number: None,
            source_context: None,
            sections: (0..12)
                .map(|s| format!("Record {index:03} part {s}"))
                .collect(),
        })
        .collect()
}

fn turn_with(health: &PlannerHealth, candidates: Vec<String>) -> PlannerTurn<'_> {
    let mut turn = PlannerTurn::with_health(health);
    turn.candidates = Some(candidates);
    turn
}

#[tokio::test]
async fn a_large_vault_is_shown_as_a_shortlist_inside_the_prompt_target() {
    let health = PlannerHealth::new();
    let stub = UtilityStub::new(16_384, true);
    let catalog = vault();
    let candidates = vec![uuid(200), uuid(305), uuid(17)];

    plan(
        &stub,
        "Whoever handles logistics for the migration: when does their schedule start?",
        Some(&"user: an earlier question\nassistant: an earlier answer\n".repeat(60)),
        &catalog,
        &HashMap::new(),
        &turn_with(&health, candidates),
        &CancellationToken::new(),
    )
    .await
    .unwrap();

    let used = stub.last_request_tokens();
    assert!(
        used <= PLANNER_PROMPT_TOKENS,
        "a {used}-token planner prompt against a {PLANNER_PROMPT_TOKENS}-token target"
    );
    assert!(
        stub.exact_counts.load(Ordering::Relaxed) > 0,
        "the server's tokenizer was never asked"
    );
    assert_eq!(
        stub.output_caps.lock().unwrap().as_slice(),
        [Some(PLANNER_OUTPUT_TOKENS)]
    );
    let prompt = stub.last_prompt();
    // The hits and their neighbours, not the alphabetical head of the vault.
    for shown in ["Record 200", "Record 199", "Record 201", "Record 305"] {
        assert!(prompt.contains(shown), "{shown} missing from the shortlist");
    }
    assert!(!prompt.contains("Record 000"), "{prompt}");
    // Documents are named by handles, never by their UUIDs.
    assert!(!prompt.contains("9c262a35"), "{prompt}");
    assert!(prompt.contains(r#""catalog_total":379"#), "{prompt}");
}

/// A model whose every text costs one token, so only the row cap can bind.
struct OneTokenStub;

#[async_trait::async_trait]
impl LLMPort for OneTokenStub {
    async fn complete(&self, _request: &CompletionRequest) -> Result<CompletionResponse> {
        Ok(CompletionResponse::from_text(PLAN))
    }

    fn model_name(&self) -> &str {
        "one-token-stub"
    }

    fn max_context_tokens(&self) -> usize {
        16_384
    }

    fn count_tokens(&self, _text: &str) -> usize {
        1
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn the_shortlist_takes_hits_then_neighbours_and_is_capped() {
    let catalog = vault();
    let candidates: Vec<String> = (100..160).map(uuid).collect();
    let ranked = shortlist(&catalog, &candidates, "where is the spare key kept?");
    let ids: Vec<&str> = ranked.iter().map(|d| d.id.as_str()).collect();
    assert_eq!(ids.first().copied(), Some(uuid(100).as_str()));
    // The first hit brings the documents either side of it in its series.
    assert!(ids.iter().take(3).any(|id| *id == uuid(99)));
    assert!(!ids.contains(&uuid(0).as_str()));

    let parts = prompt::PromptParts {
        question: "where is the spare key kept?",
        history: "",
        catalog_total: catalog.len(),
        correction: None,
        summaries: &HashMap::new(),
    };
    let built = prompt::build(
        &OneTokenStub,
        &parts,
        &ranked,
        &catalog,
        PLANNER_PROMPT_TOKENS,
    )
    .await
    .unwrap();
    assert_eq!(built.shown, SHORTLIST_CAP);
}

#[test]
fn without_a_ranking_the_catalog_keeps_its_own_order() {
    let catalog = vault();
    let ranked = shortlist(&catalog, &[], "anything");
    assert_eq!(ranked.len(), catalog.len());
    assert_eq!(ranked.first().map(|d| d.id.clone()), Some(uuid(0)));
}

#[tokio::test]
async fn the_plan_names_documents_by_handle_and_gets_their_real_ids_back() {
    let health = PlannerHealth::new();
    let catalog = vault();
    let stub = UtilityStub::new(16_384, true).then(
        Duration::ZERO,
        Ok(r#"{"queries":["spare key"],"opening_document_ids":["d2","D1","not-a-handle"],"start_at_beginning":false}"#
            .into()),
    );
    let plan = plan(
        &stub,
        "Where is the spare key kept?",
        None,
        &catalog,
        &HashMap::new(),
        &turn_with(&health, vec![uuid(42)]),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    // Handles are numbered in reading order: 41, 42, 43.
    assert_eq!(plan.opening_document_ids, [uuid(42), uuid(41)]);
}

/// A book split into files, with front matter, among unrelated documents.
fn library_with_a_book() -> Vec<CorpusDocument> {
    let group = SourceGroup {
        id: "6b7e2f0a-8a3c-4c1e-9d55-0f6a2b9c1d10".into(),
        title: "Examining manual".into(),
        edition: Some("2026".into()),
        description: None,
        ordered: true,
        structure: StructureMode::Sections,
    };
    let book = [
        "Table of contents",
        "Change summary",
        "Foreword",
        "Introduction",
        "Chapter 100",
        "Chapter 200",
        "Chapter 300",
        "Chapter 400",
        "Chapter 500",
        "Chapter 600",
        "Chapter 700",
    ]
    .iter()
    .enumerate()
    .map(|(position, name)| CorpusDocument {
        id: format!("book-{position:02}"),
        name: name.to_string(),
        opening: name.to_string(),
        chapter_number: CorpusDocument::opening_chapter_number(name),
        sections: Vec::new(),
        source_context: Some(SourceContext {
            group: group.clone(),
            position: position as u32,
        }),
    });
    let mut catalog: Vec<_> = vault().into_iter().take(60).chain(book).collect();
    catalog.sort_by_key(|d| {
        (
            d.source_context
                .as_ref()
                .map(|s| s.group.id.clone())
                .unwrap_or_default(),
            d.source_context.as_ref().map(|s| s.position),
            d.name.clone(),
        )
    });
    catalog
}

#[tokio::test]
async fn a_learning_request_sees_its_books_front_matter_and_opens_it_in_order() {
    let health = PlannerHealth::new();
    let catalog = library_with_a_book();
    let question = "Help me learn the examining manual properly";
    let ranked = shortlist(&catalog, &["book-10".to_string()], question);
    let ids: Vec<&str> = ranked.iter().map(|d| d.id.as_str()).collect();
    // Chapter 700 brings the introduction and chapter 100 with it.
    for needed in ["book-10", "book-03", "book-04"] {
        assert!(ids.contains(&needed), "{needed} missing from {ids:?}");
    }

    // The planner picks one chapter it was shown; validation still reads the
    // whole book to find where it begins.
    let stub = UtilityStub::new(16_384, true).then(
        Duration::ZERO,
        Ok(r#"{"queries":["examining manual introduction"],"opening_document_ids":["d1"],"start_at_beginning":true}"#
            .into()),
    );
    let plan = plan(
        &stub,
        question,
        None,
        &catalog,
        &HashMap::new(),
        &turn_with(&health, vec!["book-10".into()]),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert!(plan.start_at_beginning);
    assert!(plan.opening_document_ids.contains(&"book-03".to_string()));
    assert!(plan.opening_document_ids.contains(&"book-04".to_string()));
    assert!(!plan.opening_document_ids.contains(&"book-01".to_string()));
    // Chapter 200 was never shown to the planner; reading order found it.
    assert!(!stub.last_prompt().contains("Chapter 200"));
    assert!(plan.opening_document_ids.contains(&"book-05".to_string()));
}

#[tokio::test]
async fn a_window_smaller_than_the_target_bounds_the_prompt_on_an_estimate() {
    let window = 2_048;
    let health = PlannerHealth::new();
    let stub = UtilityStub::new(window, false);
    plan(
        &stub,
        "Where is the spare key kept?",
        Some(&"user: earlier\n".repeat(400)),
        &vault(),
        &HashMap::new(),
        &turn_with(&health, (0..40).map(uuid).collect()),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let used = stub.last_request_tokens() + PLANNER_OUTPUT_TOKENS as usize;
    assert!(
        used <= window,
        "{used} tokens with the answer's room against a {window}-token window"
    );
}

#[tokio::test]
async fn a_prompt_that_cannot_fit_the_window_is_never_sent() {
    let health = PlannerHealth::new();
    let stub = UtilityStub::new(600, true);
    let result = plan(
        &stub,
        "Where is the spare key kept?",
        None,
        &vault(),
        &HashMap::new(),
        &turn_with(&health, vec![uuid(1)]),
        &CancellationToken::new(),
    )
    .await;
    assert!(result.is_err());
    assert_eq!(stub.calls(), 0);
    // Not sending is not a timeout: nothing about the planner's speed is learned.
    assert_eq!(health.prompt_tokens(), PLANNER_PROMPT_TOKENS);
}

#[tokio::test(start_paused = true)]
async fn a_timeout_halves_the_next_prompt_instead_of_standing_the_planner_down() {
    let health = PlannerHealth::new();
    let catalog = vault();
    let stub = UtilityStub::new(16_384, true)
        .then(Duration::from_secs(60), Ok(PLAN.into()))
        .then(Duration::ZERO, Ok(PLAN.into()));

    let first = turn_with(&health, (0..40).map(uuid).collect());
    let timed_out = plan(
        &stub,
        "Where is the spare key kept?",
        None,
        &catalog,
        &HashMap::new(),
        &first,
        &CancellationToken::new(),
    )
    .await;
    assert!(timed_out.is_err());
    let sent = stub.last_request_tokens();
    let next = health.prompt_tokens();
    assert!((PLANNER_PROMPT_FLOOR..PLANNER_PROMPT_TOKENS).contains(&next));
    assert!(health.standing_down().is_none());

    // The next turn plans again, smaller.
    let second = turn_with(&health, (0..40).map(uuid).collect());
    assert!(second.unavailable().is_none());
    plan(
        &stub,
        "Where is the spare key kept?",
        None,
        &catalog,
        &HashMap::new(),
        &second,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let resent = stub.last_request_tokens();
    assert!(resent < sent, "{resent} tokens after a timeout at {sent}");
}

#[tokio::test(start_paused = true)]
async fn the_corrective_retry_replans_on_the_turns_after_a_timeout_but_not_its_own() {
    let health = PlannerHealth::new();
    let catalog = vault();
    let stub = UtilityStub::new(16_384, true)
        .then(Duration::from_secs(60), Ok(PLAN.into()))
        .then(
            Duration::ZERO,
            Ok(r#"{"queries":["spare key cabinet"],"opening_document_ids":[],"start_at_beginning":false}"#.into()),
        );
    let correction = CorrectionRequest {
        queries_already_tried: vec!["spare key".into()],
        top_result_titles: Vec::new(),
        why_insufficient: vec!["low_top_score"],
    };

    let turn = turn_with(&health, vec![uuid(5)]);
    assert!(plan(
        &stub,
        "Where is the spare key kept?",
        None,
        &catalog,
        &HashMap::new(),
        &turn,
        &CancellationToken::new(),
    )
    .await
    .is_err());
    // The turn that already waited out a timeout keeps its first-pass evidence.
    assert!(turn.unavailable().is_some());
    assert!(plan_correction(
        &stub,
        "Where is the spare key kept?",
        None,
        &catalog,
        &correction,
        &turn,
        &CancellationToken::new(),
    )
    .await
    .is_err());
    assert_eq!(stub.calls(), 1);

    // The next turn's correction replans.
    let next = turn_with(&health, vec![uuid(5)]);
    let retry = plan_correction(
        &stub,
        "Where is the spare key kept?",
        None,
        &catalog,
        &correction,
        &next,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(retry.queries, ["spare key cabinet"]);
}

#[tokio::test(start_paused = true)]
async fn only_a_timeout_at_the_smallest_prompt_stands_the_planner_down_and_only_briefly() {
    let health = PlannerHealth::new();
    let mut timeouts = 0;
    while health.prompt_tokens() > PLANNER_PROMPT_FLOOR {
        health.timed_out(health.prompt_tokens());
        assert!(health.standing_down().is_none());
        timeouts += 1;
    }
    assert_eq!(timeouts, 2, "2560 -> 1280 -> 1024");
    health.timed_out(PLANNER_PROMPT_FLOOR);
    let remaining = health.standing_down().expect("standing down");
    assert!(remaining <= PLANNER_STAND_DOWN);
    assert!(PlannerTurn::with_health(&health).unavailable().is_some());

    tokio::time::advance(PLANNER_STAND_DOWN + Duration::from_secs(1)).await;
    assert!(health.standing_down().is_none());
    assert!(PlannerTurn::with_health(&health).unavailable().is_none());
}

#[test]
fn a_quick_answer_earns_back_prompt_room_and_a_slow_one_keeps_it() {
    let health = PlannerHealth::new();
    health.timed_out(PLANNER_PROMPT_TOKENS);
    let shrunk = health.prompt_tokens();
    health.answered(PLANNER_TIMEOUT * 3 / 4);
    assert_eq!(health.prompt_tokens(), shrunk);
    health.answered(Duration::from_secs(4));
    assert_eq!(health.prompt_tokens(), PLANNER_PROMPT_TOKENS);
    health.answered(Duration::from_secs(1));
    assert_eq!(health.prompt_tokens(), PLANNER_PROMPT_TOKENS);
}

#[tokio::test]
async fn a_window_refusal_shrinks_the_prompt_without_standing_down() {
    let health = PlannerHealth::new();
    let stub = UtilityStub::new(16_384, true).then(
        Duration::ZERO,
        Err(AppError::Network(
            "llama-server stream error (400 exceed_context_size_error)".into(),
        )),
    );
    let result = plan(
        &stub,
        "Where is the spare key kept?",
        None,
        &vault(),
        &HashMap::new(),
        &turn_with(&health, vec![uuid(3)]),
        &CancellationToken::new(),
    )
    .await;
    assert!(result.is_err());
    assert!(health.prompt_tokens() < PLANNER_PROMPT_TOKENS);
    assert!(health.standing_down().is_none());
    assert!(PlannerTurn::with_health(&health).unavailable().is_none());
}

#[tokio::test]
async fn an_unusable_answer_still_falls_back_to_the_local_plan() {
    let health = PlannerHealth::new();
    let catalog = vault();
    let stub = UtilityStub::new(16_384, true).then(Duration::ZERO, Ok("not json".into()));
    let question = "Where is the spare key kept?";
    let planned = plan(
        &stub,
        question,
        None,
        &catalog,
        &HashMap::new(),
        &turn_with(&health, vec![uuid(3)]),
        &CancellationToken::new(),
    )
    .await
    .unwrap_or_else(|_| fallback_with_catalog(question, &catalog));
    assert_eq!(planned.queries, [question]);
    // A bad answer is still an answer in time.
    assert_eq!(health.prompt_tokens(), PLANNER_PROMPT_TOKENS);
}
