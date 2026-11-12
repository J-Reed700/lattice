use super::super::readings::Read;
use super::bridge_tests::{citing, STATED};
use super::readings_tests::{claim, driven, paged, read_observed, world_of};
use super::*;
pub(in crate::lessons) use crate::content_verification::test_support::setup;
use crate::curriculum_repository::LESSON_PREPARATION;
use crate::{curriculum_repository::LearningCurriculumRepository, lesson_drafts};
use lattice_core::application::ports::llm_port::CompletionInput;
use lattice_core::shared::runtime::jobs::{JobContext, JobStore, RecoveryPolicy};

#[derive(Default)]
struct BatchModel {
    calls: Mutex<Vec<&'static str>>,
    progress_calls: std::sync::atomic::AtomicUsize,
    active_calls: std::sync::atomic::AtomicUsize,
    peak_calls: std::sync::atomic::AtomicUsize,
    malformed_first: bool,
    block_individual: bool,
    block_batch: bool,
    max_batch_size: Option<usize>,
    /// A group of more claims than this fits the window by the calibrated
    /// estimate but not by the backend's exact count, and is refused for its
    /// size with the numbers, as the scheduler refuses it.
    window_holds: Option<usize>,
    batch_error: Option<AppError>,
    batch_sizes: Mutex<Vec<usize>>,
}
#[async_trait::async_trait]
impl LLMPort for BatchModel {
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

    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let user = request
            .input
            .iter()
            .find_map(|item| match item {
                CompletionInput::Message { role, content } if role == "user" => Some(content),
                _ => None,
            })
            .unwrap();
        let batch = serde_json::from_str::<Value>(user)
            .ok()
            .filter(|input| input["claims"].is_array());
        self.calls.lock().unwrap().push(if batch.is_some() {
            "batch-evidence"
        } else {
            "individual-evidence"
        });
        let text = if let Some(batch) = batch {
            let size = batch["claims"].as_array().unwrap().len();
            self.batch_sizes.lock().unwrap().push(size);
            if self.block_batch {
                std::future::pending::<()>().await;
            }
            if self.window_holds.is_some_and(|holds| size > holds) {
                let window = self.max_context_tokens();
                return Err(lattice_core::application::ports::llm_port::window_refusal(
                    self.model_name(),
                    lattice_core::application::ports::llm_port::RequestSize {
                        prompt_tokens: window + 831,
                        counted_exactly: true,
                        ..Default::default()
                    },
                    window,
                ));
            }
            if self.max_batch_size.is_some_and(|max| size > max) {
                return Err(AppError::ServiceNotAvailable(
                    "Fixture server cannot finish this group".into(),
                ));
            }
            if let Some(error) = &self.batch_error {
                return Err(error.clone());
            }
            // All ten fixture assertions use the same source: it is sent once.
            assert_eq!(batch["sourcePassages"].as_array().unwrap().len(), 1);
            let checks:Vec<_>=batch["claims"].as_array().unwrap().iter().map(|claim| {
                json!({"id":claim["id"],"response":if self.malformed_first&&claim["id"]=="claim-0" {"incomplete response".into()}else{format!("Reason: The fixture passage states the claim.\nSource passage: {}\nVerdict: supported",claim["evidenceIds"][0].as_str().unwrap())}})
            }).collect();
            json!({"checks":checks}).to_string()
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
async fn failing_groups_split_and_remember_the_plan_after_reopen() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let options = crate::test_support::file_options(directory.path().join("adaptive-batches.db"));
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options.clone())
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    sqlx::migrate!("../lattice-desktop/migrations")
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
    let result = lesson_drafts::run(&Database::from(pool.clone()), &repo, &job, &lesson, async {
        paged(&model, &references, &inventory(8), &measured()).await
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
    JobStore::new(pool.clone())
        .recover(LESSON_PREPARATION, RecoveryPolicy::Requeue)
        .await?;
    assert!(JobStore::new(pool.clone()).claim(&job).await?.is_some());
    let model = BatchModel {
        max_batch_size: Some(4),
        ..Default::default()
    };
    lesson_drafts::run(&Database::from(pool.clone()), &repo, &job, &lesson, async {
        let reads = paged(&model, &references, &inventory(8), &measured()).await?;
        assert_eq!(reads.len(), 8);
        assert!(reads
            .iter()
            .all(|read| read.finding().verdict == ClaimVerdict::Supported));
        assert_eq!(
            *model.batch_sizes.lock().unwrap(),
            vec![4, 4],
            "Never replay the failed eight-claim group after reopening"
        );
        // All eight decisions are reusable.
        paged(&model, &references, &inventory(8), &measured()).await?;
        assert_eq!(*model.batch_sizes.lock().unwrap(), vec![4, 4]);
        // One check per group: no second call follows a check.
        assert_eq!(model.calls.lock().unwrap().len(), 2);
        Ok(())
    })
    .await?;
    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn a_group_counted_past_the_window_is_split_and_every_claim_is_judged() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let (repo, job, lesson) = setup(&pool).await?;
    let sources = vec![source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let model = BatchModel {
        window_holds: Some(4),
        ..Default::default()
    };
    lesson_drafts::run(&Database::from(pool.clone()), &repo, &job, &lesson, async {
        let reads = paged(&model, &references, &inventory(8), &measured()).await?;
        assert_eq!(reads.len(), 8);
        assert!(reads
            .iter()
            .all(|read| read.finding().verdict == ClaimVerdict::Supported));
        // The refused group is checked in halves on the same evidence; no
        // claim falls back to a check of its own.
        assert_eq!(*model.batch_sizes.lock().unwrap(), vec![8, 4, 4]);
        assert!(!model.calls.lock().unwrap().contains(&"individual-evidence"));
        Ok(())
    })
    .await?;
    Ok(())
}

#[tokio::test]
async fn a_group_counted_past_the_window_is_never_sent_again() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let (repo, job, lesson) = setup(&pool).await?;
    let sources = vec![source()];
    let references = ReferenceCollection::lexical(&sources)?;
    lesson_drafts::run(&Database::from(pool.clone()), &repo, &job, &lesson, async {
        // The refusal splits the group and records the split; an outage then
        // stops the first half.
        let interrupted = BatchModel {
            window_holds: Some(4),
            batch_error: Some(AppError::Network(
                "Interrupted after the split was saved".into(),
            )),
            ..Default::default()
        };
        let result = paged(&interrupted, &references, &inventory(8), &measured()).await;
        assert!(matches!(result, Err(AppError::Network(_))));
        assert_eq!(*interrupted.batch_sizes.lock().unwrap(), vec![8, 4]);
        // Resumed, the recorded split sends halves straight away.
        let resumed = BatchModel {
            window_holds: Some(4),
            ..Default::default()
        };
        let reads = paged(&resumed, &references, &inventory(8), &measured()).await?;
        assert_eq!(reads.len(), 8);
        assert_eq!(
            *resumed.batch_sizes.lock().unwrap(),
            vec![4, 4],
            "Never resend a group the backend counted past the window"
        );
        Ok(())
    })
    .await?;
    Ok(())
}

#[tokio::test]
async fn connection_loss_rate_limits_and_invalid_requests_do_not_fan_out() -> Result<()> {
    let pool = crate::tests::pool().await?;
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
        let result =
            lesson_drafts::run(&Database::from(pool.clone()), &repo, &job, &lesson, async {
                paged(&model, &references, &inventory(8), &measured()).await
            })
            .await;
        assert_eq!(
            lattice_core::shared::ErrorCode::from(&result.unwrap_err()),
            lattice_core::shared::ErrorCode::from(&error)
        );
        assert_eq!(*model.batch_sizes.lock().unwrap(), vec![8]);
    }
    Ok(())
}

/// `count` claims of one section, as requests hold the claims of one
/// section only.
fn inventory(count: usize) -> Inventory {
    sections(1, count)
}

/// `sections` sections of `claims` claims each, every statement its own.
fn sections(sections: usize, claims: usize) -> Inventory {
    Inventory {
        units: (0..sections)
            .map(|index| UnitClaims {
                index,
                claims: (0..claims)
                    .map(|claim| Claim {
                        quote: GOOD.into(),
                        statement: format!("{GOOD} Case {}.", index * claims + claim),
                    })
                    .collect(),
                non_factual_reason: String::new(),
            })
            .collect(),
    }
}

#[tokio::test]
async fn batched_checks_reuse_individual_receipts_and_recheck_changed_evidence() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let (repo, job, lesson) = setup(&pool).await?;
    let mut sources = vec![source()];
    let model = BatchModel::default();
    lesson_drafts::run(&Database::from(pool.clone()), &repo, &job, &lesson, async {
        for pass in 0..3 {
            if pass == 2 {
                sources[0].excerpt.push_str(" Additional source context.");
            }
            let references = ReferenceCollection::lexical(&sources)?;
            let reads = paged(&model, &references, &inventory(10), &measured()).await?;
            assert_eq!(reads.len(), 10);
            assert!(reads
                .iter()
                .all(|read| read.finding().verdict == ClaimVerdict::Supported));
            let expected = if pass < 2 { 1 } else { 2 };
            assert_eq!(
                model.calls.lock().unwrap().len(),
                expected,
                "One request for the section, then zero requests for unchanged checkpoints"
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
    let options =
        crate::test_support::file_options(directory.path().join("restore-before-model.sqlite"));
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options.clone())
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    sqlx::migrate!("../lattice-desktop/migrations")
        .run(&pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let (repo, job, lesson) = setup(&pool).await?;
    let sources = vec![source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let mut saved = inventory(170);
    let missing: Vec<String> = [0, 85, 169]
        .map(|index| saved.units[0].claims[index].statement.clone())
        .into();
    saved.units[0]
        .claims
        .retain(|claim| !missing.contains(&claim.statement));
    lesson_drafts::run(&Database::from(pool.clone()), &repo, &job, &lesson, async {
        paged(&BatchModel::default(), &references, &saved, &measured()).await?;
        Ok(())
    })
    .await?;
    let blocked = BatchModel {
        block_batch: true,
        block_individual: true,
        ..Default::default()
    };
    crate::lesson_progress::run(&JobContext::detached(&JobStore::new(pool.clone()), &job).await?, lesson_drafts::run(&Database::from(pool.clone()), &repo, &job, &lesson, async {
        let inventory = inventory(170);
        let retained = measured();
        crate::lesson_progress::begin_checks(170);
        let work = driven(&blocked, &references, &inventory, &retained);
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
    let progress = repo.job(&job).await?;
    assert!(
        progress
            .progress_message
            .starts_with("Comparing 3 statements of section 1 with their evidence"),
        "{}",
        progress.progress_message
    );
    let activity = progress.activity.unwrap();
    assert_eq!(activity.checks_total, 170);
    assert_eq!(
        activity.checks_reused, 167,
        "Late saved checks must be visible before the slow first request finishes"
    );
    assert_eq!(activity.checks_completed, 167);
    assert_eq!(
        *blocked.batch_sizes.lock().unwrap(),
        vec![3],
        "Pending claims of one section share one request"
    );
    drop(repo);
    pool.close().await;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    JobStore::new(pool.clone())
        .recover(LESSON_PREPARATION, RecoveryPolicy::Requeue)
        .await?;
    assert!(JobStore::new(pool.clone()).claim(&job).await?.is_some());
    let resumed = BatchModel::default();
    lesson_drafts::run(&Database::from(pool.clone()), &repo, &job, &lesson, async {
        let reads = paged(&resumed, &references, &inventory(170), &measured()).await?;
        assert_eq!(reads.len(), 170);
        assert_eq!(reads.iter().filter(|read| read.reused()).count(), 167);
        assert!(reads
            .iter()
            .all(|read| read.finding().verdict == ClaimVerdict::Supported));
        Ok(())
    })
    .await?;
    assert_eq!(*resumed.batch_sizes.lock().unwrap(), vec![3]);
    assert_eq!(*resumed.calls.lock().unwrap(), vec!["batch-evidence"]);
    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn completed_batch_siblings_survive_interruption_during_individual_fallback() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let options = crate::test_support::file_options(dir.path().join("batch.sqlite"));
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(options.clone())
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    sqlx::migrate!("../lattice-desktop/migrations")
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
    let claim = &input.units[0].claims[7];
    let evidence = evidence_for(&model, &claim.statement, 0, &references, &[]).await?;
    let key = claim_receipt_key(&model, &ClaimChecks::key(0, claim, &evidence));
    let retained = measured();
    crate::lesson_progress::run(&JobContext::detached(&JobStore::new(pool.clone()), &job).await?, lesson_drafts::run(&Database::from(pool.clone()), &repo,&job,&lesson,async {
        crate::lesson_progress::begin_checks(8);
        let work=driven(&model,&references,&input,&retained);
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
    assert_eq!(activity.checks_reused, 0);
    drop(repo);
    pool.close().await;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    JobStore::new(pool.clone())
        .recover(LESSON_PREPARATION, RecoveryPolicy::Requeue)
        .await?;
    assert!(JobStore::new(pool.clone()).claim(&job).await?.is_some());
    let model = BatchModel::default();
    lesson_drafts::run(&Database::from(pool.clone()), &repo, &job, &lesson, async {
        let reads = paged(&model, &references, &inventory(8), &measured()).await?;
        assert!(reads
            .iter()
            .all(|read| read.finding().verdict == ClaimVerdict::Supported));
        assert_eq!(*model.calls.lock().unwrap(), vec!["individual-evidence"]);
        Ok(())
    })
    .await?;
    pool.close().await;
    Ok(())
}

/// `reading` over a batch whose first statement waits on a model that never
/// answers, and whose other two have receipts saved under the earlier key: a
/// reader restores one by saving it again under the current key, which it must
/// have done for both, and told of both, by the time the first call is made.
async fn restores_the_receipts_behind_a_slow_call(reading: Reading) -> Result<()> {
    let pool = crate::tests::pool().await?;
    let (repo, job, lesson) = setup(&pool).await?;
    let sources = super::bridge_tests::sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let citations = citing(&references)?;
    let mut page = citations.of(0).to_vec();
    canonicalize_evidence(&mut page);
    let world = world_of(&references, citations);
    let blocked = BatchModel {
        block_batch: true,
        block_individual: true,
        ..Default::default()
    };
    let slow = claim("Quillet is a lexer that turns characters into tokens.");
    let saved = [
        claim("Quillet is a lexer that reads characters."),
        claim("Quillet is a lexer that emits tokens."),
    ];
    lesson_drafts::run(&Database::from(pool.clone()), &repo, &job, &lesson, async {
        for one in &saved {
            let receipt = ClaimReceipt {
                verdict: ClaimVerdict::Supported,
                reason: "The earlier key's receipt.".into(),
                supporting_quote: Some(STATED.into()),
                shown: None,
                parts: Vec::new(),
            };
            let compared = ClaimChecks::key(0, one, &page);
            lesson_drafts::record_checkpoint(
                &claim_receipt_key(&blocked, &format!("{compared}|guard:rule-layer")),
                serde_json::to_value(&receipt)?,
            )
            .await?;
        }
        let claims = [(0, slow), (0, saved[0].clone()), (0, saved[1].clone())];
        let told = Mutex::new(Vec::new());
        let record = |ordinal: usize, _: &Read| told.lock().unwrap().push(ordinal);
        let work = read_observed(reading, &blocked, &world, &claims, &record);
        tokio::select! {
            result = work => panic!("The slow call should still be waiting: {result:?}"),
            result = tokio::time::timeout(Duration::from_secs(5), async {
                while blocked.calls.lock().unwrap().is_empty() {
                    tokio::task::yield_now().await;
                }
            }) => result.expect("The slow call never started"),
        }
        for one in &saved {
            let compared = ClaimChecks::key(0, one, &page);
            let key = claim_receipt_key(&blocked, &format!("{compared}|guard:rule-layer"));
            assert!(
                lesson_drafts::checkpoint(&key).await?.is_some(),
                "A receipt behind a slow call must be restored before the call is made"
            );
        }
        assert_eq!(
            *told.lock().unwrap(),
            vec![1, 2],
            "The receipts behind a slow call are told of before it returns"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn parts_restores_the_receipts_behind_a_slow_call_before_making_the_call() -> Result<()> {
    restores_the_receipts_behind_a_slow_call(Reading::Parts).await
}

#[tokio::test]
async fn ground_restores_the_receipts_behind_a_slow_call_before_making_the_call() -> Result<()> {
    restores_the_receipts_behind_a_slow_call(Reading::Ground).await
}

#[tokio::test]
async fn a_page_batch_restores_the_pages_it_has_saved_before_it_asks_what_an_answer_costs(
) -> Result<()> {
    let pool = crate::tests::pool().await?;
    let (repo, job, lesson) = setup(&pool).await?;
    let sources = vec![source()];
    let references = ReferenceCollection::lexical(&sources)?;
    // The first statement has no page saved; the others do, and a receipt each
    // under the earlier key.
    let everything = inventory(3);
    let mut saved = everything.clone();
    saved.units[0].claims.remove(0);
    let blocked = BatchModel {
        block_batch: true,
        block_individual: true,
        ..Default::default()
    };
    lesson_drafts::run(&Database::from(pool.clone()), &repo, &job, &lesson, async {
        // A run that read their pages and went down before it asked about them.
        let down = BatchModel {
            batch_error: Some(AppError::Network("down".into())),
            ..Default::default()
        };
        let failed = paged(&down, &references, &saved, &measured()).await;
        assert!(matches!(failed, Err(AppError::Network(_))));
        for one in &saved.units[0].claims {
            let page = evidence_for(&blocked, &one.statement, 0, &references, &[]).await?;
            let receipt = ClaimReceipt {
                verdict: ClaimVerdict::Supported,
                reason: "The earlier key's receipt.".into(),
                supporting_quote: page.first().map(|passage| passage.text.clone()),
                shown: None,
                parts: Vec::new(),
            };
            lesson_drafts::record_checkpoint(
                &claim_receipt_key(&blocked, &ClaimChecks::key(0, one, &page)),
                serde_json::to_value(&receipt)?,
            )
            .await?;
        }
        // No answer is measured: the first statement's page is read now, and
        // the request about it is alone.
        let retained = ClaimChecks::default();
        let work = paged(&blocked, &references, &everything, &retained);
        tokio::select! {
            result = work => panic!("The first request should still be waiting: {result:?}"),
            result = tokio::time::timeout(Duration::from_secs(5), async {
                while blocked.calls.lock().unwrap().is_empty() {
                    tokio::task::yield_now().await;
                }
            }) => result.expect("The first request never started"),
        }
        for one in &saved.units[0].claims {
            let page = evidence_for(&blocked, &one.statement, 0, &references, &[]).await?;
            let key = claim_receipt_key(&blocked, &ClaimChecks::key(0, one, &page));
            assert!(
                lesson_drafts::checkpoint(&key).await?.is_some(),
                "A saved page's receipt must be restored before the request that measures an answer"
            );
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_statement_parts_hands_to_the_page_is_counted_once_after_its_page_reading() -> Result<()>
{
    let pool = crate::tests::pool().await?;
    let (repo, job, lesson) = setup(&pool).await?;
    let sources = vec![source()];
    let references = ReferenceCollection::lexical(&sources)?;
    // The first and third statements have page receipts; the second is asked.
    let mut saved = inventory(3);
    let asked = saved.units[0].claims[1].statement.clone();
    saved.units[0].claims.retain(|one| one.statement != asked);
    lesson_drafts::run(&Database::from(pool.clone()), &repo, &job, &lesson, async {
        paged(&BatchModel::default(), &references, &saved, &measured()).await?;
        let model = BatchModel::default();
        let (steps, trace) = crate::lesson_progress::traced(async {
            crate::lesson_progress::begin_checks(3);
            driven(&model, &references, &inventory(3), &measured()).await
        })
        .await;
        assert_eq!(steps?.len(), 3);
        assert_eq!(
            (trace.checked, trace.reused),
            (3, 2),
            "Each statement counts once, reused when its reading came from a saved check"
        );
        assert_eq!(model.calls.lock().unwrap().len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn claims_of_two_sections_never_share_a_request() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let (repo, job, lesson) = setup(&pool).await?;
    let sources = vec![source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let model = BatchModel::default();
    lesson_drafts::run(&Database::from(pool.clone()), &repo, &job, &lesson, async {
        let reads = paged(&model, &references, &sections(2, 3), &measured()).await?;
        assert_eq!(reads.len(), 6);
        assert_eq!(*model.batch_sizes.lock().unwrap(), vec![3, 3]);
        Ok(())
    })
    .await?;
    Ok(())
}

/// The comparisons `pack` puts in each request, as `(ordinal, pass)`, for
/// comparisons given as `(ordinal, unit, statement, pass, page)`.
fn packed(
    llm: &dyn LLMPort,
    comparisons: Vec<(usize, usize, String, Pass, Vec<EvidencePassage>)>,
) -> Vec<Vec<(usize, Pass)>> {
    let pending = comparisons
        .into_iter()
        .map(
            |(ordinal, unit, statement, pass, evidence)| evidence_checks::PendingClaim {
                ordinal,
                key: String::new(),
                receipt_key: String::new(),
                unit,
                claim: Claim {
                    quote: statement.clone(),
                    statement: statement.clone(),
                },
                checked: statement,
                evidence,
                pass,
            },
        )
        .collect();
    evidence_checks::pack(llm, pending, &evidence_checks::Labels::new())
        .into_iter()
        .map(|request| {
            request
                .iter()
                .map(|item| (item.ordinal, item.pass))
                .collect()
        })
        .collect()
}

#[test]
fn claims_are_packed_by_section_with_both_comparisons_together() {
    use lattice_core::application::services::claim_verification::{ClaimChecker, LocatedClaim};
    let model = BatchModel::default();
    // Pages of about ten thousand tokens each, each text its own, so a
    // request holds a few claims; one page no request can hold.
    let page = |claim: usize, pass: usize, chars: usize| {
        vec![EvidencePassage {
            source_id: format!("source-{claim}-{pass}"),
            text: format!("{claim} {pass} {}", "x".repeat(chars)),
            start_byte: 0,
            end_byte: chars,
            retrieval_kind: "lexical".into(),
            score: 0.0,
        }]
    };
    // Sections interleaved in claim order; every claim has the course's first
    // page, and the even ones a comparison with their own citation too.
    let mut comparisons = Vec::new();
    for claim in 0..12 {
        let chars = if claim == 5 { 400_000 } else { 20_000 };
        let statement = format!("Claim {claim}.");
        comparisons.push((
            claim,
            claim % 2,
            statement.clone(),
            Pass::Corroboration,
            page(claim, 1, chars),
        ));
        if claim % 2 == 0 {
            comparisons.push((
                claim,
                claim % 2,
                statement,
                Pass::Attribution,
                page(claim, 0, chars),
            ));
        }
    }
    let requests = packed(&model, comparisons.clone());
    assert_eq!(
        requests,
        packed(&model, comparisons.clone()),
        "the same pages pack the same way, so a split is found again"
    );
    let find = |(ordinal, pass): (usize, Pass)| {
        comparisons
            .iter()
            .find(|comparison| comparison.0 == ordinal && comparison.3 == pass)
            .unwrap()
    };
    let checker = ClaimChecker::for_policy(
        &model,
        SamplingOverride::deterministic(),
        CheckPolicy::Strict,
    );
    let targets = |request: &[(usize, Pass)]| -> Vec<LocatedClaim> {
        request
            .iter()
            .map(|item| {
                let comparison = find(*item);
                LocatedClaim {
                    claim: comparison.2.clone(),
                    passages: comparison.4.iter().map(|p| p.text.clone()).collect(),
                }
            })
            .collect()
    };
    assert!(
        requests.len() > 2,
        "a section's pages need more than one request"
    );
    for (at, request) in requests.iter().enumerate() {
        let unit = find(request[0]).1;
        assert!(request.iter().all(|item| find(*item).1 == unit));
        // A claim's comparisons travel together, its citation's first.
        for item in request {
            let together: Vec<_> = request.iter().filter(|other| other.0 == item.0).collect();
            let all = comparisons.iter().filter(|c| c.0 == item.0).count();
            assert_eq!(together.len(), all, "claim {} was split", item.0);
        }
        assert!(request.windows(2).all(|pair| pair[0] < pair[1]));
        if request.len() > 1 && request.iter().any(|item| item.0 != request[0].0) {
            assert!(checker.batch_fits(&targets(request)));
        }
        // The next claim of the section did not fit beside these.
        if let Some(next) = requests.get(at + 1).filter(|next| find(next[0]).1 == unit) {
            let mut grown = request.clone();
            grown.extend(next.iter().filter(|item| item.0 == next[0].0));
            assert!(!checker.batch_fits(&targets(&grown)));
        }
    }
    // The page no request holds goes alone, where its least needed passages
    // give way.
    assert!(requests.contains(&vec![(5, Pass::Corroboration)]));
}

#[tokio::test]
async fn a_statement_with_no_passages_is_derived_not_restored() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let (repo, job, lesson) = setup(&pool).await?;
    let sources: Vec<LearningSourceDto> = Vec::new();
    let references = ReferenceCollection::lexical(&sources)?;
    let model = BatchModel::default();
    let retained = measured();
    lesson_drafts::run(&Database::from(pool.clone()), &repo, &job, &lesson, async {
        let first = paged(&model, &references, &inventory(1), &retained).await?;
        assert!(first.iter().all(|read| !read.reused()));
        let second = paged(&model, &references, &inventory(1), &retained).await?;
        assert_eq!(second.len(), 1);
        assert!(
            second.iter().all(|read| !read.reused()),
            "a statement with no passage is derived on every read, never restored from a receipt"
        );
        assert!(model.calls.lock().unwrap().is_empty());
        Ok(())
    })
    .await
}
