//! Repository behaviour for the bounded memory ledger.
//!
//! Design: `docs/design/2026-09-19-conversation-memory.md` §16.4.
//!
//! These run against real temporary SQLite databases built from the production
//! migration, because most of what is being tested here *is* the schema:
//! triggers, foreign keys, unique constraints, and transaction boundaries. A
//! mock repository would assert that the code calls itself.

use super::*;
use crate::application::ports::conversation_memory::{
    MemoryCommitCandidate, MemoryCommitPreconditions, SourceReadLimits, SourceSpanRef,
    SummaryUpdate,
};
use crate::domain::conversation_memory::{
    compute_digest, EvidencePurpose, EvidenceSpan, MemoryCommit, MemoryId, MemoryItem, MemoryKind,
    MemoryReview, MemoryState, MemoryValidity, SourceRole,
};
use crate::features::conversation::repository::ConversationRepository;

/// A conversation with `user`/`assistant` turns whose text is predictable.
async fn seed_memory_thread(pool: &SqlitePool) -> String {
    sqlx::query(
        "INSERT INTO conversations (id, title, model_name, space_id, created_at, updated_at) \
         VALUES ('conv-mem', 'Memory', 'model', 'space_general', \
                 '2026-09-01T10:00:00Z', '2026-09-01T10:00:00Z')",
    )
    .execute(pool)
    .await
    .unwrap();

    // Deliberately identical timestamps: `created_at` is not an order, and this
    // is the fixture that proves sequence is what carries it.
    for (id, role, content) in [
        ("mm1", "user", "Do not deploy until I approve."),
        ("mm2", "assistant", "Understood, I will wait."),
        ("mm3", "user", "Budget is $120 for the importer."),
        ("mm4", "assistant", "Noted."),
    ] {
        seed_message(
            pool,
            "conv-mem",
            id,
            role,
            content,
            10,
            "2026-09-01T10:00:00Z",
        )
        .await;
    }
    "conv-mem".to_string()
}

/// An evidence span over the whole of one seeded message.
async fn whole_message_span(
    pool: &SqlitePool,
    message_id: &str,
    purpose: EvidencePurpose,
) -> EvidenceSpan {
    #[derive(sqlx::FromRow)]
    struct Row {
        sequence: i64,
        role: String,
        content: String,
    }
    let row = sqlx::query_as::<_, Row>(
        "SELECT sequence, role, content FROM conversation_messages WHERE id = ?",
    )
    .bind(message_id)
    .fetch_one(pool)
    .await
    .unwrap();
    EvidenceSpan {
        message_id: message_id.to_string(),
        sequence: row.sequence,
        role: row.role.parse().unwrap(),
        start_byte: 0,
        end_byte: row.content.len() as u32,
        content_digest: compute_digest(&row.content),
        purpose,
    }
}

fn item(id: &MemoryId, conversation_id: &str, kind: MemoryKind, span: EvidenceSpan) -> MemoryItem {
    MemoryItem {
        id: id.clone(),
        conversation_id: conversation_id.to_string(),
        kind,
        state: MemoryState::Active,
        label: "Approval required before deploy".into(),
        created_at_sequence: span.sequence,
        changed_at_sequence: span.sequence,
        evidence: vec![span],
        superseded_by: None,
        revision: 0,
        review: MemoryReview::Supported,
        related_item_ids: Vec::new(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

fn candidate(
    commit: MemoryCommit,
    transcript_revision: i64,
    summary: Option<SummaryUpdate>,
) -> MemoryCommitCandidate {
    MemoryCommitCandidate {
        commit,
        summary,
        source_message_ids: vec!["mm1".into(), "mm2".into()],
        transcript_revision,
        extractor_model_identity: Some("test-utility".into()),
        extractor_prompt_version: Some("extract-v1".into()),
        validator_version: Some("validate-v1".into()),
        operation: "compact".into(),
    }
}

#[tokio::test]
async fn a_commit_makes_ledger_summary_watermark_and_revision_durable_together() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_memory_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());

    let before = repo.load_memory_snapshot(&conversation_id).await.unwrap();
    assert_eq!(before.state.memory_revision, 0);
    assert_eq!(before.state.processed_through_sequence, 0);
    assert!(before.active_items.is_empty());
    assert!(before.summary.is_none());

    let id = MemoryId::new();
    let span = whole_message_span(&pool, "mm1", EvidencePurpose::Assertion).await;
    let commit = MemoryCommit {
        inserts: vec![item(&id, &conversation_id, MemoryKind::Constraint, span)],
        updates: Vec::new(),
        summary: Some("Working on the importer.".into()),
        processed_through_sequence: 2,
    };
    let committed = repo
        .commit_memory(
            &MemoryCommitPreconditions {
                conversation_id: conversation_id.clone(),
                expected_transcript_revision: before.transcript_revision,
                expected_memory_revision: 0,
                operation_id: "op-1".into(),
            },
            &candidate(
                commit,
                before.transcript_revision,
                Some(SummaryUpdate {
                    summary_text: "Working on the importer.".into(),
                    up_to_message_id: "mm2".into(),
                    original_message_count: 2,
                    original_tokens: 20,
                    summary_tokens: 5,
                }),
            ),
        )
        .await
        .expect("commit succeeds");

    assert_eq!(committed.memory_revision, 1);
    assert_eq!(committed.active_mandatory_count, 1);
    assert_eq!(committed.active_optional_count, 0);
    assert!(!committed.was_already_committed);

    // Reloaded from disk, every part of the commit arrived and they agree about
    // which revision they belong to.
    let after = repo.load_memory_snapshot(&conversation_id).await.unwrap();
    assert_eq!(after.state.memory_revision, 1);
    assert_eq!(after.state.processed_through_sequence, 2);
    assert_eq!(after.state.validity, MemoryValidity::Ready);
    assert_eq!(
        after.state.extractor_prompt_version.as_deref(),
        Some("extract-v1")
    );
    assert_eq!(after.summary.as_deref(), Some("Working on the importer."));
    assert_eq!(after.active_items.len(), 1);
    let stored = &after.active_items[0];
    assert_eq!(stored.kind, MemoryKind::Constraint);
    assert_eq!(stored.evidence.len(), 1);

    let summary_revision: i64 = sqlx::query_scalar(
        "SELECT memory_revision FROM conversation_summaries WHERE conversation_id = ?",
    )
    .bind(&conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        summary_revision, 1,
        "summary must name the revision it belongs to"
    );
}

#[tokio::test]
async fn recompacting_updates_the_one_summary_row_in_place() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_memory_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());

    // The memory commit is the only writer of `conversation_summaries`, so this
    // is where the one-row-per-conversation invariant has to hold. It used to be
    // covered against a separate `upsert_summary` helper, which was a second way
    // to write the same row and could disagree with the ledger's revision.
    for (round, text) in [(0, "first summary"), (1, "second summary")] {
        let snapshot = repo.load_memory_snapshot(&conversation_id).await.unwrap();
        repo.commit_memory(
            &MemoryCommitPreconditions {
                conversation_id: conversation_id.clone(),
                expected_transcript_revision: snapshot.transcript_revision,
                // The round *is* the expected revision: each commit bumps it by one.
                expected_memory_revision: round,
                operation_id: format!("summary-{round}"),
            },
            &candidate(
                MemoryCommit {
                    processed_through_sequence: 4,
                    ..Default::default()
                },
                snapshot.transcript_revision,
                Some(SummaryUpdate {
                    summary_text: text.to_string(),
                    up_to_message_id: "mm2".into(),
                    original_message_count: 2,
                    original_tokens: 20,
                    summary_tokens: 4 + round,
                }),
            ),
        )
        .await
        .unwrap();
    }

    let rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM conversation_summaries WHERE conversation_id = ?")
            .bind(&conversation_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(rows, 1, "a conversation must keep at most one summary");

    let snapshot = repo.load_memory_snapshot(&conversation_id).await.unwrap();
    assert_eq!(snapshot.summary.as_deref(), Some("second summary"));
    // And the row names the revision it was written with, so a reader can never
    // pair this summary with a different ledger.
    let paired: i64 = sqlx::query_scalar(
        "SELECT memory_revision FROM conversation_summaries WHERE conversation_id = ?",
    )
    .bind(&conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(paired, snapshot.state.memory_revision);
}

#[tokio::test]
async fn evidence_resolves_to_exact_source_text_and_stops_when_the_source_moves() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_memory_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());

    let spans = [SourceSpanRef {
        message_id: "mm1".into(),
        start_byte: 0,
        end_byte: "Do not deploy".len() as u32,
    }];
    let resolved = repo
        .read_memory_source_spans(&conversation_id, &spans, SourceReadLimits::DEFAULT)
        .await
        .unwrap();
    assert_eq!(resolved[0].text.as_deref(), Some("Do not deploy"));
    assert_eq!(resolved[0].role, SourceRole::User);

    // Editing the message changes its digest, so the same offsets no longer
    // resolve. There is no cached copy of the old text to fall back to.
    sqlx::query("UPDATE conversation_messages SET content = ? WHERE id = 'mm1'")
        .bind("You may deploy whenever you like.")
        .execute(&pool)
        .await
        .unwrap();
    let resolved = repo
        .read_memory_source_spans(&conversation_id, &spans, SourceReadLimits::DEFAULT)
        .await
        .unwrap();
    assert_eq!(
        resolved[0].text, None,
        "a moved source must not hand back bytes the user never wrote"
    );
}

#[tokio::test]
async fn a_foreign_span_cannot_be_read_or_committed_through_another_conversation() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_memory_thread(&pool).await;
    // A second thread with a byte-identical message.
    sqlx::query(
        "INSERT INTO conversations (id, title, model_name, space_id, created_at, updated_at) \
         VALUES ('conv-other', 'Other', 'model', 'space_general', \
                 '2026-09-01T10:00:00Z', '2026-09-01T10:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    seed_message(
        &pool,
        "conv-other",
        "other1",
        "user",
        "Do not deploy until I approve.",
        10,
        "2026-09-01T10:00:00Z",
    )
    .await;
    let repo = ConversationRepository::new(pool.clone());

    // Reading it through the wrong conversation yields no text at all.
    let resolved = repo
        .read_memory_source_spans(
            &conversation_id,
            &[SourceSpanRef {
                message_id: "other1".into(),
                start_byte: 0,
                end_byte: 13,
            }],
            SourceReadLimits::DEFAULT,
        )
        .await
        .unwrap();
    assert_eq!(resolved[0].text, None);

    // And committing an item whose evidence names it is rejected, even though
    // the quotation is genuinely present *somewhere* in the database.
    let foreign = whole_message_span(&pool, "other1", EvidencePurpose::Assertion).await;
    let id = MemoryId::new();
    let snapshot = repo.load_memory_snapshot(&conversation_id).await.unwrap();
    let error = repo
        .commit_memory(
            &MemoryCommitPreconditions {
                conversation_id: conversation_id.clone(),
                expected_transcript_revision: snapshot.transcript_revision,
                expected_memory_revision: 0,
                operation_id: "op-foreign".into(),
            },
            &candidate(
                MemoryCommit {
                    inserts: vec![item(&id, &conversation_id, MemoryKind::Constraint, foreign)],
                    processed_through_sequence: 4,
                    ..Default::default()
                },
                snapshot.transcript_revision,
                None,
            ),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), "unresolvable_evidence");
    assert!(repo
        .load_memory_snapshot(&conversation_id)
        .await
        .unwrap()
        .active_items
        .is_empty());
}

#[tokio::test]
async fn a_concurrent_append_makes_the_commit_conflict_rather_than_publish_stale_state() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_memory_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());

    let snapshot = repo.load_memory_snapshot(&conversation_id).await.unwrap();

    // The user says something else while extraction is running.
    seed_message(
        &pool,
        &conversation_id,
        "mm5",
        "user",
        "Actually, deploy staging is fine.",
        10,
        "2026-09-01T10:00:00Z",
    )
    .await;

    let id = MemoryId::new();
    let span = whole_message_span(&pool, "mm1", EvidencePurpose::Assertion).await;
    let error = repo
        .commit_memory(
            &MemoryCommitPreconditions {
                conversation_id: conversation_id.clone(),
                expected_transcript_revision: snapshot.transcript_revision,
                expected_memory_revision: 0,
                operation_id: "op-stale".into(),
            },
            &candidate(
                MemoryCommit {
                    inserts: vec![item(&id, &conversation_id, MemoryKind::Constraint, span)],
                    processed_through_sequence: 4,
                    ..Default::default()
                },
                snapshot.transcript_revision,
                None,
            ),
        )
        .await
        .unwrap_err();

    assert_eq!(error.code(), "transcript_conflict");
    assert!(
        error.is_retryable(),
        "the caller should retry from a fresh snapshot"
    );
    // Nothing was written, so the retry starts from a clean slate.
    let after = repo.load_memory_snapshot(&conversation_id).await.unwrap();
    assert_eq!(after.state.memory_revision, 0);
    assert!(after.active_items.is_empty());
}

#[tokio::test]
async fn two_compactions_race_and_only_one_publishes() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_memory_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());
    let snapshot = repo.load_memory_snapshot(&conversation_id).await.unwrap();

    let span = whole_message_span(&pool, "mm1", EvidencePurpose::Assertion).await;
    let build = |label: &str| {
        let mut memory_item = item(
            &MemoryId::new(),
            &conversation_id,
            MemoryKind::Constraint,
            span.clone(),
        );
        memory_item.label = label.to_string();
        memory_item
    };

    let first = build("winner");
    let second = build("loser");

    let preconditions = |operation: &str| MemoryCommitPreconditions {
        conversation_id: conversation_id.clone(),
        expected_transcript_revision: snapshot.transcript_revision,
        expected_memory_revision: 0,
        operation_id: operation.to_string(),
    };

    repo.commit_memory(
        &preconditions("op-a"),
        &candidate(
            MemoryCommit {
                inserts: vec![first],
                processed_through_sequence: 4,
                ..Default::default()
            },
            snapshot.transcript_revision,
            None,
        ),
    )
    .await
    .expect("first commit wins");

    // The loser read the same snapshot, so its memory revision precondition is
    // now stale. It cannot overwrite the winner.
    let error = repo
        .commit_memory(
            &preconditions("op-b"),
            &candidate(
                MemoryCommit {
                    inserts: vec![second],
                    processed_through_sequence: 4,
                    ..Default::default()
                },
                snapshot.transcript_revision,
                None,
            ),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), "memory_conflict");

    let after = repo.load_memory_snapshot(&conversation_id).await.unwrap();
    assert_eq!(after.active_items.len(), 1);
    assert_eq!(after.active_items[0].label, "winner");
}

#[tokio::test]
async fn retrying_the_same_operation_id_returns_one_committed_result() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_memory_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());
    let snapshot = repo.load_memory_snapshot(&conversation_id).await.unwrap();

    let id = MemoryId::new();
    let span = whole_message_span(&pool, "mm1", EvidencePurpose::Assertion).await;
    let request = candidate(
        MemoryCommit {
            inserts: vec![item(&id, &conversation_id, MemoryKind::Constraint, span)],
            processed_through_sequence: 4,
            ..Default::default()
        },
        snapshot.transcript_revision,
        None,
    );
    let preconditions = MemoryCommitPreconditions {
        conversation_id: conversation_id.clone(),
        expected_transcript_revision: snapshot.transcript_revision,
        expected_memory_revision: 0,
        operation_id: "op-retry".into(),
    };

    let first = repo.commit_memory(&preconditions, &request).await.unwrap();
    assert!(!first.was_already_committed);

    // The caller crashed before seeing the response and retries. The second
    // call must recognize its own work, not apply it again — and must not fail
    // on the now-stale memory-revision precondition either.
    let second = repo.commit_memory(&preconditions, &request).await.unwrap();
    assert!(second.was_already_committed);
    assert_eq!(second.memory_revision, first.memory_revision);

    let after = repo.load_memory_snapshot(&conversation_id).await.unwrap();
    assert_eq!(
        after.active_items.len(),
        1,
        "the patch was applied exactly once"
    );
}

#[tokio::test]
async fn deleting_a_source_message_takes_its_quotation_and_the_summary_with_it() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_memory_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());
    let snapshot = repo.load_memory_snapshot(&conversation_id).await.unwrap();

    let id = MemoryId::new();
    let span = whole_message_span(&pool, "mm1", EvidencePurpose::Assertion).await;
    repo.commit_memory(
        &MemoryCommitPreconditions {
            conversation_id: conversation_id.clone(),
            expected_transcript_revision: snapshot.transcript_revision,
            expected_memory_revision: 0,
            operation_id: "op-del".into(),
        },
        &candidate(
            MemoryCommit {
                inserts: vec![item(&id, &conversation_id, MemoryKind::Constraint, span)],
                processed_through_sequence: 4,
                ..Default::default()
            },
            snapshot.transcript_revision,
            Some(SummaryUpdate {
                summary_text: "The user forbade deploying.".into(),
                up_to_message_id: "mm2".into(),
                original_message_count: 2,
                original_tokens: 20,
                summary_tokens: 5,
            }),
        ),
    )
    .await
    .unwrap();

    repo.delete_messages(&conversation_id, &["mm1".to_string()])
        .await
        .unwrap();

    // The quotation is gone, and so is the summary that might have repeated it.
    // Precise dependency repair would be cheaper; this is the version that is
    // easy to be sure about.
    let after = repo.load_memory_snapshot(&conversation_id).await.unwrap();
    assert!(after.active_items.is_empty());
    assert!(after.summary.is_none());
    assert_eq!(after.state.validity, MemoryValidity::RebuildRequired);
    assert_eq!(after.state.processed_through_sequence, 0);
    assert!(
        !after.is_usable(),
        "memory must not be consulted until rebuilt"
    );

    let evidence: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM conversation_memory_evidence")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(evidence, 0);
}

#[tokio::test]
async fn editing_a_source_message_invalidates_memory_without_deleting_the_transcript() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_memory_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());
    let snapshot = repo.load_memory_snapshot(&conversation_id).await.unwrap();

    let id = MemoryId::new();
    let span = whole_message_span(&pool, "mm3", EvidencePurpose::Assertion).await;
    repo.commit_memory(
        &MemoryCommitPreconditions {
            conversation_id: conversation_id.clone(),
            expected_transcript_revision: snapshot.transcript_revision,
            expected_memory_revision: 0,
            operation_id: "op-edit".into(),
        },
        &candidate(
            MemoryCommit {
                inserts: vec![item(&id, &conversation_id, MemoryKind::Decision, span)],
                processed_through_sequence: 4,
                ..Default::default()
            },
            snapshot.transcript_revision,
            None,
        ),
    )
    .await
    .unwrap();

    sqlx::query("UPDATE conversation_messages SET content = ? WHERE id = 'mm3'")
        .bind("Budget is $80 for the importer.")
        .execute(&pool)
        .await
        .unwrap();

    let after = repo.load_memory_snapshot(&conversation_id).await.unwrap();
    assert_eq!(after.state.validity, MemoryValidity::RebuildRequired);
    assert!(after.active_items.is_empty());
    // The original messages are all still there: invalidation clears *derived*
    // memory, never source material.
    let messages = repo.get_messages(&conversation_id).await.unwrap();
    assert_eq!(messages.len(), 4);
    assert_eq!(messages[2].content, "Budget is $80 for the importer.");
}

#[tokio::test]
async fn equal_timestamps_keep_a_deterministic_order_through_reload_and_vacuum() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_memory_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());

    let expected: Vec<String> = vec!["mm1".into(), "mm2".into(), "mm3".into(), "mm4".into()];
    let ids: Vec<String> = repo
        .get_messages(&conversation_id)
        .await
        .unwrap()
        .into_iter()
        .map(|message| message.id)
        .collect();
    assert_eq!(
        ids, expected,
        "every created_at is identical in this fixture"
    );

    // rowid is not durable; sequence is. VACUUM is the operation that makes the
    // difference observable.
    sqlx::query("VACUUM").execute(&pool).await.unwrap();
    let ids: Vec<String> = repo
        .get_messages(&conversation_id)
        .await
        .unwrap()
        .into_iter()
        .map(|message| message.id)
        .collect();
    assert_eq!(ids, expected);

    let page = repo
        .page_memory_source_messages(&conversation_id, 0, 99, SourceReadLimits::DEFAULT)
        .await
        .unwrap();
    assert_eq!(
        page.messages.iter().map(|m| m.sequence).collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
}

#[tokio::test]
async fn a_fork_gets_its_own_sequence_run_and_no_inherited_memory() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_memory_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());
    let snapshot = repo.load_memory_snapshot(&conversation_id).await.unwrap();

    let id = MemoryId::new();
    let span = whole_message_span(&pool, "mm1", EvidencePurpose::Assertion).await;
    repo.commit_memory(
        &MemoryCommitPreconditions {
            conversation_id: conversation_id.clone(),
            expected_transcript_revision: snapshot.transcript_revision,
            expected_memory_revision: 0,
            operation_id: "op-fork".into(),
        },
        &candidate(
            MemoryCommit {
                inserts: vec![item(&id, &conversation_id, MemoryKind::Constraint, span)],
                processed_through_sequence: 4,
                ..Default::default()
            },
            snapshot.transcript_revision,
            Some(SummaryUpdate {
                summary_text: "Parent summary.".into(),
                up_to_message_id: "mm2".into(),
                original_message_count: 2,
                original_tokens: 20,
                summary_tokens: 5,
            }),
        ),
    )
    .await
    .unwrap();

    let (fork_id, copied) = repo
        .fork(&conversation_id, Some("mm2"), "conv-fork", "Branch")
        .await
        .unwrap();
    assert_eq!(copied, 2);

    let fork_memory = repo.load_memory_snapshot(&fork_id).await.unwrap();
    assert!(
        fork_memory.active_items.is_empty(),
        "a branch must not inherit the parent's ledger"
    );
    assert!(fork_memory.summary.is_none());
    assert_eq!(fork_memory.state.memory_revision, 0);

    // The branch's messages have fresh ids and a sequence run of their own,
    // so no parent evidence span can ever name one of them.
    let fork_page = repo
        .page_memory_source_messages(&fork_id, 0, 99, SourceReadLimits::DEFAULT)
        .await
        .unwrap();
    assert_eq!(
        fork_page
            .messages
            .iter()
            .map(|m| m.sequence)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    for message in &fork_page.messages {
        assert!(!["mm1", "mm2", "mm3", "mm4"].contains(&message.id.as_str()));
        assert!(
            message.digest_matches(),
            "a copied message carries its own digest"
        );
    }

    // A later correction in the parent does not reach the branch.
    seed_message(
        &pool,
        &conversation_id,
        "mm6",
        "user",
        "Correction: deploying is fine now.",
        10,
        "2026-09-01T10:00:00Z",
    )
    .await;
    let fork_page = repo
        .page_memory_source_messages(&fork_id, 0, 99, SourceReadLimits::DEFAULT)
        .await
        .unwrap();
    assert_eq!(fork_page.messages.len(), 2);
}

#[tokio::test]
async fn an_old_summary_without_a_ledger_does_not_claim_extraction_coverage() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_memory_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());

    // Exactly the state an existing conversation is in before this feature: a
    // prose summary written by the old compaction path, and no ledger.
    sqlx::query(
        "INSERT INTO conversation_summaries \
            (id, conversation_id, summary_text, up_to_message_id, original_message_count, \
             original_tokens, summary_tokens, compression_ratio, created_at) \
         VALUES ('legacy', ?, 'The user said some things.', 'mm2', 2, 20, 5, 0.25, \
                 '2026-09-01T10:05:00Z')",
    )
    .bind(&conversation_id)
    .execute(&pool)
    .await
    .unwrap();

    let snapshot = repo.load_memory_snapshot(&conversation_id).await.unwrap();
    assert_eq!(
        snapshot.summary.as_deref(),
        Some("The user said some things.")
    );
    // The summary is carried as fallible working context; the watermark says
    // plainly that nothing has been extracted, so no constraint is claimed.
    assert_eq!(snapshot.state.processed_through_sequence, 0);
    assert!(snapshot.active_items.is_empty());
    assert!(snapshot.mandatory_items().is_empty());
}

#[tokio::test]
async fn a_supersession_keeps_the_old_item_readable_as_history() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_memory_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());
    let snapshot = repo.load_memory_snapshot(&conversation_id).await.unwrap();

    let original = MemoryId::new();
    let original_span = whole_message_span(&pool, "mm3", EvidencePurpose::Assertion).await;
    let mut first = item(
        &original,
        &conversation_id,
        MemoryKind::Decision,
        original_span.clone(),
    );
    first.label = "Budget $120".into();
    repo.commit_memory(
        &MemoryCommitPreconditions {
            conversation_id: conversation_id.clone(),
            expected_transcript_revision: snapshot.transcript_revision,
            expected_memory_revision: 0,
            operation_id: "op-first".into(),
        },
        &candidate(
            MemoryCommit {
                inserts: vec![first.clone()],
                processed_through_sequence: 4,
                ..Default::default()
            },
            snapshot.transcript_revision,
            None,
        ),
    )
    .await
    .unwrap();

    // The correction arrives as an ordinary new message.
    seed_message(
        &pool,
        &conversation_id,
        "mm7",
        "user",
        "Correction: $80 for the importer.",
        10,
        "2026-09-01T10:00:00Z",
    )
    .await;
    let snapshot = repo.load_memory_snapshot(&conversation_id).await.unwrap();
    let transition_span = whole_message_span(&pool, "mm7", EvidencePurpose::Transition).await;

    let replacement = MemoryId::new();
    let mut second = item(
        &replacement,
        &conversation_id,
        MemoryKind::Decision,
        whole_message_span(&pool, "mm7", EvidencePurpose::Assertion).await,
    );
    second.label = "Budget $80".into();

    let mut retired = snapshot.active_items[0].clone();
    retired.state = MemoryState::Superseded;
    retired.superseded_by = Some(replacement.clone());
    retired.changed_at_sequence = transition_span.sequence;
    retired.evidence.push(transition_span);

    repo.commit_memory(
        &MemoryCommitPreconditions {
            conversation_id: conversation_id.clone(),
            expected_transcript_revision: snapshot.transcript_revision,
            expected_memory_revision: 1,
            operation_id: "op-correct".into(),
        },
        &candidate(
            MemoryCommit {
                inserts: vec![second],
                updates: vec![retired],
                summary: None,
                processed_through_sequence: 5,
            },
            snapshot.transcript_revision,
            None,
        ),
    )
    .await
    .expect("supersession commits");

    // Current state carries only the new decision...
    let after = repo.load_memory_snapshot(&conversation_id).await.unwrap();
    assert_eq!(after.active_items.len(), 1);
    assert_eq!(after.active_items[0].label, "Budget $80");

    // ...and the old one is still on disk, answerable for "what was my original
    // budget?", with the passage that retired it attached.
    let history = repo
        .page_inactive_memory_items(&conversation_id, 0, 10)
        .await
        .unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].label, "Budget $120");
    assert_eq!(history[0].state, MemoryState::Superseded);
    assert_eq!(history[0].superseded_by.as_ref(), Some(&replacement));
    assert_eq!(history[0].evidence.len(), 2);
    assert!(history[0]
        .evidence
        .iter()
        .any(|span| span.purpose == EvidencePurpose::Transition));
}

#[tokio::test]
async fn paging_is_bounded_by_bytes_as_well_as_message_count() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_memory_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());

    // One pasted log file is larger than several ordinary turns put together.
    seed_message(
        &pool,
        &conversation_id,
        "mm-big",
        "user",
        &"x".repeat(50_000),
        10,
        "2026-09-01T10:00:00Z",
    )
    .await;

    let page = repo
        .page_memory_source_messages(&conversation_id, 0, 99, SourceReadLimits::new(256, 1_000))
        .await
        .unwrap();
    // The byte ceiling bound the page even though the message count did not.
    assert!(
        page.messages.len() < 5,
        "got {} messages",
        page.messages.len()
    );
    assert!(page.has_more);

    // Resumable: continuing from the cursor eventually reaches the end, and a
    // single oversized message is still delivered rather than blocking forever.
    let mut cursor = page.next_after_sequence;
    let mut seen = page.messages.len();
    for _ in 0..10 {
        let next = repo
            .page_memory_source_messages(
                &conversation_id,
                cursor,
                99,
                SourceReadLimits::new(256, 1_000),
            )
            .await
            .unwrap();
        if next.messages.is_empty() {
            break;
        }
        seen += next.messages.len();
        cursor = next.next_after_sequence;
    }
    assert_eq!(seen, 5, "every message is eventually paged");

    // The message limit binds on its own too.
    let page = repo
        .page_memory_source_messages(&conversation_id, 0, 99, SourceReadLimits::new(2, 1 << 20))
        .await
        .unwrap();
    assert_eq!(page.messages.len(), 2);
    assert!(page.has_more);
}

#[tokio::test]
async fn recall_is_scoped_to_one_conversation_and_ignores_titles_and_bookmarks() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_memory_thread(&pool).await;
    sqlx::query(
        "INSERT INTO conversations (id, title, model_name, space_id, created_at, updated_at) \
         VALUES ('conv-other', 'deploy approval notes', 'model', 'space_general', \
                 '2026-09-01T10:00:00Z', '2026-09-01T10:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    seed_message(
        &pool,
        "conv-other",
        "other1",
        "user",
        "Do not deploy until I approve.",
        10,
        "2026-09-01T10:00:00Z",
    )
    .await;
    // A bookmark note in *this* conversation, which the same FTS index holds.
    sqlx::query(
        "INSERT INTO conversation_message_bookmarks (id, conversation_id, message_id, title, note) \
         VALUES ('bk1', ?, 'mm2', 'deploy', 'a note mentioning deploy')",
    )
    .bind(&conversation_id)
    .execute(&pool)
    .await
    .unwrap();

    let repo = ConversationRepository::new(pool.clone());
    let found = repo
        .search_memory_source_messages(&conversation_id, "deploy approve", &[], 24)
        .await
        .unwrap();

    assert!(found.lexical_ran);
    assert!(found.index_error.is_none());
    assert!(!found.candidates.is_empty());
    for candidate in &found.candidates {
        assert!(
            ["mm1", "mm2", "mm3", "mm4"].contains(&candidate.message_id.as_str()),
            "recall returned {} from outside this conversation or index source",
            candidate.message_id
        );
    }
    assert!(found.candidates.iter().any(|c| c.message_id == "mm1"));
}

#[tokio::test]
async fn exact_identifiers_are_case_sensitive_and_survive_a_truncated_lexical_query() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_memory_thread(&pool).await;
    seed_message(
        &pool,
        &conversation_id,
        "mm-id",
        "user",
        "Use src/Auth/Token.ts, version 4.12.0-rc.3, not authtoken.",
        10,
        "2026-09-01T10:00:00Z",
    )
    .await;
    let repo = ConversationRepository::new(pool.clone());

    let found = repo
        .search_memory_source_messages(
            &conversation_id,
            "",
            &["src/Auth/Token.ts".into(), "4.12.0-rc.3".into()],
            24,
        )
        .await
        .unwrap();
    assert!(found.candidates.iter().any(|c| c.message_id == "mm-id"));
    assert!(found.candidates.iter().all(|c| c.exact_identifier));
    // An empty query has no lexical branch at all, and that is reported.
    assert!(!found.lexical_ran);

    // Case matters: `AuthToken` and `authtoken` are different identifiers.
    let found = repo
        .search_memory_source_messages(&conversation_id, "", &["src/auth/token.ts".into()], 24)
        .await
        .unwrap();
    assert!(found.candidates.is_empty());
}

#[tokio::test]
async fn an_unsupported_stored_schema_blocks_use_until_rebuilt() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_memory_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());
    let snapshot = repo.load_memory_snapshot(&conversation_id).await.unwrap();

    // A future build wrote this row.
    sqlx::query(
        "INSERT INTO conversation_memory_state (conversation_id, schema_version, memory_revision) \
         VALUES (?, 99, 7)",
    )
    .bind(&conversation_id)
    .execute(&pool)
    .await
    .unwrap();

    let snapshot_after = repo.load_memory_snapshot(&conversation_id).await.unwrap();
    assert_eq!(snapshot_after.state.schema_version, 99);
    assert!(
        !snapshot_after.is_usable(),
        "an unknown layout must not be read as empty memory"
    );

    // And a commit against it is refused rather than silently downgrading it.
    let id = MemoryId::new();
    let span = whole_message_span(&pool, "mm1", EvidencePurpose::Assertion).await;
    let error = repo
        .commit_memory(
            &MemoryCommitPreconditions {
                conversation_id: conversation_id.clone(),
                expected_transcript_revision: snapshot.transcript_revision,
                expected_memory_revision: 7,
                operation_id: "op-future".into(),
            },
            &candidate(
                MemoryCommit {
                    inserts: vec![item(&id, &conversation_id, MemoryKind::Constraint, span)],
                    processed_through_sequence: 4,
                    ..Default::default()
                },
                snapshot.transcript_revision,
                None,
            ),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), "unsupported_schema");

    // A rebuild clears the unusable state and returns it to this build's layout.
    repo.mark_memory_rebuild_required(&conversation_id, "schema_upgrade")
        .await
        .unwrap();
    let rebuilt = repo.load_memory_state(&conversation_id).await.unwrap();
    assert_eq!(rebuilt.schema_version, 1);
    assert_eq!(rebuilt.validity, MemoryValidity::RebuildRequired);
}

#[tokio::test]
async fn a_recorded_error_leaves_the_last_good_ledger_exactly_as_it_was() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_memory_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());
    let snapshot = repo.load_memory_snapshot(&conversation_id).await.unwrap();

    let id = MemoryId::new();
    let span = whole_message_span(&pool, "mm1", EvidencePurpose::Assertion).await;
    repo.commit_memory(
        &MemoryCommitPreconditions {
            conversation_id: conversation_id.clone(),
            expected_transcript_revision: snapshot.transcript_revision,
            expected_memory_revision: 0,
            operation_id: "op-good".into(),
        },
        &candidate(
            MemoryCommit {
                inserts: vec![item(&id, &conversation_id, MemoryKind::Constraint, span)],
                processed_through_sequence: 4,
                ..Default::default()
            },
            snapshot.transcript_revision,
            Some(SummaryUpdate {
                summary_text: "Good summary.".into(),
                up_to_message_id: "mm2".into(),
                original_message_count: 2,
                original_tokens: 20,
                summary_tokens: 5,
            }),
        ),
    )
    .await
    .unwrap();

    // A later attempt fails for any reason at all.
    repo.record_memory_error(&conversation_id, "malformed_json")
        .await
        .unwrap();

    let after = repo.load_memory_snapshot(&conversation_id).await.unwrap();
    assert_eq!(
        after.state.last_error_code.as_deref(),
        Some("malformed_json")
    );
    // Everything that was committed is still usable: a maintenance failure is
    // reported, not applied.
    assert_eq!(after.state.validity, MemoryValidity::Ready);
    assert_eq!(after.state.memory_revision, 1);
    assert_eq!(after.state.processed_through_sequence, 4);
    assert_eq!(after.active_items.len(), 1);
    assert_eq!(after.summary.as_deref(), Some("Good summary."));
}

#[tokio::test]
async fn event_retention_is_bounded_and_never_removes_item_evidence() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_memory_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());

    let id = MemoryId::new();
    let span = whole_message_span(&pool, "mm1", EvidencePurpose::Assertion).await;
    for round in 0..105 {
        let snapshot = repo.load_memory_snapshot(&conversation_id).await.unwrap();
        let commit = if round == 0 {
            MemoryCommit {
                inserts: vec![item(
                    &id,
                    &conversation_id,
                    MemoryKind::Constraint,
                    span.clone(),
                )],
                processed_through_sequence: 4,
                ..Default::default()
            }
        } else {
            // Later rounds change nothing but still write an event.
            MemoryCommit {
                processed_through_sequence: 4,
                ..Default::default()
            }
        };
        repo.commit_memory(
            &MemoryCommitPreconditions {
                conversation_id: conversation_id.clone(),
                expected_transcript_revision: snapshot.transcript_revision,
                // The round *is* the expected revision: every commit bumps it once.
                expected_memory_revision: round,
                operation_id: format!("op-{round}"),
            },
            &candidate(commit, snapshot.transcript_revision, None),
        )
        .await
        .unwrap();
    }

    let events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversation_memory_events WHERE conversation_id = ?",
    )
    .bind(&conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(events, 100, "the log is bounded at 100 per conversation");

    // Pruning the log did not touch the ledger it describes.
    let after = repo.load_memory_snapshot(&conversation_id).await.unwrap();
    assert_eq!(after.active_items.len(), 1);
    assert_eq!(after.active_items[0].evidence.len(), 1);
}
