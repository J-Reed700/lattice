use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

struct DisconnectDuringFidelity {
    inner: Model,
    pool: sqlx::SqlitePool,
    mapping_calls: AtomicUsize,
    fidelity_calls: AtomicUsize,
}

#[async_trait::async_trait]
impl LLMPort for DisconnectDuringFidelity {
    async fn generate(
        &self,
        prompt: &str,
        context: &[String],
        images: Option<Vec<String>>,
    ) -> Result<String> {
        if prompt.starts_with("Audit claim coverage independently.") {
            self.mapping_calls.fetch_add(1, Ordering::Relaxed);
        }
        if prompt.starts_with("Audit claim fidelity.")
            && self.fidelity_calls.fetch_add(1, Ordering::Relaxed) == 1
        {
            // Disconnect only after the other concurrent passage has committed
            // its result. This deterministically exercises an unfinished batch.
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let saved = sqlx::query_scalar::<_, i64>(
                        "SELECT COUNT(*) FROM learning_generation_checkpoints WHERE checkpoint_key LIKE 'coverage-fidelity-v1:%'",
                    )
                    .fetch_one(&self.pool)
                    .await
                    .unwrap();
                    if saved > 0 {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("A completed passage must be saved before its batch finishes");
            return Err(AppError::ServiceNotAvailable("fixture disconnect".into()));
        }
        self.inner.generate(prompt, context, images).await
    }

    async fn generate_streaming(
        &self,
        _: &str,
        _: &[String],
        _: Option<Vec<String>>,
    ) -> Result<Box<dyn futures::Stream<Item = Result<String>> + Send + Unpin + '_>> {
        Err(invalid("unused fixture stream"))
    }
    fn model_name(&self) -> &str {
        self.inner.model_name()
    }
    fn count_tokens(&self, text: &str) -> usize {
        self.inner.count_tokens(text)
    }
    fn max_context_tokens(&self) -> usize {
        self.inner.max_context_tokens()
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn interrupted_coverage_reuses_mapping_and_each_completed_passage() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson) = batch_scheduler_tests::setup(&pool).await?;
    let model = DisconnectDuringFidelity {
        inner: Model::new(),
        pool: pool.clone(),
        mapping_calls: AtomicUsize::new(0),
        fidelity_calls: AtomicUsize::new(0),
    };
    let mut content = vec![
        json!({"kind":"teaching","body":GOOD}),
        json!({"kind":"teaching","body":GOOD}),
    ];
    let inventory = Inventory {
        units: (0..2)
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
    crate::features::learning::lesson_drafts::run(&repo, &job, &lesson, async {
        assert!(coverage::audit(&model, &content, &inventory, None)
            .await
            .is_err());
        assert_eq!(model.mapping_calls.load(Ordering::Relaxed), 1);
        assert_eq!(model.fidelity_calls.load(Ordering::Relaxed), 2);
        Ok(())
    })
    .await?;
    // A new task-local context must recover the persisted results, not memory.
    crate::features::learning::lesson_drafts::run(&repo, &job, &lesson, async {
        let audited = coverage::audit(&model, &content, &inventory, None).await?;
        assert!(audited.units.iter().all(|unit| unit.complete));
        assert_eq!(model.mapping_calls.load(Ordering::Relaxed), 1);
        assert_eq!(model.fidelity_calls.load(Ordering::Relaxed), 3);
        coverage::audit(&model, &content, &inventory, None).await?;
        assert_eq!(model.fidelity_calls.load(Ordering::Relaxed), 3);
        content[1]["body"] = json!(format!("{GOOD} Follow these practice instructions."));
        coverage::audit(&model, &content, &inventory, None).await?;
        assert_eq!(model.mapping_calls.load(Ordering::Relaxed), 2);
        assert_eq!(model.fidelity_calls.load(Ordering::Relaxed), 4);
        Ok(())
    })
    .await
}
