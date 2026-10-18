//! The verify-and-repair loop stops when repairs stop making progress.
use super::*;
use crate::features::learning::lesson_drafts;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// An unsupported figure no reference gives, so it stays a lesson issue.
fn figure(n: usize) -> String {
    format!("DictReader buffers {}00 rows before parsing.", n + 1)
}

/// Every repair trades the section's one defect for a new unsupported figure.
struct GrowingDefects {
    inner: Model,
    /// Offsets the figures, so a resumed run writes text the draft lacks.
    start: usize,
    repairs: AtomicUsize,
    escalated: AtomicUsize,
    /// Fail the escalated repair, as an interrupted job would.
    interrupt_escalation: AtomicBool,
}

impl GrowingDefects {
    fn new(start: usize) -> Self {
        Self {
            inner: Model::new(),
            start,
            repairs: AtomicUsize::new(0),
            escalated: AtomicUsize::new(0),
            interrupt_escalation: AtomicBool::new(false),
        }
    }

    async fn respond(&self, prompt: &str) -> Result<String> {
        if prompt.starts_with("Extract lesson claims.") {
            let data = self::context(prompt);
            let passage = data["units"][0]["passages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["field"] == "/body")
                .unwrap();
            let text = passage["text"].as_str().unwrap();
            let statement = text
                .find("DictReader buffers ")
                .map(|start| {
                    let rest = &text[start..];
                    rest[..=rest.find('.').unwrap()].to_owned()
                })
                .unwrap_or_else(|| BAD.to_owned());
            return Ok(inventory::fixture_response(vec![
                json!({"index":0,"claims":[{"passageId":passage["id"],"statement":statement}],"nonFactualReason":""}),
            ])
            .to_string());
        }
        if prompt.starts_with("Update claims after a lesson edit.") {
            // The repaired section asserts its new figure in place of the old claim.
            let data = self::context(prompt);
            let passages = data["revisedSection"]["passages"].as_array().unwrap();
            let passage = passages
                .iter()
                .find(|p| p["text"].as_str().unwrap().contains("DictReader buffers "))
                .unwrap();
            let text = passage["text"].as_str().unwrap();
            let start = text.find("DictReader buffers ").unwrap();
            let statement = &text[start..=start + text[start..].find('.').unwrap()];
            let changes: Vec<_> = data["existingClaims"]
                .as_array()
                .unwrap()
                .iter()
                .map(|claim| json!({"claimId":claim["claimId"],"passageId":passage["id"],"statement":statement}))
                .collect();
            return Ok(json!({"changes":changes,"nonFactualReason":""}).to_string());
        }
        if prompt.starts_with("Repair verified lesson defects.") {
            let data = self::context(prompt);
            if data["repeatedDefects"].as_bool() == Some(true) {
                self.escalated.fetch_add(1, Ordering::Relaxed);
                if self.interrupt_escalation.load(Ordering::Relaxed) {
                    return Err(AppError::ServiceNotAvailable("Interrupted".into()));
                }
            }
            let repair = self.repairs.fetch_add(1, Ordering::Relaxed);
            assert!(repair < 4, "repairs must stop when they stop helping");
            let body = candidate(&figure(self.start + repair))["blocks"][0]["body"].clone();
            return Ok(repair_body(prompt, body));
        }
        if prompt.starts_with("Source passages:")
            && prompt
                .split("\n\nClaim: ")
                .nth(1)
                .is_some_and(|claim| claim.starts_with("DictReader buffers "))
        {
            return Ok("unsupported\nReason: The reference does not state a buffer size.\nSource passage: none".into());
        }
        self.inner.respond(prompt).await
    }
}

#[async_trait::async_trait]
impl LLMPort for GrowingDefects {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let mut replies = Vec::new();
        for prompt in fixture_prompts(request) {
            replies.push(self.respond(&prompt).await?);
        }
        Ok(fixture_completion(request, replies))
    }
    fn model_name(&self) -> &str {
        "growing-defects-fixture"
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    fn max_context_tokens(&self) -> usize {
        128_000
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

async fn repair(
    model: &GrowingDefects,
    raw: String,
    references: &ReferenceCollection<'_>,
) -> Result<(String, LessonVerificationReport)> {
    verify_and_repair_with_references(
        model,
        &crate::features::learning::embedded_runtime::BuiltinRuntimes::for_tests(),
        "{}",
        &json!({}),
        raw,
        references,
        3000,
    )
    .await
}

fn stopped(error: &AppError) -> bool {
    error.to_string().contains(
        "Repairs stopped reducing the lesson's problems: 1 remain (Section 1: DictReader strips field-name whitespace by default.",
    ) && error.to_string().contains("The best draft is saved; retry to continue.")
}

#[tokio::test]
async fn repairs_that_trade_one_defect_for_another_stop_after_an_escalated_round() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson) = batch_scheduler_tests::setup(&pool).await?;
    let model = GrowingDefects::new(0);
    let sources = [source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let best = candidate(BAD).to_string();
    lesson_drafts::run(&repo, &job, &lesson, async {
        lesson_drafts::resume("progress-fixture".into()).await?;
        lesson_drafts::save(&best).await?;
        let error = repair(&model, best.clone(), &references)
            .await
            .err()
            .unwrap();
        assert!(stopped(&error), "{error}");
        // One ordinary repair, then one escalated repair, then the stop.
        assert_eq!(model.repairs.load(Ordering::Relaxed), 2);
        assert_eq!(model.escalated.load(Ordering::Relaxed), 1);
        // The saved draft is the best candidate, not the last repair.
        let saved = lesson_drafts::resume("progress-fixture".into())
            .await?
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&saved)?,
            serde_json::from_str::<Value>(&best)?
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_resume_during_an_escalation_keeps_it() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson) = batch_scheduler_tests::setup(&pool).await?;
    let sources = [source()];
    let references = ReferenceCollection::lexical(&sources)?;
    lesson_drafts::run(&repo, &job, &lesson, async {
        lesson_drafts::resume("progress-fixture".into()).await?;
        lesson_drafts::save(&candidate(BAD).to_string()).await?;
        // The escalated repair is interrupted after its pending repair is saved.
        let interrupted = GrowingDefects::new(0);
        interrupted
            .interrupt_escalation
            .store(true, Ordering::Relaxed);
        let error = repair(&interrupted, candidate(BAD).to_string(), &references)
            .await
            .err()
            .unwrap();
        assert!(error.to_string().contains("Interrupted"), "{error}");
        assert_eq!(interrupted.repairs.load(Ordering::Relaxed), 1);
        // The resumed job finishes the escalated repair; when it does not
        // beat the best, the loop stops instead of escalating again.
        let draft = lesson_drafts::resume("progress-fixture".into())
            .await?
            .unwrap();
        let resumed = GrowingDefects::new(5);
        let error = repair(&resumed, draft, &references).await.err().unwrap();
        assert!(stopped(&error), "{error}");
        assert_eq!(resumed.repairs.load(Ordering::Relaxed), 1);
        assert_eq!(resumed.escalated.load(Ordering::Relaxed), 1);
        Ok(())
    })
    .await
}
