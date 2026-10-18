//! Generation must never consume a plan with missing unprocessed history.

use super::*;

#[tokio::test]
async fn missing_utility_and_failed_compaction_never_release_an_incomplete_plan() {
    for failure in [
        AppError::ServiceNotAvailable("No utility model is available".into()),
        AppError::InvalidState("Utility model request failed".into()),
    ] {
        let pool = database().await;
        let id = seed(&pool, LONG_CONVERSATION).await;
        let repository = ConversationRepository::new(pool.clone());
        let model = llm();
        let before = repository.load_memory_snapshot(&id).await.unwrap();
        let attempts = std::cell::Cell::new(0);
        let result = prepare_memory_turn(
            || {
                build_memory_plan(
                    &repository,
                    &repository,
                    &model,
                    &id,
                    "You are helpful.",
                    "Deploy now.",
                    0,
                    Vec::new(),
                    true,
                )
            },
            || async {
                attempts.set(attempts.get() + 1);
                Err(failure)
            },
        )
        .await;
        let error = result.unwrap_err().to_string();
        assert!(error.contains("This turn was stopped"), "{error}");
        assert!(error.contains("/compact"), "{error}");
        assert_eq!(attempts.get(), 1);
        let after = repository.load_memory_snapshot(&id).await.unwrap();
        assert_eq!(before.transcript_revision, after.transcript_revision);
        assert_eq!(before.state.memory_revision, after.state.memory_revision);
        assert_eq!(before.latest_sequence, after.latest_sequence);
    }
}

#[tokio::test]
async fn successful_noop_cannot_release_an_incomplete_plan_or_retry_forever() {
    let pool = database().await;
    let id = seed(&pool, LONG_CONVERSATION).await;
    let repository = ConversationRepository::new(pool.clone());
    let model = llm();
    let builds = std::cell::Cell::new(0);
    let attempts = std::cell::Cell::new(0);
    let result = prepare_memory_turn(
        || {
            builds.set(builds.get() + 1);
            build_memory_plan(
                &repository,
                &repository,
                &model,
                &id,
                "You are helpful.",
                "Deploy now.",
                0,
                Vec::new(),
                true,
            )
        },
        || async {
            attempts.set(attempts.get() + 1);
            Ok(())
        },
    )
    .await;
    assert!(result.is_err());
    assert_eq!(builds.get(), 2, "recheck coverage even after a no-op");
    assert_eq!(
        attempts.get(),
        1,
        "never loop model calls until all history fits"
    );
}

#[tokio::test]
async fn partial_commit_is_preserved_but_does_not_authorize_generation_with_missing_source() {
    let pool = database().await;
    let id = seed(&pool, LONG_CONVERSATION).await;
    let repository = ConversationRepository::new(pool.clone());
    let model = llm();
    let result = prepare_memory_turn(
        || {
            build_memory_plan(
                &repository,
                &repository,
                &model,
                &id,
                "You are helpful.",
                "Deploy now.",
                0,
                Vec::new(),
                true,
            )
        },
        || async {
            record_constraint_through(&pool, &id, Some(1)).await;
            Ok(())
        },
    )
    .await;
    assert!(result.is_err());
    let after = repository.load_memory_snapshot(&id).await.unwrap();
    assert_eq!(
        after.state.memory_revision, 1,
        "keep the valid partial commit"
    );
    assert_eq!(after.state.processed_through_sequence, 1);
    assert_eq!(after.latest_sequence, LONG_CONVERSATION as i64);
}

#[tokio::test]
async fn a_rebuilt_complete_plan_can_continue_with_the_original_restriction() {
    let pool = database().await;
    let id = seed(&pool, LONG_CONVERSATION).await;
    let repository = ConversationRepository::new(pool.clone());
    let model = llm();
    let result = prepare_memory_turn(
        || {
            build_memory_plan(
                &repository,
                &repository,
                &model,
                &id,
                "You are helpful.",
                "Deploy now.",
                0,
                Vec::new(),
                true,
            )
        },
        || async {
            record_constraint(&pool, &id).await;
            Ok(())
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert!(!result.plan.compaction_required);
    assert!(result.plan.accounting.fits());
    assert_eq!(result.plan.memory_revision, 1);
    assert!(result.plan.messages.iter().any(|message| matches!(message,
        lattice_core::application::ports::llm_port::CompletionInput::Message { content, .. }
            if content.contains("Do not deploy until I approve.")
    )));
}

#[tokio::test]
async fn complete_raw_history_and_disabled_memory_do_not_need_a_utility_model() {
    let pool = database().await;
    let id = seed(&pool, 4).await;
    let repository = ConversationRepository::new(pool.clone());
    let model = llm();
    for enabled in [true, false] {
        let result = prepare_memory_turn(
            || {
                build_memory_plan(
                    &repository,
                    &repository,
                    &model,
                    &id,
                    "You are helpful.",
                    "Continue.",
                    0,
                    Vec::new(),
                    enabled,
                )
            },
            || async { panic!("a complete raw prompt does not require compaction") },
        )
        .await
        .unwrap();
        assert_eq!(result.is_some(), enabled);
    }
}

#[tokio::test]
async fn fewer_than_64_messages_can_still_require_compaction_before_the_first_ledger() {
    let pool = database().await;
    let id = seed(&pool, 4).await;
    let repository = ConversationRepository::new(pool.clone());
    repository
        .add_message(
            &id,
            lattice_core::domain::conversation::MessageRole::User,
            &"A long requirement. ".repeat(400),
            2000,
            None,
        )
        .await
        .unwrap();
    repository
        .add_message(
            &id,
            lattice_core::domain::conversation::MessageRole::Assistant,
            "Acknowledged.",
            5,
            None,
        )
        .await
        .unwrap();
    let plan = build_memory_plan(
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
    .unwrap()
    .plan;
    assert!(
        plan.compaction_required,
        "message count is not a token budget"
    );
}

#[tokio::test]
async fn invalidated_memory_must_recheck_raw_history_instead_of_falling_back() {
    let pool = database().await;
    let id = seed(&pool, LONG_CONVERSATION).await;
    record_constraint(&pool, &id).await;
    sqlx::query("UPDATE conversation_messages SET content = 'Do not deploy at all.' WHERE conversation_id = ? AND sequence = 1")
        .bind(&id).execute(&pool).await.unwrap();
    let repository = ConversationRepository::new(pool.clone());
    assert!(!repository
        .load_memory_snapshot(&id)
        .await
        .unwrap()
        .is_usable());
    let model = llm();
    let result = prepare_memory_turn(
        || {
            build_memory_plan(
                &repository,
                &repository,
                &model,
                &id,
                "You are helpful.",
                "Deploy now.",
                0,
                Vec::new(),
                true,
            )
        },
        || async { Ok(()) },
    )
    .await;
    assert!(
        result.is_err(),
        "invalid memory cannot cover the omitted original restriction"
    );
}

#[tokio::test]
async fn a_byte_limited_source_page_reports_its_missing_tail() {
    let pool = database().await;
    let id = seed(&pool, 0).await;
    let repository = ConversationRepository::new(pool.clone());
    // Three of 400 KB: one source page (1 MiB) holds the newest two.
    for _ in 0..3 {
        repository
            .add_message(
                &id,
                lattice_core::domain::conversation::MessageRole::User,
                &"x".repeat(400_000),
                100_000,
                None,
            )
            .await
            .unwrap();
    }
    let snapshot = repository.load_memory_snapshot(&id).await.unwrap();
    // No ledger: nothing stands in for the message the cap left unread.
    let RecentCandidates {
        messages: recent,
        unread_source: unread,
        reaches_start,
    } = load_recent(&repository, &id, &snapshot, 0, 1_000, &estimate)
        .await
        .unwrap();
    assert!(
        !reaches_start,
        "the first message was not read, so the window cannot claim the whole transcript"
    );
    assert_eq!(recent.len(), 2);
    assert_eq!(
        recent[1].sequence, snapshot.latest_sequence,
        "the newest is kept"
    );
    assert!(
        unread,
        "a byte cap must not silently hide unprocessed source"
    );
}

#[tokio::test]
async fn a_fallback_after_compaction_cannot_bypass_the_generation_gate() {
    let pool = database().await;
    let id = seed(&pool, LONG_CONVERSATION).await;
    let repository = ConversationRepository::new(pool.clone());
    let model = llm();
    let enabled = std::cell::Cell::new(true);
    let result = prepare_memory_turn(
        || {
            build_memory_plan(
                &repository,
                &repository,
                &model,
                &id,
                "You are helpful.",
                "Deploy now.",
                0,
                Vec::new(),
                enabled.get(),
            )
        },
        || async {
            enabled.set(false);
            Ok(())
        },
    )
    .await;
    assert!(
        result.is_err(),
        "losing the typed plan is not proof that the missing source fits"
    );
}

/// A model with a 128K window and the default estimate.
struct LargeWindow;

#[async_trait::async_trait]
impl LLMPort for LargeWindow {
    async fn complete(&self, _request: &CompletionRequest) -> Result<CompletionResponse> {
        Ok(CompletionResponse::from_text(String::new()))
    }
    fn model_name(&self) -> &str {
        "large"
    }
    fn max_context_tokens(&self) -> usize {
        131_072
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// Candidates used to stop at the newest 64 messages, so a long chat of
/// short turns asked for compaction even when the window held all of it.
/// How far back they reach is now the recent-history pool's call.
#[tokio::test]
async fn a_long_chat_of_short_turns_rides_verbatim_in_a_large_window() {
    let pool = database().await;
    let id = seed(&pool, 200).await;
    let repository = ConversationRepository::new(pool.clone());
    let model: Arc<dyn LLMPort> = Arc::new(LargeWindow);

    let built = build_memory_plan(
        &repository,
        &repository,
        &model,
        &id,
        "You are helpful.",
        "What is the status?",
        0,
        Vec::new(),
        true,
    )
    .await
    .unwrap()
    .expect("a plan");

    assert!(!built.plan.compaction_required);
    assert!(built.plan.messages.iter().any(|message| matches!(
        message,
        lattice_core::application::ports::llm_port::CompletionInput::Message { content, .. }
            if content == "Do not deploy until I approve."
    )));
}
