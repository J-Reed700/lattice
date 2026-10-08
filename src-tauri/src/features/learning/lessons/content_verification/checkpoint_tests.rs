use super::*;
use crate::features::learning::{
    curriculum::{LearningGenerationJobKind, LearningGenerationJobStatus},
    curriculum_repository::LearningCurriculumRepository,
    dto::AcceptLearningProgramRequestDto,
    lesson_drafts,
    plan_dto::StartLearningGenerationJobRequestDto,
    repository::LearningRepository,
};

#[tokio::test]
async fn section_repairs_and_restart_retain_only_completed_unchanged_inventories() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(directory.path().join("sections.sqlite"))
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(options.clone())
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let programs = LearningRepository::new(pool.clone());
    let program = crate::features::learning::tests::fixture();
    programs.create(&program).await?;
    programs
        .accept(&AcceptLearningProgramRequestDto {
            program_id: program.summary.id.clone(),
            expected_revision: 0,
            title: program.summary.title.clone(),
        })
        .await?;
    let program = programs.get(&program.summary.id).await?;
    let lesson_id = &program.modules[0].lessons[0].id;
    let repo = LearningCurriculumRepository::new(pool.clone());
    let job = repo
        .start_job(&StartLearningGenerationJobRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: program.summary.id,
            expected_revision: program.summary.revision,
            kind: LearningGenerationJobKind::LessonPreparation,
            request_json: json!({"lessonIds":[lesson_id]}).to_string(),
            progress_total: 1,
        })
        .await?;
    assert!(repo.begin_job(&job.id).await?);
    let mut candidate = candidate(GOOD);
    candidate["blocks"] = json!((0..5)
        .map(|i| {
            let mut block = candidate["blocks"][0].clone();
            block["title"] = json!(format!("Section {i}"));
            block
        })
        .collect::<Vec<_>>());
    let model = Model {
        fail_coverage_at: Some(1),
        ..Model::new()
    };
    lesson_drafts::run(&repo, &job.id, lesson_id, async {
        lesson_drafts::resume("section-fixture".into()).await?;
        lesson_drafts::save(&candidate.to_string()).await?;
        let sources = vec![source()];
        let result = verify_with_references(
            &model,
            &candidate,
            &ReferenceCollection::lexical(&sources)?,
            &mut ClaimChecks::default(),
        )
        .await;
        assert!(result.is_err(), "Second batch was deliberately interrupted");
        assert_eq!(
            *model.audited_sections.lock().unwrap(),
            vec![vec![0, 1, 2, 3]]
        );
        Ok(())
    })
    .await?;
    drop(programs);
    drop(repo);
    pool.close().await;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    repo.recover_running_jobs().await?;
    assert!(repo.begin_job(&job.id).await?);
    let resumed_model = Model::new();
    lesson_drafts::run(&repo, &job.id, lesson_id, async {
        lesson_drafts::resume("section-fixture".into()).await?;
        let sources = vec![source()];
        verify_with_references(
            &resumed_model,
            &candidate,
            &ReferenceCollection::lexical(&sources)?,
            &mut ClaimChecks::default(),
        )
        .await?;
        assert!(resumed_model.extracted_sections.lock().unwrap().is_empty());
        assert_eq!(
            *resumed_model.audited_sections.lock().unwrap(),
            vec![vec![4]],
            "The aggregate checkpoint predates the first completed audit batch"
        );
        Ok(())
    })
    .await?;
    // The whole-candidate checkpoint must not bypass the model binding of the
    // individual section receipts or relabel old audits as new-model results.
    let changed_model = Model {
        name: "changed-model",
        ..Model::new()
    };
    lesson_drafts::run(&repo, &job.id, lesson_id, async {
        lesson_drafts::resume("section-fixture".into()).await?;
        let sources = vec![source()];
        verify_with_references(
            &changed_model,
            &candidate,
            &ReferenceCollection::lexical(&sources)?,
            &mut ClaimChecks::default(),
        )
        .await?;
        for sections in [
            &changed_model.extracted_sections,
            &changed_model.audited_sections,
        ] {
            let mut checked: Vec<_> = sections.lock().unwrap().iter().flatten().copied().collect();
            checked.sort_unstable();
            assert_eq!(checked, vec![0, 1, 2, 3, 4]);
        }
        Ok(())
    })
    .await?;
    // A later repair must not restart the four finished, unchanged sections.
    candidate["blocks"][1]["body"] = json!(BAD);
    let model = Model::new();
    lesson_drafts::run(&repo, &job.id, lesson_id, async {
        let content = units(&candidate)?;
        let (inventory, coverage) =
            extract_audited_inventory(&model, &content, &candidate.to_string()).await?;
        assert!(
            model.extracted_sections.lock().unwrap().is_empty(),
            "A changed section updates its saved inventory instead of re-extracting it"
        );
        assert_eq!(*model.audited_sections.lock().unwrap(), vec![vec![1]]);
        assert_eq!(inventory.units.len(), 5);
        assert!(coverage.units.iter().all(|unit| unit.complete));
        let sources = vec![source()];
        let report = check_evidence(
            &model,
            &candidate,
            &ReferenceCollection::lexical(&sources)?,
            inventory,
            coverage,
            vec![],
            &mut ClaimChecks::default(),
        )
        .await?;
        assert_eq!(
            report.findings[1].verdict,
            ClaimVerdict::Contradicted,
            "Retained coverage must not approve factual content"
        );
        let (retained, _) = section_checkpoints::load(&model, &content).await?;
        assert_eq!(retained.len(), 5);
        let different = Model {
            name: "previously-unused-model",
            ..Model::new()
        };
        assert!(section_checkpoints::load(&different, &content)
            .await?
            .0
            .is_empty());
        let mut reordered = content.clone();
        reordered.swap(0, 1);
        assert_eq!(
            section_checkpoints::load(&model, &reordered).await?.0.len(),
            3
        );
        Ok(())
    })
    .await?;
    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn interrupted_coverage_retains_omissions_without_reusing_them_after_correction() -> Result<()>
{
    let directory = tempfile::tempdir()?;
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(directory.path().join("coverage.sqlite"))
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(options.clone())
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let programs = LearningRepository::new(pool.clone());
    let program = crate::features::learning::tests::fixture();
    programs.create(&program).await?;
    programs
        .accept(&AcceptLearningProgramRequestDto {
            program_id: program.summary.id.clone(),
            expected_revision: 0,
            title: program.summary.title.clone(),
        })
        .await?;
    let program = programs.get(&program.summary.id).await?;
    let lesson_id = &program.modules[0].lessons[0].id;
    let repo = LearningCurriculumRepository::new(pool.clone());
    let job = repo
        .start_job(&StartLearningGenerationJobRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: program.summary.id,
            expected_revision: program.summary.revision,
            kind: LearningGenerationJobKind::LessonPreparation,
            request_json: json!({"lessonIds":[lesson_id]}).to_string(),
            progress_total: 1,
        })
        .await?;
    assert!(repo.begin_job(&job.id).await?);
    let content: Vec<_> = (0..5)
        .map(|index| json!({"kind":"teaching","title":format!("Section {index}"),"body":GOOD}))
        .collect();
    let inventory = Inventory {
        units: (0..5)
            .map(|index| UnitClaims {
                index,
                claims: vec![Claim {
                    quote: GOOD.into(),
                    statement: GOOD.into(),
                }],
                non_factual_reason: String::new(),
            })
            .collect(),
    };
    let model = Model {
        coverage_gap_in: Some(1),
        fail_coverage_at: Some(1),
        ..Model::new()
    };
    lesson_drafts::run(&repo, &job.id, lesson_id, async {
        assert!(coverage::audit(&model, &content, &inventory, None)
            .await
            .is_err());
        assert_eq!(
            *model.audited_sections.lock().unwrap(),
            vec![vec![0, 1, 2, 3]]
        );
        Ok(())
    })
    .await?;
    drop(programs);
    drop(repo);
    pool.close().await;

    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    repo.recover_running_jobs().await?;
    assert!(repo.begin_job(&job.id).await?);
    lesson_drafts::run(&repo, &job.id, lesson_id, async {
        let model = Model::new();
        let coverage = coverage::audit(&model, &content, &inventory, None).await?;
        assert_eq!(*model.audited_sections.lock().unwrap(), vec![vec![4]]);
        assert_eq!(coverage.units.len(), 5);
        assert!(
            !coverage.units[1].complete,
            "A saved omission must remain unresolved"
        );
        assert_eq!(coverage.units[1].unresolved_passages.len(), 1);
        assert_eq!(
            coverage.units.iter().filter(|unit| unit.complete).count(),
            4
        );

        let mut corrected = inventory.clone();
        corrected.units[1].claims[0]
            .statement
            .push_str(" An additional extracted assertion.");
        let model = Model::new();
        let checked = coverage::audit(&model, &content, &corrected, None).await?;
        assert_eq!(*model.audited_sections.lock().unwrap(), vec![vec![1]]);
        assert!(checked.units.iter().all(|unit| unit.complete));

        let mut changed = content.clone();
        changed[2]["body"] = json!(format!("{GOOD} Another original assertion."));
        let model = Model::new();
        coverage::audit(&model, &changed, &corrected, None).await?;
        assert_eq!(*model.audited_sections.lock().unwrap(), vec![vec![2]]);

        let model = Model {
            name: "different-coverage-model",
            ..Model::new()
        };
        coverage::audit(&model, &changed, &corrected, None).await?;
        assert_eq!(
            *model.audited_sections.lock().unwrap(),
            vec![vec![0, 1, 2, 3], vec![4]]
        );
        Ok(())
    })
    .await?;
    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn interrupted_verification_reuses_completed_checks_and_rechecks_changed_evidence(
) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(directory.path().join("checkpoints.sqlite"))
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(options.clone())
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let programs = LearningRepository::new(pool.clone());
    let program = crate::features::learning::tests::fixture();
    programs.create(&program).await?;
    programs
        .accept(&AcceptLearningProgramRequestDto {
            program_id: program.summary.id.clone(),
            expected_revision: 0,
            title: program.summary.title.clone(),
        })
        .await?;
    let program = programs.get(&program.summary.id).await?;
    let lesson_id = &program.modules[0].lessons[0].id;
    let repo = LearningCurriculumRepository::new(pool.clone());
    let job = repo
        .start_job(&StartLearningGenerationJobRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: program.summary.id,
            expected_revision: program.summary.revision,
            kind: LearningGenerationJobKind::LessonPreparation,
            request_json: json!({"lessonIds":[lesson_id]}).to_string(),
            progress_total: 1,
        })
        .await?;
    assert!(repo.begin_job(&job.id).await?);
    let mut model = Model::new();
    model.hold_bad_claim = true;
    let sources = vec![source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let inventory = Inventory {
        units: vec![UnitClaims {
            index: 0,
            claims: vec![
                Claim {
                    quote: BAD.into(),
                    statement: BAD.into(),
                },
                Claim {
                    quote: GOOD.into(),
                    statement: GOOD.into(),
                },
            ],
            non_factual_reason: String::new(),
        }],
    };
    let claim = &inventory.units[0].claims[1];
    let evidence = evidence_for(GOOD, 0, &references, &[]).await?;
    let key = claim_receipt_key(&model, &ClaimChecks::key(0, claim, &evidence));
    let candidate = candidate(GOOD);
    // First claim never finishes. The second must still be checkpointed; a
    // buffered/in-order stream would lose its work on interruption here.
    lesson_drafts::run(&repo, &job.id, lesson_id, async {
        let mut checks = ClaimChecks::default();
        let work = check_evidence(
            &model,
            &candidate,
            &references,
            inventory.clone(),
            Coverage { units: vec![] },
            vec![],
            &mut checks,
        );
        tokio::select! {
            result = work => panic!("The deliberately blocked claim finished: {result:?}"),
            result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                loop {
                    // Retrieval may complete out of order too. Interrupt only
                    // once the unfinished call has actually started and the
                    // other comparison has durably finished.
                    if lesson_drafts::checkpoint(&key).await?.is_some()
                        && *model.judge_calls.lock().unwrap() == 2
                    { return Ok::<_, AppError>(()); }
                    tokio::task::yield_now().await;
                }
            }) => result.expect("completed check was not saved")?,
        }
        Ok(())
    })
    .await?;
    assert_eq!(*model.judge_calls.lock().unwrap(), 2);
    let writes = (0..12).map(|i| {
        let repo = &repo;
        let job_id = &job.id;
        async move {
            repo.save_lesson_checkpoint(job_id, lesson_id, &format!("parallel-{i}"), json!(i))
                .await
        }
    });
    for result in futures::future::join_all(writes).await {
        result?;
    }
    // Close every connection and reopen the file, without an orderly job
    // transition. This is the durable state a new process sees after a crash.
    drop(repo);
    drop(programs);
    pool.close().await;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    repo.recover_running_jobs().await?;
    assert_eq!(
        repo.job(&job.id).await?.status,
        LearningGenerationJobStatus::Pending
    );
    let (left, right) = tokio::join!(repo.begin_job(&job.id), repo.begin_job(&job.id));
    assert_ne!(
        left?, right?,
        "Duplicate delivery must have exactly one claimant"
    );
    for i in 0..12 {
        assert_eq!(
            repo.lesson_checkpoint(&job.id, lesson_id, &format!("parallel-{i}"))
                .await?,
            Some(json!(i))
        );
    }
    let model = Model::new();
    lesson_drafts::run(&repo, &job.id, lesson_id, async {
        let report = check_evidence(
            &model,
            &candidate,
            &references,
            inventory.clone(),
            Coverage { units: vec![] },
            vec![],
            &mut ClaimChecks::default(),
        )
        .await?;
        assert_eq!(
            *model.judge_calls.lock().unwrap(),
            1,
            "Only the interrupted comparison should repeat"
        );
        assert_eq!(report.findings[0].verdict, ClaimVerdict::Contradicted);
        assert_eq!(report.findings[1].verdict, ClaimVerdict::Supported);
        let mut changed = sources.clone();
        changed[0]
            .excerpt
            .push_str(" Additional evidence changes the judgment input.");
        check_evidence(
            &model,
            &candidate,
            &ReferenceCollection::lexical(&changed)?,
            inventory,
            Coverage { units: vec![] },
            vec![],
            &mut ClaimChecks::default(),
        )
        .await?;
        assert_eq!(
            *model.judge_calls.lock().unwrap(),
            3,
            "Changed evidence cannot reuse approvals"
        );
        let changed_model = Model {
            name: "different-checker",
            ..Model::new()
        };
        check_evidence(
            &changed_model,
            &candidate,
            &references,
            Inventory {
                units: vec![UnitClaims {
                    index: 0,
                    claims: vec![Claim {
                        quote: GOOD.into(),
                        statement: GOOD.into(),
                    }],
                    non_factual_reason: String::new(),
                }],
            },
            Coverage { units: vec![] },
            vec![],
            &mut ClaimChecks::default(),
        )
        .await?;
        assert_eq!(
            *changed_model.judge_calls.lock().unwrap(),
            1,
            "A changed model must check again"
        );
        Ok(())
    })
    .await?;
    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn verified_lesson_checkpoint_is_bound_to_content_sources_policy_and_model() -> Result<()> {
    let model = Model::new();
    let sources = vec![source()];
    let mut candidate = candidate(GOOD);
    candidate["blocks"][0]["sourceIds"] = json!([]);
    let second = candidate["blocks"][0].clone();
    candidate["blocks"].as_array_mut().unwrap().push(second);
    let report = verify(&model, &candidate, &sources).await?;
    let mut prepared = PreparedLearningLesson {
        verification: None,
        blocks: serde_json::from_value(candidate["blocks"].clone())?,
        questions: vec![],
        keys: vec![],
    };
    prepared.verification = Some(report.bind("lesson", &prepared)?);
    let resumed: PreparedLearningLesson = serde_json::from_value(serde_json::to_value(&prepared)?)?;
    let report = resumed.verification.as_ref().unwrap();
    assert!(report.reusable("lesson", &resumed, &sources, &model));
    let mut missing = report.clone();
    missing.findings.pop();
    assert!(
        !missing.reusable("lesson", &resumed, &sources, &model),
        "A completed checkpoint cannot approve only a subset of the inventory"
    );
    let mut duplicate = report.clone();
    duplicate.findings[1] = duplicate.findings[0].clone();
    assert!(
        !duplicate.reusable("lesson", &resumed, &sources, &model),
        "Duplicating a result cannot stand in for a missing section's claim"
    );
    assert!(!report.reusable("other-lesson", &resumed, &sources, &model));
    let mut altered = resumed.clone();
    altered.blocks[0].body.push_str(" An edit.");
    assert!(!report.reusable("lesson", &altered, &sources, &model));
    let mut changed_sources = sources.clone();
    changed_sources[0].excerpt.push(' ');
    assert!(!report.reusable("lesson", &resumed, &changed_sources, &model));
    changed_sources = sources.clone();
    changed_sources.push(source());
    assert!(!report.reusable("lesson", &resumed, &changed_sources, &model));
    let mut obsolete = report.clone();
    obsolete.policy = "obsolete".into();
    assert!(!obsolete.reusable("lesson", &resumed, &sources, &model));
    let changed_model = Model {
        name: "different-checker",
        ..Model::new()
    };
    assert!(!report.reusable("lesson", &resumed, &sources, &changed_model));
    Ok(())
}

#[test]
fn incomplete_or_unquoted_receipts_never_resume_as_approval() {
    let claim = Claim {
        quote: GOOD.into(),
        statement: GOOD.into(),
    };
    for verdict in [ClaimVerdict::Unverified, ClaimVerdict::Supported] {
        let receipt = ClaimReceipt {
            verdict,
            reason: "Incomplete fixture".into(),
            supporting_quote: None,
            interpretation_policy: None,
        };
        assert!(receipt.finding(0, &claim, &[]).is_none());
    }
}

#[test]
fn interpretation_update_keeps_approvals_and_reconsiders_old_negative_receipts() {
    let claim = Claim {
        quote: GOOD.into(),
        statement: GOOD.into(),
    };
    let evidence = vec![EvidencePassage {
        source_id: "reference".into(),
        text: GOOD.into(),
        start_byte: 0,
        end_byte: GOOD.len(),
        retrieval_kind: "test".into(),
        score: 0.0,
    }];
    for (verdict, policy, reusable) in [
        (ClaimVerdict::Supported, None, true),
        (ClaimVerdict::Unsupported, None, false),
        (ClaimVerdict::Contradicted, None, false),
        (
            ClaimVerdict::Unsupported,
            Some(crate::application::services::claim_verification::STRICT_INTERPRETATION_POLICY),
            true,
        ),
    ] {
        let receipt = ClaimReceipt {
            verdict,
            reason: "Recorded comparison".into(),
            supporting_quote: Some(GOOD.into()),
            interpretation_policy: policy.map(str::to_owned),
        };
        assert_eq!(receipt.finding(0, &claim, &evidence).is_some(), reusable);
    }
}
