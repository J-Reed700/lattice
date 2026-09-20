//! Turn assembly against a real database.
//!
//! Uses the production repository and migration rather than a fake port: the
//! things most worth checking here are that the switch actually gates the path
//! and that a real snapshot plus real recall reach the assembler intact.

use super::*;
use crate::features::conversation::repository::ConversationRepository;
use crate::features::llm::engine::factory::MockLLMPort;
use sqlx::SqlitePool;

async fn database() -> SqlitePool {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .unwrap();
    pool
}

/// A conversation with `count` turns, written through the production path so
/// sequences and digests are allocated the way they are in production.
async fn seed(pool: &SqlitePool, count: usize) -> String {
    let repository = ConversationRepository::new(pool.clone());
    let conversation = repository
        .create_conversation("Memory turn", "test-model", None)
        .await
        .unwrap();
    let id = conversation.id.to_string();
    for index in 0..count {
        let (role, content) = if index == 0 {
            (
                crate::domain::conversation::MessageRole::User,
                "Do not deploy until I approve.".to_string(),
            )
        } else if index % 2 == 1 {
            (
                crate::domain::conversation::MessageRole::Assistant,
                format!("Answer {index}."),
            )
        } else {
            (
                crate::domain::conversation::MessageRole::User,
                format!("Question {index}."),
            )
        };
        repository
            .add_message(&id, role, &content, 10, None)
            .await
            .unwrap();
    }
    id
}

/// Commit one mandatory constraint quoting the first message.
async fn record_constraint(pool: &SqlitePool, conversation_id: &str) {
    record_constraint_through(pool, conversation_id, None).await;
}

async fn record_constraint_through(pool: &SqlitePool, conversation_id: &str, through: Option<i64>) {
    use crate::application::ports::conversation_memory::{
        MemoryCommitCandidate, MemoryCommitPreconditions, SummaryUpdate,
    };
    use crate::domain::conversation_memory::{
        compute_digest, EvidencePurpose, EvidenceSpan, MemoryCommit, MemoryId, MemoryItem,
        MemoryKind, MemoryReview, MemoryState, SourceRole,
    };

    let repository = ConversationRepository::new(pool.clone());
    let snapshot = repository
        .load_memory_snapshot(conversation_id)
        .await
        .unwrap();

    #[derive(sqlx::FromRow)]
    struct Row {
        id: String,
        content: String,
        sequence: i64,
    }
    let first = sqlx::query_as::<_, Row>(
        "SELECT id, content, sequence FROM conversation_messages \
         WHERE conversation_id = ? ORDER BY sequence ASC LIMIT 1",
    )
    .bind(conversation_id)
    .fetch_one(pool)
    .await
    .unwrap();

    let item = MemoryItem {
        id: MemoryId::new(),
        conversation_id: conversation_id.to_string(),
        kind: MemoryKind::Constraint,
        state: MemoryState::Active,
        label: "Approval required before deploy".into(),
        evidence: vec![EvidenceSpan {
            message_id: first.id.clone(),
            sequence: first.sequence,
            role: SourceRole::User,
            start_byte: 0,
            end_byte: first.content.len() as u32,
            content_digest: compute_digest(&first.content),
            purpose: EvidencePurpose::Assertion,
        }],
        created_at_sequence: first.sequence,
        changed_at_sequence: first.sequence,
        superseded_by: None,
        revision: 0,
        review: MemoryReview::Supported,
        related_item_ids: Vec::new(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    repository
        .commit_memory(
            &MemoryCommitPreconditions {
                conversation_id: conversation_id.to_string(),
                expected_transcript_revision: snapshot.transcript_revision,
                expected_memory_revision: 0,
                operation_id: "seed".into(),
            },
            &MemoryCommitCandidate {
                commit: MemoryCommit {
                    inserts: vec![item],
                    updates: Vec::new(),
                    summary: None,
                    processed_through_sequence: through.unwrap_or(snapshot.latest_sequence),
                },
                summary: Some(SummaryUpdate {
                    summary_text: "The user is setting up a deployment workflow.".into(),
                    up_to_message_id: first.id,
                    original_message_count: 2,
                    original_tokens: 20,
                    summary_tokens: 6,
                }),
                source_message_ids: Vec::new(),
                transcript_revision: snapshot.transcript_revision,
                extractor_model_identity: Some("test".into()),
                extractor_prompt_version: Some("v1".into()),
                validator_version: Some("v1".into()),
                operation: "seed".into(),
            },
        )
        .await
        .unwrap();
}

fn llm() -> Arc<dyn LLMPort> {
    Arc::new(MockLLMPort::new())
}

#[tokio::test]
async fn the_switch_being_off_leaves_the_existing_path_entirely_alone() {
    let pool = database().await;
    let id = seed(&pool, 6).await;
    record_constraint(&pool, &id).await;
    let repository = ConversationRepository::new(pool.clone());

    let built = build_memory_plan(
        &repository,
        &repository,
        &llm(),
        &id,
        "You are helpful.",
        "What is the status?",
        0,
        Vec::new(),
        false,
    )
    .await
    .unwrap();
    assert!(
        built.is_none(),
        "nothing may be assembled while the rollout switch is off"
    );
}

#[tokio::test]
async fn a_conversation_without_memory_is_checked_and_keeps_its_raw_history() {
    let pool = database().await;
    let id = seed(&pool, 4).await;
    let repository = ConversationRepository::new(pool.clone());

    let built = build_memory_plan(
        &repository,
        &repository,
        &llm(),
        &id,
        "You are helpful.",
        "What is the status?",
        0,
        Vec::new(),
        true,
    )
    .await
    .unwrap();
    let plan = built
        .expect("enabled memory always checks source coverage")
        .plan;
    assert!(!plan.compaction_required);
    assert!(plan.messages.iter().any(|message| matches!(message,
        crate::application::ports::llm_port::CompletionInput::Message { content, .. }
            if content == "Do not deploy until I approve."
    )));
}

#[tokio::test]
async fn a_long_conversation_that_was_never_compacted_still_asks_for_compaction() {
    let pool = database().await;
    // More messages than the candidate window, and no ledger at all. Before the
    // out-of-reach check this returned `None`: the assembler was never consulted,
    // so nothing ever reported that the oldest messages had silently stopped
    // reaching the model, and the automatic trigger could not fire on the one
    // shape of conversation that most needs it.
    let id = seed(&pool, RECENT_CANDIDATE_MESSAGES + 8).await;
    let repository = ConversationRepository::new(pool.clone());

    let built = build_memory_plan(
        &repository,
        &repository,
        &llm(),
        &id,
        "You are helpful.",
        "What is the status?",
        0,
        Vec::new(),
        true,
    )
    .await
    .unwrap()
    .expect("source out of the assembler's reach must engage the memory path");

    assert!(
        built.plan.compaction_required,
        "messages neither in the ledger nor in the window have nothing standing in for them"
    );
}

#[tokio::test]
async fn a_recorded_constraint_reaches_the_prompt_with_its_exact_words() {
    let pool = database().await;
    let id = seed(&pool, 8).await;
    record_constraint(&pool, &id).await;
    let repository = ConversationRepository::new(pool.clone());

    let built = build_memory_plan(
        &repository,
        &repository,
        &llm(),
        &id,
        "You are helpful.",
        "Can you ship it now?",
        0,
        Vec::new(),
        true,
    )
    .await
    .unwrap()
    .expect("a conversation with memory assembles a plan");

    let rendered: Vec<(String, String)> = built
        .plan
        .messages
        .iter()
        .map(|message| match message {
            crate::application::ports::llm_port::CompletionInput::Message { role, content } => {
                (role.clone(), content.clone())
            }
            other => panic!("unexpected native input {other:?}"),
        })
        .collect();

    // The constraint is present, quoted, and attributed to the user.
    let block = rendered
        .iter()
        .find(|(role, content)| role == "user" && content.starts_with("[recorded requirements"))
        .expect("the constraint must reach the prompt");
    assert!(block.1.contains("Do not deploy until I approve."));

    // The summary is present as assistant context, not as policy.
    let summary = rendered
        .iter()
        .find(|(_, content)| content.starts_with("[generated summary"))
        .expect("the summary must be carried");
    assert_eq!(summary.0, "assistant");

    // The current input is last and appears once.
    assert_eq!(rendered.last().unwrap().1, "Can you ship it now?");
    assert_eq!(
        rendered
            .iter()
            .filter(|(_, content)| content == "Can you ship it now?")
            .count(),
        1
    );

    assert_eq!(built.plan.accounting.active_mandatory_count, 1);
    assert!(built.plan.accounting.fits());
    assert!(built.plan.max_output_tokens > 0);
    assert_eq!(built.plan.memory_revision, 1);
}

#[tokio::test]
async fn a_failed_assistant_message_is_not_replayed_as_an_answer() {
    let pool = database().await;
    let id = seed(&pool, 6).await;
    record_constraint(&pool, &id).await;
    // Mark one assistant message failed: it is not an answer that was returned.
    sqlx::query(
        "UPDATE conversation_messages SET status = 'failed' \
         WHERE conversation_id = ? AND role = 'assistant' AND sequence = 2",
    )
    .bind(&id)
    .execute(&pool)
    .await
    .unwrap();
    let repository = ConversationRepository::new(pool.clone());

    let built = build_memory_plan(
        &repository,
        &repository,
        &llm(),
        &id,
        "You are helpful.",
        "Continue.",
        0,
        Vec::new(),
        true,
    )
    .await
    .unwrap()
    .expect("a plan");

    // A status change alters what the prompt may carry, so it bumps the
    // transcript revision — but it does not touch message *content*, so every
    // evidence span still resolves and the ledger stays usable. Invalidating
    // here would throw away a correct ledger on every ordinary turn completion.
    let state = repository.load_memory_state(&id).await.unwrap();
    assert_eq!(
        state.validity,
        crate::domain::conversation_memory::MemoryValidity::Ready,
        "a status change must not invalidate memory whose evidence is unmoved"
    );

    let rendered: Vec<String> = built
        .plan
        .messages
        .iter()
        .filter_map(|message| match message {
            crate::application::ports::llm_port::CompletionInput::Message { content, .. } => {
                Some(content.clone())
            }
            _ => None,
        })
        .collect();
    assert!(
        !rendered.iter().any(|content| content == "Answer 1."),
        "a failed assistant output is not an answer that was returned, so it must not be \
         replayed as one"
    );
    // The user's own turns are unaffected.
    assert!(rendered.iter().any(|content| content == "Question 2."));
}

#[tokio::test]
async fn recall_availability_is_reported_even_when_nothing_was_found() {
    let pool = database().await;
    let id = seed(&pool, 8).await;
    record_constraint(&pool, &id).await;
    let repository = ConversationRepository::new(pool.clone());

    let built = build_memory_plan(
        &repository,
        &repository,
        &llm(),
        &id,
        "You are helpful.",
        "zzzzqqqq nonexistent terminology",
        0,
        Vec::new(),
        true,
    )
    .await
    .unwrap()
    .expect("a plan");

    assert!(
        !built.recall_note.is_empty(),
        "the availability flag is rendered whether or not anything was found"
    );
    // A miss must be stated as a miss, with the hedge attached: "found nothing"
    // and "the user never said it" are different claims, and the note has to be
    // the first one.
    let note = built.recall_note.to_lowercase();
    assert!(note.contains("found nothing"), "{}", built.recall_note);
    assert!(
        note.contains("not evidence"),
        "a no-hit note must say it is not proof of absence: {}",
        built.recall_note
    );
}

#[tokio::test]
async fn an_oversized_current_message_is_an_error_rather_than_a_clipped_instruction() {
    let pool = database().await;
    let id = seed(&pool, 8).await;
    record_constraint(&pool, &id).await;
    let repository = ConversationRepository::new(pool.clone());

    // MockLLMPort advertises a small window; a message far larger than it cannot
    // be carried, and must not be silently shortened into a different request.
    let huge = "word ".repeat(200_000);
    let error = build_memory_plan(
        &repository,
        &repository,
        &llm(),
        &id,
        "You are helpful.",
        &huge,
        0,
        Vec::new(),
        true,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(error, crate::shared::error::AppError::InvalidInput(_)),
        "{error:?}"
    );
}

mod preparation;
