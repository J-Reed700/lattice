use super::*;
use crate::application::ports::llm_port::CompletionInput;
use crate::features::learning::{
    curriculum::LearningGenerationJobKind, curriculum_repository::LearningCurriculumRepository,
    dto::AcceptLearningProgramRequestDto, lesson_drafts,
    plan_dto::StartLearningGenerationJobRequestDto, repository::LearningRepository,
};

#[derive(Default)]
struct BatchModel {
    calls: Mutex<Vec<&'static str>>,
    progress_calls: std::sync::atomic::AtomicUsize,
    active_calls: std::sync::atomic::AtomicUsize,
    peak_calls: std::sync::atomic::AtomicUsize,
    malformed_first: bool,
    block_individual: bool,
    block_batch: bool,
    block_challenge: bool,
    max_batch_size: Option<usize>,
    batch_error: Option<AppError>,
    batch_sizes: Mutex<Vec<usize>>,
}
#[async_trait::async_trait]
impl LLMPort for BatchModel {
    fn supports_typed_completions(&self) -> bool {
        true
    }
    fn model_name(&self) -> &str {
        "batch-scheduler-fixture"
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    fn max_context_tokens(&self) -> usize {
        64000
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
    async fn generate(&self, _: &str, _: &[String], _: Option<Vec<String>>) -> Result<String> {
        Err(invalid("Unexpected legacy request"))
    }
    async fn generate_streaming(
        &self,
        _: &str,
        _: &[String],
        _: Option<Vec<String>>,
    ) -> Result<Box<dyn futures::Stream<Item = Result<String>> + Send + Unpin + '_>> {
        Err(invalid("Unexpected legacy stream"))
    }
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let system = request
            .input
            .iter()
            .find_map(|item| match item {
                CompletionInput::Message { role, content } if role == "system" => Some(content),
                _ => None,
            })
            .unwrap();
        let user = request
            .input
            .iter()
            .find_map(|item| match item {
                CompletionInput::Message { role, content } if role == "user" => Some(content),
                _ => None,
            })
            .unwrap();
        let challenge = system.starts_with("Plan independent evidence searches");
        let batch = serde_json::from_str::<Value>(user)
            .ok()
            .filter(|input| input["claims"].is_array());
        self.calls
            .lock()
            .unwrap()
            .push(match (batch.is_some(), challenge) {
                (true, false) => "batch-evidence",
                (true, true) => "batch-challenge",
                (false, false) => "individual-evidence",
                (false, true) => "individual-challenge",
            });
        let text = if let Some(batch) = batch {
            let size = batch["claims"].as_array().unwrap().len();
            if challenge && self.block_challenge {
                std::future::pending::<()>().await;
            }
            if !challenge {
                self.batch_sizes.lock().unwrap().push(size);
                if self.block_batch {
                    std::future::pending::<()>().await;
                }
                if self.max_batch_size.is_some_and(|max| size > max) {
                    return Err(AppError::ServiceNotAvailable(
                        "Fixture server cannot finish this group".into(),
                    ));
                }
                if let Some(error) = &self.batch_error {
                    return Err(error.clone());
                }
            }
            // All ten fixture assertions use the same source: it is sent once.
            assert_eq!(batch["sourcePassages"].as_array().unwrap().len(), 1);
            let checks:Vec<_>=batch["claims"].as_array().unwrap().iter().map(|claim| {
                if challenge {json!({"id":claim["id"],"queries":[]})}
                else {json!({"id":claim["id"],"response":if self.malformed_first&&claim["id"]=="claim-0" {"incomplete response".into()}else{format!("Reason: The fixture passage states the claim.\nSource passage: {}\nVerdict: supported",claim["evidenceIds"][0].as_str().unwrap())}})}
            }).collect();
            json!({"checks":checks}).to_string()
        } else if challenge {
            json!({"queries":[]}).to_string()
        } else {
            if self.block_individual {
                std::future::pending::<()>().await;
            }
            "Reason: The fixture passage states the claim.\nSource passage: passage-0\nVerdict: supported".into()
        };
        Ok(CompletionResponse {
            text,
            finish_reason: "stop".into(),
            ..Default::default()
        })
    }
    async fn complete_with_retry_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_retry: &(dyn Fn(usize) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct Active<'a>(&'a AtomicUsize);
        impl Drop for Active<'_> {
            fn drop(&mut self) {
                self.0.fetch_sub(1, Ordering::Relaxed);
            }
        }
        let active = self.active_calls.fetch_add(1, Ordering::Relaxed) + 1;
        let _active = Active(&self.active_calls);
        self.peak_calls.fetch_max(active, Ordering::Relaxed);
        tokio::task::yield_now().await;
        self.progress_calls
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        on_retry(1)?;
        let response = self.complete(request).await?;
        on_text(response.text.clone())?;
        Ok(response)
    }
}

#[tokio::test]
async fn interrupted_challenge_resumes_without_repeating_entailment() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(directory.path().join("challenge-resume.sqlite"))
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
    let (repo, job, lesson) = setup(&pool).await?;
    let sources = vec![source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let interrupted = BatchModel {
        block_challenge: true,
        ..Default::default()
    };
    crate::features::learning::lesson_progress::run(
        &repo,
        &job,
        lesson_drafts::run(&repo, &job, &lesson, async {
            let claims = inventory(8);
            let retained = ClaimChecks::default();
            let work = evidence_checks::check(&interrupted, &references, &claims, &[], &retained);
            tokio::select! {
                result = work => panic!("The challenge should still be running: {result:?}"),
                result = tokio::time::timeout(Duration::from_secs(5), async {
                    while !interrupted.calls.lock().unwrap().contains(&"batch-challenge") {
                        tokio::task::yield_now().await;
                    }
                }) => result.expect("The challenge never started"),
            }
            Ok(())
        }),
    )
    .await?;
    assert_eq!(
        *interrupted.calls.lock().unwrap(),
        vec!["batch-evidence", "batch-challenge"]
    );
    assert!(
        repo.job(&job)
            .await?
            .progress_message
            .contains("Challenging 8 provisional approvals"),
        "The durable UI should identify the challenge stage instead of appearing frozen"
    );
    drop(repo);
    pool.close().await;

    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    repo.recover_running_jobs().await?;
    assert!(repo.begin_job(&job).await?);
    let resumed = BatchModel::default();
    lesson_drafts::run(&repo, &job, &lesson, async {
        let results = evidence_checks::check(
            &resumed,
            &references,
            &inventory(8),
            &[],
            &ClaimChecks::default(),
        )
        .await?;
        assert_eq!(results.len(), 8);
        assert!(results
            .iter()
            .all(|result| result.2.verdict == ClaimVerdict::Supported));
        Ok(())
    })
    .await?;
    assert_eq!(
        *resumed.calls.lock().unwrap(),
        vec!["batch-challenge"],
        "Resume should reuse the durable entailment result and continue at the challenge"
    );
    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn failing_groups_split_and_remember_the_plan_after_reopen() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(directory.path().join("adaptive-batches.db"))
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options.clone())
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let (repo, job, lesson) = setup(&pool).await?;
    let sources = vec![source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let model = BatchModel {
        max_batch_size: Some(4),
        batch_error: Some(AppError::Network(
            "Interrupted after the split was saved".into(),
        )),
        ..Default::default()
    };
    let result = lesson_drafts::run(&repo, &job, &lesson, async {
        check_evidence(
            &model,
            &candidate(GOOD),
            &references,
            inventory(8),
            coverage(8),
            vec![],
            &mut ClaimChecks::default(),
        )
        .await
    })
    .await;
    assert!(matches!(result, Err(AppError::Network(_))));
    assert_eq!(*model.batch_sizes.lock().unwrap(), vec![8, 4]);
    drop(repo);
    pool.close().await;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    repo.recover_running_jobs().await?;
    assert!(repo.begin_job(&job).await?);
    let model = BatchModel {
        max_batch_size: Some(4),
        ..Default::default()
    };
    lesson_drafts::run(&repo, &job, &lesson, async {
        let report = check_evidence(
            &model,
            &candidate(GOOD),
            &references,
            inventory(8),
            coverage(8),
            vec![],
            &mut ClaimChecks::default(),
        )
        .await?;
        assert!(report.issues.is_empty());
        assert_eq!(report.findings.len(), 8);
        assert!(report
            .findings
            .iter()
            .all(|finding| finding.verdict == ClaimVerdict::Supported));
        assert_eq!(
            *model.batch_sizes.lock().unwrap(),
            vec![4, 4],
            "Never replay the failed eight-claim group after reopening"
        );
        // All eight decisions, including independent challenges, are reusable.
        check_evidence(
            &model,
            &candidate(GOOD),
            &references,
            inventory(8),
            coverage(8),
            vec![],
            &mut ClaimChecks::default(),
        )
        .await?;
        assert_eq!(*model.batch_sizes.lock().unwrap(), vec![4, 4]);
        assert_eq!(model.calls.lock().unwrap().len(), 4);
        Ok(())
    })
    .await?;
    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn connection_loss_rate_limits_and_invalid_requests_do_not_fan_out() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson) = setup(&pool).await?;
    let sources = vec![source()];
    let references = ReferenceCollection::lexical(&sources)?;
    for error in [
        AppError::Network("offline".into()),
        AppError::RateLimitExceeded("busy".into()),
        AppError::InvalidState("bad request".into()),
    ] {
        let model = BatchModel {
            batch_error: Some(error.clone()),
            ..Default::default()
        };
        let result = lesson_drafts::run(&repo, &job, &lesson, async {
            check_evidence(
                &model,
                &candidate(GOOD),
                &references,
                inventory(8),
                coverage(8),
                vec![],
                &mut ClaimChecks::default(),
            )
            .await
        })
        .await;
        assert_eq!(result.unwrap_err().error_code(), error.error_code());
        assert_eq!(*model.batch_sizes.lock().unwrap(), vec![8]);
    }
    Ok(())
}

fn inventory(count: usize) -> Inventory {
    Inventory {
        units: (0..count)
            .map(|index| UnitClaims {
                index,
                claims: vec![Claim {
                    quote: GOOD.into(),
                    statement: GOOD.into(),
                }],
                non_factual_reason: String::new(),
            })
            .collect(),
    }
}
fn coverage(count: usize) -> Coverage {
    Coverage {
        units: (0..count)
            .map(|index| CoverageUnit {
                index,
                complete: true,
                reason: "Fixture coverage".into(),
                unresolved_passages: vec![],
            })
            .collect(),
    }
}

pub(in crate::features::learning::lessons::content_verification) async fn setup(
    pool: &sqlx::SqlitePool,
) -> Result<(LearningCurriculumRepository, String, String)> {
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
    let lesson_id = program.modules[0].lessons[0].id.clone();
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
    Ok((repo, job.id, lesson_id))
}

#[tokio::test]
async fn batched_checks_reuse_individual_receipts_and_recheck_changed_evidence() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson) = setup(&pool).await?;
    let mut sources = vec![source()];
    let model = BatchModel::default();
    lesson_drafts::run(&repo, &job, &lesson, async {
        for pass in 0..3 {
            if pass == 2 {
                sources[0].excerpt.push_str(" Additional source context.");
            }
            let references = ReferenceCollection::lexical(&sources)?;
            let report = check_evidence(
                &model,
                &candidate(GOOD),
                &references,
                inventory(10),
                coverage(10),
                vec![],
                &mut ClaimChecks::default(),
            )
            .await?;
            assert_eq!(report.findings.len(), 10);
            assert!(report
                .findings
                .iter()
                .all(|f| f.verdict == ClaimVerdict::Supported));
            let expected = if pass < 2 { 4 } else { 8 };
            assert_eq!(
                model.calls.lock().unwrap().len(),
                expected,
                "Two requests for each group, then zero requests for unchanged checkpoints"
            );
            assert_eq!(
                model
                    .progress_calls
                    .load(std::sync::atomic::Ordering::Relaxed),
                expected,
                "Both batch passes must report streaming and retry progress"
            );
            assert_eq!(
                model.peak_calls.load(std::sync::atomic::Ordering::Relaxed),
                1
            );
        }
        Ok(())
    })
    .await?;
    Ok(())
}

#[tokio::test]
async fn restores_all_170_claim_receipts_before_slow_checks_and_batches_only_missing_work(
) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(directory.path().join("restore-before-model.sqlite"))
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options.clone())
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let (repo, job, lesson) = setup(&pool).await?;
    let sources = vec![source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let mut saved = inventory(170);
    saved
        .units
        .retain(|unit| ![0, 85, 169].contains(&unit.index));
    lesson_drafts::run(&repo, &job, &lesson, async {
        evidence_checks::check(
            &BatchModel::default(),
            &references,
            &saved,
            &[],
            &ClaimChecks::default(),
        )
        .await?;
        Ok(())
    })
    .await?;
    let blocked = BatchModel {
        block_batch: true,
        block_individual: true,
        ..Default::default()
    };
    crate::features::learning::lesson_progress::run(&repo, &job, lesson_drafts::run(&repo, &job, &lesson, async {
        let inventory = inventory(170);
        let retained = ClaimChecks::default();
        let work = evidence_checks::check(&blocked, &references, &inventory, &[], &retained);
        tokio::select! {
            result = work => panic!("The unfinished checks should still be running: {result:?}"),
            result = tokio::time::timeout(Duration::from_secs(5), async {
                while blocked.calls.lock().unwrap().is_empty() {
                    tokio::task::yield_now().await;
                }
            }) => result.expect("The model request never started"),
        }
        Ok(())
    })).await?;
    let activity = repo.job(&job).await?.activity.unwrap();
    assert_eq!(activity.checks_total, 170);
    assert_eq!(
        activity.checks_reused, 167,
        "Late saved checks must be visible before the slow first request finishes"
    );
    assert_eq!(activity.checks_completed, 167);
    assert_eq!(activity.model_checks_total, Some(3));
    assert_eq!(
        *blocked.batch_sizes.lock().unwrap(),
        vec![3],
        "Pending claims from separate inventory groups should share one request"
    );
    drop(repo);
    pool.close().await;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    repo.recover_running_jobs().await?;
    assert!(repo.begin_job(&job).await?);
    let resumed = BatchModel::default();
    lesson_drafts::run(&repo, &job, &lesson, async {
        let results = evidence_checks::check(
            &resumed,
            &references,
            &inventory(170),
            &[],
            &ClaimChecks::default(),
        )
        .await?;
        assert_eq!(results.len(), 170);
        assert_eq!(results.iter().filter(|result| result.3).count(), 167);
        assert!(results
            .iter()
            .all(|result| result.2.verdict == ClaimVerdict::Supported));
        assert_eq!(
            results.iter().map(|result| result.0).collect::<Vec<_>>(),
            (0..170).collect::<Vec<_>>()
        );
        Ok(())
    })
    .await?;
    assert_eq!(*resumed.batch_sizes.lock().unwrap(), vec![3]);
    assert_eq!(
        *resumed.calls.lock().unwrap(),
        vec!["batch-evidence", "batch-challenge"]
    );
    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn completed_batch_siblings_survive_interruption_during_individual_fallback() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(dir.path().join("batch.sqlite"))
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
    let (repo, job, lesson) = setup(&pool).await?;
    let sources = vec![source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let model = BatchModel {
        malformed_first: true,
        block_individual: true,
        ..Default::default()
    };
    let input = inventory(8);
    let claim = &input.units[7].claims[0];
    let evidence = evidence_for(&claim.statement, 7, &references, &[]).await?;
    let key = claim_receipt_key(&model, &ClaimChecks::key(7, claim, &evidence));
    crate::features::learning::lesson_progress::run(&repo, &job, lesson_drafts::run(&repo,&job,&lesson,async {
        let mut retained=ClaimChecks::default();let candidate=candidate(GOOD);
        let work=check_evidence(&model,&candidate,&references,input,coverage(8),vec![],&mut retained);
        tokio::select! {
            result=work=>panic!("The incomplete claim should be waiting: {result:?}"),
            result=tokio::time::timeout(Duration::from_secs(5),async {
                loop {
                    if lesson_drafts::checkpoint(&key).await?.is_some()&&model.calls.lock().unwrap().contains(&"individual-evidence") {return Ok::<_,AppError>(());}
                    tokio::task::yield_now().await;
                }
            })=>result.expect("completed siblings were not saved")?,
        }
        Ok(())
    })).await?;
    let activity = repo.job(&job).await?.activity.unwrap();
    assert_eq!(
        activity.checks_completed, 7,
        "Progress must include completed siblings while the last check is blocked"
    );
    assert_eq!(activity.checks_unresolved, 0);
    assert_eq!(activity.checks_reused, 0);
    drop(repo);
    pool.close().await;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    repo.recover_running_jobs().await?;
    assert!(repo.begin_job(&job).await?);
    let model = BatchModel::default();
    lesson_drafts::run(&repo, &job, &lesson, async {
        let report = check_evidence(
            &model,
            &candidate(GOOD),
            &references,
            inventory(8),
            coverage(8),
            vec![],
            &mut ClaimChecks::default(),
        )
        .await?;
        assert!(report.issues.is_empty());
        assert_eq!(
            *model.calls.lock().unwrap(),
            vec!["individual-evidence", "individual-challenge"]
        );
        Ok(())
    })
    .await?;
    pool.close().await;
    Ok(())
}
