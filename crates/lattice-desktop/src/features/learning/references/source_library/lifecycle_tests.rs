//! Freshness policy, refresh guards, adoption, deletion and capture checks
//! for the program source library.
use super::*;
use crate::features::learning::{repository::LearningRepository, tests};

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[track_caller]
fn assert_invalid<T: std::fmt::Debug>(result: Result<T>) {
    assert!(
        matches!(result, Err(AppError::InvalidInput(_))),
        "expected InvalidInput, got {result:?}"
    );
}

#[track_caller]
fn assert_not_found<T: std::fmt::Debug>(result: Result<T>) {
    assert!(
        matches!(result, Err(AppError::NotFound(_))),
        "expected NotFound, got {result:?}"
    );
}

async fn active_program(pool: &SqlitePool) -> Result<String> {
    let repo = LearningRepository::new(pool.clone());
    let program = tests::fixture();
    repo.create(&program).await?;
    repo.accept(&AcceptLearningProgramRequestDto {
        program_id: program.summary.id.clone(),
        expected_revision: 0,
        title: program.summary.title.clone(),
    })
    .await?;
    Ok(program.summary.id)
}

fn page(title: &str, text: &str) -> CapturedLearningSource {
    CapturedLearningSource {
        title: title.into(),
        publisher: Some("Field Guide Press".into()),
        requested_url: Some("https://example.org/altitude".into()),
        resolved_url: Some("https://example.org/altitude".into()),
        text: text.into(),
        truncated: false,
        extraction_version: "test_v1".into(),
        signals: crate::features::learning::references::trust::capture::not_extracted(
            "test fixture",
        ),
    }
}

fn new_source<'a>(
    operation_id: &'a str,
    source_id: &'a str,
    version_id: &'a str,
    program_id: &'a str,
    kind: LearningSourceKind,
    freshness: LearningSourcePolicy,
) -> NewLearningSource<'a> {
    let web = kind == LearningSourceKind::Web;
    NewLearningSource {
        operation_id,
        source_id,
        version_id,
        program_id,
        kind,
        operation_kind: if web { "add_web" } else { "add_text" },
        origin: if web {
            "https://example.org/altitude"
        } else {
            "Field notes"
        },
        requested_url: if web {
            Some("https://example.org/altitude")
        } else {
            None
        },
        freshness,
    }
}

/// Adds a source and returns `(source_id, first_version_id)`.
async fn add(
    library: &LearningSourceLibraryRepository,
    program_id: &str,
    kind: LearningSourceKind,
    freshness: LearningSourcePolicy,
) -> Result<(String, String)> {
    let (op, source_id, version_id) = (id(), id(), id());
    library
        .add(
            new_source(&op, &source_id, &version_id, program_id, kind, freshness),
            page("Boiling at altitude", "Water boils near 70 °C on Everest."),
            id(),
        )
        .await?;
    Ok((source_id, version_id))
}

async fn try_add(
    library: &LearningSourceLibraryRepository,
    program_id: &str,
    captured: CapturedLearningSource,
    kind: LearningSourceKind,
    freshness: LearningSourcePolicy,
) -> Result<()> {
    let (op, source_id, version_id) = (id(), id(), id());
    library
        .add(
            new_source(&op, &source_id, &version_id, program_id, kind, freshness),
            captured,
            id(),
        )
        .await
}

async fn item(
    library: &LearningSourceLibraryRepository,
    program_id: &str,
    source_id: &str,
) -> Result<LearningSourceLibraryItemDto> {
    library
        .workspace_including_deleted(program_id)
        .await?
        .sources
        .into_iter()
        .find(|s| s.id == source_id)
        .ok_or_else(|| AppError::NotFound("source missing from workspace".into()))
}

fn policy_request(
    program_id: &str,
    source_id: &str,
    freshness_policy: LearningSourcePolicy,
    expected_revision: i64,
) -> UpdateLearningSourcePolicyRequestDto {
    UpdateLearningSourcePolicyRequestDto {
        operation_id: id(),
        program_id: program_id.into(),
        source_id: source_id.into(),
        freshness_policy,
        expected_revision,
    }
}

fn refresh_request(
    program_id: &str,
    source_id: &str,
    expected_revision: i64,
) -> RefreshLearningSourceRequestDto {
    RefreshLearningSourceRequestDto {
        operation_id: id(),
        program_id: program_id.into(),
        source_id: source_id.into(),
        expected_revision,
    }
}

async fn check_row(
    pool: &SqlitePool,
    operation_id: &str,
) -> Result<(String, Option<String>, Option<String>)> {
    let row = sqlx::query(
        "SELECT status,message,pending_version_id FROM learning_source_refresh_checks WHERE operation_id=?",
    )
    .bind(operation_id)
    .fetch_one(pool)
    .await
    .map_err(db)?;
    Ok((
        row.get("status"),
        row.get("message"),
        row.get("pending_version_id"),
    ))
}

#[tokio::test]
async fn policy_changes_are_receipted_revision_guarded_and_kind_checked() -> Result<()> {
    let pool = tests::pool().await?;
    let program_id = active_program(&pool).await?;
    let library = LearningSourceLibraryRepository::new(pool.clone());
    let (web, _) = add(
        &library,
        &program_id,
        LearningSourceKind::Web,
        LearningSourcePolicy::Manual,
    )
    .await?;
    let (pasted, _) = add(
        &library,
        &program_id,
        LearningSourceKind::Pasted,
        LearningSourcePolicy::Fixed,
    )
    .await?;

    let change = policy_request(&program_id, &web, LearningSourcePolicy::BeforeUse, 0);
    library.update_policy(&change).await?;
    library.update_policy(&change).await?;
    let changed = item(&library, &program_id, &web).await?;
    assert_eq!(changed.freshness_policy, LearningSourcePolicy::BeforeUse);
    assert_eq!(
        changed.revision, 1,
        "a replay must not bump the revision again"
    );

    let mut reused = change.clone();
    reused.freshness_policy = LearningSourcePolicy::Fixed;
    assert_invalid(library.update_policy(&reused).await);

    // Asking for the policy the source already has is accepted without a new revision.
    library
        .update_policy(&policy_request(
            &program_id,
            &web,
            LearningSourcePolicy::BeforeUse,
            1,
        ))
        .await?;
    assert_eq!(item(&library, &program_id, &web).await?.revision, 1);

    assert_invalid(
        library
            .update_policy(&policy_request(
                &program_id,
                &web,
                LearningSourcePolicy::Fixed,
                0,
            ))
            .await,
    );
    assert_eq!(
        item(&library, &program_id, &web).await?.freshness_policy,
        LearningSourcePolicy::BeforeUse
    );

    assert_invalid(
        library
            .update_policy(&policy_request(
                &program_id,
                &pasted,
                LearningSourcePolicy::Manual,
                0,
            ))
            .await,
    );
    library
        .update_policy(&policy_request(
            &program_id,
            &pasted,
            LearningSourcePolicy::Fixed,
            0,
        ))
        .await?;
    assert_eq!(item(&library, &program_id, &pasted).await?.revision, 0);

    assert_not_found(
        library
            .update_policy(&policy_request(
                &program_id,
                "no-such-source",
                LearningSourcePolicy::Manual,
                0,
            ))
            .await,
    );
    let mut bad_operation = policy_request(&program_id, &web, LearningSourcePolicy::Manual, 1);
    bad_operation.operation_id = "not-a-uuid".into();
    assert_invalid(library.update_policy(&bad_operation).await);
    assert_invalid(
        library
            .update_policy(&policy_request(
                &program_id,
                "  ",
                LearningSourcePolicy::Manual,
                1,
            ))
            .await,
    );

    let draft = tests::fixture();
    LearningRepository::new(pool.clone()).create(&draft).await?;
    assert_not_found(
        library
            .update_policy(&policy_request(
                &draft.summary.id,
                &web,
                LearningSourcePolicy::Manual,
                1,
            ))
            .await,
    );
    Ok(())
}

#[tokio::test]
async fn only_live_web_sources_with_a_refreshable_policy_are_offered_for_refresh() -> Result<()> {
    let pool = tests::pool().await?;
    let program_id = active_program(&pool).await?;
    let library = LearningSourceLibraryRepository::new(pool.clone());
    let (manual, _) = add(
        &library,
        &program_id,
        LearningSourceKind::Web,
        LearningSourcePolicy::Manual,
    )
    .await?;
    let (fixed, _) = add(
        &library,
        &program_id,
        LearningSourceKind::Web,
        LearningSourcePolicy::Fixed,
    )
    .await?;
    let (pasted, _) = add(
        &library,
        &program_id,
        LearningSourceKind::Pasted,
        LearningSourcePolicy::Fixed,
    )
    .await?;

    let (revision, url, policy) = library.source_for_refresh(&program_id, &manual).await?;
    assert_eq!(revision, 0);
    assert_eq!(url, "https://example.org/altitude");
    assert_eq!(policy, LearningSourcePolicy::Manual);

    assert_invalid(library.source_for_refresh(&program_id, &fixed).await);
    assert_invalid(library.source_for_refresh(&program_id, &pasted).await);
    assert_not_found(
        library
            .source_for_refresh(&program_id, "no-such-source")
            .await,
    );
    assert_invalid(library.source_for_refresh("not-a-uuid", &manual).await);
    assert_invalid(library.source_for_refresh(&program_id, "").await);

    library
        .delete_source(&DeleteLearningSourceRequestDto {
            operation_id: id(),
            program_id: program_id.clone(),
            source_id: manual.clone(),
            expected_revision: 0,
            reason: "Superseded by a field survey".into(),
        })
        .await?;
    assert_not_found(library.source_for_refresh(&program_id, &manual).await);
    Ok(())
}

#[tokio::test]
async fn refresh_guards_its_inputs_and_records_unusable_pages_as_failed_checks() -> Result<()> {
    let pool = tests::pool().await?;
    let program_id = active_program(&pool).await?;
    let library = LearningSourceLibraryRepository::new(pool.clone());
    let (web, _) = add(
        &library,
        &program_id,
        LearningSourceKind::Web,
        LearningSourcePolicy::Manual,
    )
    .await?;
    let (fixed, _) = add(
        &library,
        &program_id,
        LearningSourceKind::Web,
        LearningSourcePolicy::Fixed,
    )
    .await?;
    let (pasted, _) = add(
        &library,
        &program_id,
        LearningSourceKind::Pasted,
        LearningSourcePolicy::Fixed,
    )
    .await?;
    let fresh = || {
        Some(page(
            "Boiling at altitude",
            "Updated: water boils near 71 °C.",
        ))
    };

    assert_invalid(
        library
            .refresh(&refresh_request(&program_id, &fixed, 0), fresh(), None)
            .await,
    );
    assert_invalid(
        library
            .refresh(&refresh_request(&program_id, &pasted, 0), fresh(), None)
            .await,
    );
    assert_invalid(
        library
            .refresh(&refresh_request(&program_id, &web, 3), fresh(), None)
            .await,
    );
    assert_invalid(
        library
            .refresh(&refresh_request(&program_id, &web, 0), None, None)
            .await,
    );
    assert_not_found(
        library
            .refresh(
                &refresh_request(&program_id, "no-such-source", 0),
                fresh(),
                None,
            )
            .await,
    );

    let untitled = refresh_request(&program_id, &web, 0);
    library
        .refresh(&untitled, Some(page("   ", "Text without a title")), None)
        .await?;
    let (status, message, pending) = check_row(&pool, &untitled.operation_id).await?;
    assert_eq!(status, "failed");
    assert!(message.is_some_and(|m| m.contains("valid source metadata")));
    assert!(pending.is_none());

    let long_title = refresh_request(&program_id, &web, 0);
    library
        .refresh(&long_title, Some(page(&"t".repeat(181), "Some text")), None)
        .await?;
    assert_eq!(
        check_row(&pool, &long_title.operation_id).await?.0,
        "failed"
    );

    let unreachable = refresh_request(&program_id, &web, 0);
    library
        .refresh(&unreachable, None, Some("é".repeat(600)))
        .await?;
    let (status, message, _) = check_row(&pool, &unreachable.operation_id).await?;
    assert_eq!(status, "failed");
    assert_eq!(message.map(|m| m.chars().count()), Some(500));

    let source = item(&library, &program_id, &web).await?;
    assert_eq!(source.revision, 0, "failed checks never bump the revision");
    assert_eq!(source.versions.len(), 1);
    assert!(source.pending_version_id.is_none());
    Ok(())
}

#[tokio::test]
async fn refreshing_to_known_text_reuses_its_version_and_adoption_needs_the_pending_one(
) -> Result<()> {
    let pool = tests::pool().await?;
    let program_id = active_program(&pool).await?;
    let library = LearningSourceLibraryRepository::new(pool.clone());
    let (web, first_version) = add(
        &library,
        &program_id,
        LearningSourceKind::Web,
        LearningSourcePolicy::BeforeUse,
    )
    .await?;
    let update = || {
        Some(page(
            "Boiling at altitude",
            "Revised: pressure cookers help.",
        ))
    };

    let first = refresh_request(&program_id, &web, 0);
    library.refresh(&first, update(), None).await?;
    let (status, _, pending) = check_row(&pool, &first.operation_id).await?;
    assert_eq!(status, "update_available");
    let pending = pending.expect("pending version");
    assert_eq!(item(&library, &program_id, &web).await?.revision, 1);

    let again = refresh_request(&program_id, &web, 1);
    library.refresh(&again, update(), None).await?;
    let (status, _, again_pending) = check_row(&pool, &again.operation_id).await?;
    assert_eq!(status, "update_available");
    assert_eq!(again_pending.as_deref(), Some(pending.as_str()));
    let source = item(&library, &program_id, &web).await?;
    assert_eq!(
        source.versions.len(),
        2,
        "the same text must not create a third version"
    );
    assert_eq!(
        source.revision, 1,
        "an unchanged pending version keeps the revision"
    );

    let adopt = |version_id: &str, expected_revision: i64| AdoptLearningSourceVersionRequestDto {
        operation_id: id(),
        program_id: program_id.clone(),
        source_id: web.clone(),
        version_id: version_id.into(),
        expected_revision,
    };
    assert_invalid(library.adopt(&adopt(&first_version, 1)).await);
    assert_invalid(library.adopt(&adopt(&pending, 0)).await);
    let mut missing = adopt(&pending, 1);
    missing.source_id = "no-such-source".into();
    assert_not_found(library.adopt(&missing).await);

    let accepted = adopt(&pending, 1);
    library.adopt(&accepted).await?;
    library.adopt(&accepted).await?;
    let source = item(&library, &program_id, &web).await?;
    assert_eq!(source.active_version_id.as_deref(), Some(pending.as_str()));
    assert!(source.pending_version_id.is_none());
    assert_eq!(source.revision, 2);

    // The adopted text is now active, so the same page reads as unchanged.
    let unchanged = refresh_request(&program_id, &web, 2);
    library.refresh(&unchanged, update(), None).await?;
    let (status, _, none_pending) = check_row(&pool, &unchanged.operation_id).await?;
    assert_eq!(status, "unchanged");
    assert!(none_pending.is_none());
    assert_eq!(item(&library, &program_id, &web).await?.revision, 2);
    Ok(())
}

#[tokio::test]
async fn deleting_a_source_needs_a_reason_and_happens_once() -> Result<()> {
    let pool = tests::pool().await?;
    let program_id = active_program(&pool).await?;
    let library = LearningSourceLibraryRepository::new(pool.clone());
    let (source_id, _) = add(
        &library,
        &program_id,
        LearningSourceKind::Pasted,
        LearningSourcePolicy::Fixed,
    )
    .await?;
    let request = |reason: &str, expected_revision: i64| DeleteLearningSourceRequestDto {
        operation_id: id(),
        program_id: program_id.clone(),
        source_id: source_id.clone(),
        expected_revision,
        reason: reason.into(),
    };

    assert_invalid(library.delete_source(&request("   ", 0)).await);
    assert_invalid(library.delete_source(&request(&"r".repeat(501), 0)).await);
    assert_invalid(library.delete_source(&request("Outdated survey", 4)).await);
    let mut missing = request("Outdated survey", 0);
    missing.source_id = "no-such-source".into();
    assert_not_found(library.delete_source(&missing).await);

    let delete = request("  Outdated survey  ", 0);
    library.delete_source(&delete).await?;
    library.delete_source(&delete).await?;
    let deleted = item(&library, &program_id, &source_id).await?;
    assert!(deleted.deleted_at.is_some());
    assert_eq!(deleted.deletion_reason.as_deref(), Some("Outdated survey"));
    assert_eq!(deleted.revision, 1);

    assert_invalid(library.delete_source(&request("Second removal", 1)).await);
    let mut reused = delete.clone();
    reused.reason = "A different reason".into();
    assert_invalid(library.delete_source(&reused).await);
    assert!(!library
        .workspace(&program_id)
        .await?
        .sources
        .iter()
        .any(|s| s.id == source_id));
    Ok(())
}

#[tokio::test]
async fn capture_validates_metadata_and_keeps_only_a_meaningful_selection_note() -> Result<()> {
    let pool = tests::pool().await?;
    let program_id = active_program(&pool).await?;
    let library = LearningSourceLibraryRepository::new(pool.clone());
    // The fixture program brings its own selected source.
    let fixture_rows = library_rows(&pool, &program_id).await?;

    let mut noted = Vec::new();
    for note in [
        Some("  Explains why pasta takes longer in the mountains  "),
        Some("   "),
        None,
    ] {
        let (op, source_id, version_id) = (id(), id(), id());
        library
            .add_selected(
                new_source(
                    &op,
                    &source_id,
                    &version_id,
                    &program_id,
                    LearningSourceKind::Web,
                    LearningSourcePolicy::Manual,
                ),
                page(
                    "Cooking at altitude",
                    "Lower pressure lowers the boiling point.",
                ),
                id(),
                note,
            )
            .await?;
        let stored: Option<String> = sqlx::query_scalar(
            "SELECT selection_note FROM learning_source_library WHERE program_id=? AND id=?",
        )
        .bind(&program_id)
        .bind(&source_id)
        .fetch_one(&pool)
        .await
        .map_err(db)?;
        noted.push(stored);
    }
    assert_eq!(
        noted,
        vec![
            Some("Explains why pasta takes longer in the mountains".to_owned()),
            None,
            None
        ]
    );

    let web = LearningSourceKind::Web;
    let manual = LearningSourcePolicy::Manual;
    assert_invalid(
        try_add(
            &library,
            &program_id,
            page("Altitude", " \r\n "),
            web.clone(),
            manual.clone(),
        )
        .await,
    );
    assert_invalid(
        try_add(
            &library,
            &program_id,
            page("", "Some text"),
            web.clone(),
            manual.clone(),
        )
        .await,
    );
    assert_invalid(
        try_add(
            &library,
            &program_id,
            page(&"t".repeat(181), "Some text"),
            web.clone(),
            manual.clone(),
        )
        .await,
    );
    let mut long_publisher = page("Altitude", "Some text");
    long_publisher.publisher = Some("p".repeat(181));
    assert_invalid(
        try_add(
            &library,
            &program_id,
            long_publisher,
            web.clone(),
            manual.clone(),
        )
        .await,
    );
    let mut long_url = page("Altitude", "Some text");
    long_url.resolved_url = Some(format!("https://example.org/{}", "a".repeat(2048)));
    assert_invalid(try_add(&library, &program_id, long_url, web.clone(), manual.clone()).await);
    assert_invalid(
        try_add(
            &library,
            &program_id,
            page("Altitude", "Some text"),
            LearningSourceKind::Pasted,
            manual.clone(),
        )
        .await,
    );
    assert_invalid(
        try_add(
            &library,
            &program_id,
            page("Altitude", "Some text"),
            LearningSourceKind::Document,
            LearningSourcePolicy::BeforeUse,
        )
        .await,
    );

    // A new operation may not reuse an existing source id.
    let (existing, _) = add(
        &library,
        &program_id,
        LearningSourceKind::Web,
        LearningSourcePolicy::Manual,
    )
    .await?;
    let (op, version_id) = (id(), id());
    assert_invalid(
        library
            .add(
                new_source(
                    &op,
                    &existing,
                    &version_id,
                    &program_id,
                    web.clone(),
                    manual.clone(),
                ),
                page("Altitude", "Different text"),
                id(),
            )
            .await,
    );
    let (op, source_id, version_id) = (id(), id(), id());
    let bad_operation = NewLearningSource {
        operation_id: "not-a-uuid",
        ..new_source(
            &op,
            &source_id,
            &version_id,
            &program_id,
            web.clone(),
            manual.clone(),
        )
    };
    assert_invalid(
        library
            .add(bad_operation, page("Altitude", "Some text"), id())
            .await,
    );

    // The three noted captures and the existing source; no rejected
    // capture leaves a row behind.
    assert_eq!(
        library_rows(&pool, &program_id).await?,
        fixture_rows + 4,
        "rejected captures must not leave rows behind"
    );
    Ok(())
}

/// How many sources `program_id`'s library holds, deleted ones included.
async fn library_rows(pool: &SqlitePool, program_id: &str) -> Result<i64> {
    sqlx::query_scalar("SELECT count(*) FROM learning_source_library WHERE program_id=?")
        .bind(program_id)
        .fetch_one(pool)
        .await
        .map_err(db)
}
