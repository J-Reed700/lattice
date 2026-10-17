//! The planner on a vault too large to show whole: which documents it is
//! shown, how its prompt is sized from its window, and that no clock cuts it
//! off.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use super::health::PlannerHealth;
use super::prompt::{window_room, PLANNER_OUTPUT_TOKENS};
use super::shortlist::shortlist;
use super::*;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::value_objects::source_context::{
    SourceContext, SourceGroup, StructureMode,
};
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
    /// Whether each request opted out of the adapter's elapsed-time limit.
    timeless: Mutex<Vec<bool>>,
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
            timeless: Mutex::new(Vec::new()),
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
        self.timeless.lock().unwrap().push(request.no_time_limit);
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

/// Plan from the vault on `stub` and return the request's size as the server
/// counts it.
async fn planned_size(stub: &UtilityStub) -> usize {
    let health = PlannerHealth::default();
    plan(
        stub,
        "Whoever handles logistics for the migration: when does their schedule start?",
        Some(&"user: an earlier question\nassistant: an earlier answer\n".repeat(60)),
        &vault(),
        &HashMap::new(),
        &turn_with(&health, vec![uuid(200), uuid(305), uuid(17)]),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    stub.last_request_tokens()
}

#[tokio::test]
async fn a_large_vault_is_shown_as_a_shortlist_that_fills_the_window() {
    let stub = UtilityStub::new(16_384, true);
    let used = planned_size(&stub).await;
    let room = window_room(&stub);
    assert!(
        used <= room,
        "a {used}-token planner prompt against a {room}-token window"
    );
    assert!(
        stub.exact_counts.load(Ordering::Relaxed) > 0,
        "the server's tokenizer was never asked"
    );
    assert_eq!(
        stub.output_caps.lock().unwrap().as_slice(),
        [Some(PLANNER_OUTPUT_TOKENS)]
    );
    assert_eq!(stub.timeless.lock().unwrap().as_slice(), [true]);
    let prompt = stub.last_prompt();
    // The hits and their neighbours, not the alphabetical head of the vault.
    for shown in ["Record 200", "Record 199", "Record 201", "Record 305"] {
        assert!(prompt.contains(shown), "{shown} missing from the shortlist");
    }
    // There is no fixed row cap and no reading-speed target: the rest of the
    // vault follows the hits and fills what the window leaves.
    assert!(used * 10 >= room * 8, "{used} of {room} tokens");
    // Documents are named by handles, never by their UUIDs.
    assert!(!prompt.contains("9c262a35"), "{prompt}");
    assert!(prompt.contains(r#""catalog_total":379"#), "{prompt}");
}

/// The prompt grows with the planner's window. It used to be held near
/// 2,560 tokens, what a 9B model on one machine read inside the planner's
/// 20-second timeout, whatever the window.
#[tokio::test]
async fn the_planners_prompt_is_sized_from_its_window() {
    let small = UtilityStub::new(8_192, true);
    let large = UtilityStub::new(32_768, true);
    let (small_used, large_used) = (planned_size(&small).await, planned_size(&large).await);
    for (stub, used) in [(&small, small_used), (&large, large_used)] {
        let room = window_room(stub);
        assert!(used <= room && used * 10 >= room * 8, "{used} of {room}");
    }
    assert!(large_used > small_used * 3, "{small_used} -> {large_used}");
    assert!(small_used > 2_560 * 2, "{small_used}");
}

/// A model whose every text costs one token, so the budget never binds.
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
async fn the_shortlist_takes_hits_then_neighbours_and_the_budget_alone_bounds_it() {
    let catalog = vault();
    let candidates: Vec<String> = (100..160).map(uuid).collect();
    let ranked = shortlist(&catalog, &candidates, "where is the spare key kept?");
    let ids: Vec<&str> = ranked.iter().map(|d| d.id.as_str()).collect();
    assert_eq!(ids.first().copied(), Some(uuid(100).as_str()));
    // The first hit brings the documents either side of it in its series.
    assert!(ids.iter().take(3).any(|id| *id == uuid(99)));
    // The rest of the catalog follows the hits and their neighbours.
    let position = |id: &str| ids.iter().position(|shown| *shown == id).unwrap();
    assert!(position(&uuid(0)) > position(&uuid(159)));
    assert_eq!(ids.len(), catalog.len());

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
        window_room(&OneTokenStub),
    )
    .await
    .unwrap();
    // The old cap showed twenty rows whatever the budget. A window with room
    // shows every document, so the planner can open any of them.
    assert_eq!(built.shown, catalog.len());
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
    let health = PlannerHealth::default();
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
    // Handles are numbered in reading order over every row shown. The hit
    // and its neighbours (41, 42, 43) come first in the shortlist, and the
    // rest of the vault fills what the budget leaves, so the vault's first
    // documents are shown too and take the first handles.
    assert_eq!(plan.opening_document_ids, [uuid(1), uuid(0)]);
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
    let health = PlannerHealth::default();
    // A window small enough that the shortlist binds, so part of the book is
    // left out of the prompt.
    let window = 3_000;
    let catalog = library_with_a_book();
    let question = "Help me learn the examining manual properly";
    let ranked = shortlist(&catalog, &["book-10".to_string()], question);
    let ids: Vec<&str> = ranked.iter().map(|d| d.id.as_str()).collect();
    // Chapter 700 brings the introduction and chapter 100 with it.
    for needed in ["book-10", "book-03", "book-04"] {
        assert!(ids.contains(&needed), "{needed} missing from {ids:?}");
    }

    // The planner picks one chapter it was shown; validation still reads the
    // whole book to find where it begins. Rows are handled in reading order
    // over everything the budget shows, so the chapter's handle is read off
    // the same prompt the planner gets.
    let parts = prompt::PromptParts {
        question,
        history: "",
        catalog_total: catalog.len(),
        correction: None,
        summaries: &HashMap::new(),
    };
    let sizing = UtilityStub::new(window, true);
    let built = prompt::build(&sizing, &parts, &ranked, &catalog, window_room(&sizing))
        .await
        .unwrap();
    let chapter = built
        .handles
        .iter()
        .find(|(_, id)| id.as_str() == "book-10")
        .map(|(handle, _)| handle.clone())
        .expect("the hit is shown");
    let stub = UtilityStub::new(window, true).then(
        Duration::ZERO,
        Ok(format!(
            r#"{{"queries":["examining manual introduction"],"opening_document_ids":["{chapter}"],"start_at_beginning":true}}"#
        )),
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
async fn a_small_window_bounds_the_prompt_on_an_estimate() {
    let window = 2_048;
    let health = PlannerHealth::default();
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
    let health = PlannerHealth::default();
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
    // Not sending is not a refusal: nothing about the window is learned.
    assert_eq!(health.prompt_tokens(&stub), window_room(&stub));
}

/// The planner used to be cut off after 20 seconds and the turn fell back to
/// the local plan. A planner that answers after 30 seconds is slow, not dead:
/// its plan is used, and the next attempt is not made smaller for it.
#[tokio::test(start_paused = true)]
async fn a_slow_planner_is_waited_for() {
    let health = PlannerHealth::default();
    let stub = UtilityStub::new(16_384, true).then(Duration::from_secs(30), Ok(PLAN.into()));
    let started = tokio::time::Instant::now();
    let planned = plan(
        &stub,
        "Where is the spare key kept?",
        None,
        &vault(),
        &HashMap::new(),
        &turn_with(&health, (0..40).map(uuid).collect()),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert!(started.elapsed() >= Duration::from_secs(30));
    assert_eq!(planned.queries, ["garden leave notice"]);
    assert_eq!(health.prompt_tokens(&stub), window_room(&stub));
}

/// A turn whose first plan was slow still gets its corrective retry: there is
/// no per-turn "already timed out" guard to skip it.
#[tokio::test(start_paused = true)]
async fn the_corrective_retry_replans_after_a_slow_first_pass() {
    let health = PlannerHealth::default();
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
        more_titles_not_shown: 0,
    };

    let turn = turn_with(&health, vec![uuid(5)]);
    plan(
        &stub,
        "Where is the spare key kept?",
        None,
        &catalog,
        &HashMap::new(),
        &turn,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let retry = plan_correction(
        &stub,
        "Where is the spare key kept?",
        None,
        &catalog,
        &correction,
        &turn,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(stub.calls(), 2);
    assert_eq!(retry.queries, ["spare key cabinet"]);
}

/// A stop press still ends a planner call nobody wants any more: the request
/// carries the turn's cancellation, which the backend's scheduler honours.
#[tokio::test]
async fn the_planners_request_carries_the_turns_stop() {
    struct Cancellable;
    #[async_trait::async_trait]
    impl LLMPort for Cancellable {
        async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
            match &request.cancel {
                Some(cancel) => {
                    cancel.cancelled().await;
                    Err(AppError::InvalidState("cancelled".into()))
                }
                None => Ok(CompletionResponse::from_text(PLAN)),
            }
        }
        fn model_name(&self) -> &str {
            "cancellable"
        }
        fn max_context_tokens(&self) -> usize {
            16_384
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }
    let health = PlannerHealth::default();
    let cancel = CancellationToken::new();
    cancel.cancel();
    let result = plan(
        &Cancellable,
        "Where is the spare key kept?",
        None,
        &vault(),
        &HashMap::new(),
        &turn_with(&health, vec![uuid(3)]),
        &cancel,
    )
    .await;
    assert!(result.is_err());
}

/// A server whose real window is smaller than the one its port reports
/// refuses the prompt. The next attempt is half of what was refused, and
/// stays under that for the runtime's life.
#[tokio::test]
async fn a_window_refusal_halves_the_next_prompt() {
    let health = PlannerHealth::default();
    let stub = UtilityStub::new(16_384, true).then(
        Duration::ZERO,
        Err(AppError::Network(
            "llama-server stream error (400 exceed_context_size_error)".into(),
        )),
    );
    let (catalog, summaries, cancel) = (vault(), HashMap::new(), CancellationToken::new());
    let turn = turn_with(&health, vec![uuid(3)]);
    let question = "Where is the spare key kept?";
    let ask = || plan(&stub, question, None, &catalog, &summaries, &turn, &cancel);
    assert!(ask().await.is_err());
    let refused = stub.last_request_tokens();
    assert_eq!(health.prompt_tokens(&stub), refused / 2);
    ask().await.unwrap();
    let resent = stub.last_request_tokens();
    assert!(
        resent <= refused / 2,
        "{resent} tokens after a refusal at {refused}"
    );
    // The planner is asked again on the very next attempt: nothing stands down.
    assert_eq!(stub.calls(), 2);
}

#[tokio::test]
async fn an_unusable_answer_still_falls_back_to_the_local_plan() {
    let health = PlannerHealth::default();
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
    // A bad answer is not a refusal: it never shrinks the prompt.
    assert_eq!(health.prompt_tokens(&stub), window_room(&stub));
}

/// The planner's excerpt of the conversation was cut by characters from its
/// start, mid-message and without a word. It keeps whole lines, newest
/// first, and counts the ones left out.
#[test]
fn the_planners_history_is_whole_lines_with_the_rest_counted() {
    let stub = UtilityStub::new(16_384, true);
    let history = (0..400)
        .map(|n| format!("user: question number {n} about the spare key"))
        .collect::<Vec<_>>()
        .join("\n");
    let kept = super::prompt::recent_history(&stub, Some(&history), window_room(&stub));
    let mut lines = kept.lines();
    assert!(lines
        .next()
        .unwrap()
        .contains("earlier lines of the conversation not shown"));
    assert!(lines.all(|line| line.starts_with("user: question number ")));
    assert!(kept.ends_with("question number 399 about the spare key"));
}

/// The planner's share of the conversation is a share of its window, and a
/// refusal that halves the prompt halves it too.
#[test]
fn the_planners_history_is_a_share_of_its_window() {
    let stub = UtilityStub::new(16_384, true);
    let health = PlannerHealth::default();
    let before = PlannerTurn::with_health(&health).history_tokens(&stub);
    assert_eq!(before, window_room(&stub) / 5);
    health.overflowed("utility-stub", window_room(&stub));
    assert_eq!(
        PlannerTurn::with_health(&health).history_tokens(&stub),
        window_room(&stub) / 2 / 5
    );
}
