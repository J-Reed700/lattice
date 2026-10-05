#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
use super::*;
use crate::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use std::sync::Mutex;
use std::time::Duration;
use tokio::time::Instant;

const BAD: &str = "DictReader strips field-name whitespace by default.";
const GOOD: &str = "DictReader preserves field-name whitespace by default.";
const SOURCE: &str = "DictReader preserves field-name whitespace by default. Missing values in nonblank rows default to None. The optional skipinitialspace setting ignores spaces immediately after delimiters; it defaults to false.";

/// A deterministic protocol fixture, not evidence of a live model's accuracy.
/// Execution behavior is covered separately with real bundled guests.
struct Model {
    repaired: Mutex<bool>,
    judge_calls: Mutex<usize>,
    failing_judge: bool,
    incomplete_coverage: bool,
}
impl Model {
    fn new() -> Self {
        Self {
            repaired: Mutex::new(false),
            judge_calls: Mutex::new(0),
            failing_judge: false,
            incomplete_coverage: false,
        }
    }
}
fn context(prompt: &str) -> Value {
    serde_json::from_str(prompt.rsplit("\n\n").next().unwrap()).unwrap()
}
/// Shared with course fixtures so they exercise the additional protocol calls.
pub(crate) fn fixture_response(prompt: &str) -> Option<String> {
    if prompt.starts_with("Audit claim fidelity.") {
        return Some("supported\nReason: The fixture inventory faithfully represents the original assertions.\nSource passage: passage-0".into());
    }
    if prompt.starts_with("Extract lesson claims.") {
        let data = context(prompt);
        let units = data["units"].as_array().unwrap();
        return Some(json!({"units":units.iter().map(|v| {
            let passages = v["passages"].as_array().unwrap();
            let passage = passages.iter().find(|p|p["field"]=="/body" || p["field"]=="/explanation").unwrap();
            let quote = passage["text"].as_str().unwrap();
            let index = v["index"].as_u64().unwrap() as usize;
            let statement = if quote.contains(BAD) { BAD } else if quote.contains(GOOD) { GOOD } else { quote };
            json!({"index":index,"claims":[{"passageId":passage["id"],"statement":statement}],"nonFactualReason":""})
        }).collect::<Vec<_>>()} ).to_string());
    }
    if prompt.starts_with("Audit claim coverage independently.") {
        let data = context(prompt);
        let mut checks = serde_json::Map::new();
        for unit in data["units"].as_array().unwrap() {
            let ids: Vec<_> = unit["claims"]
                .as_array()
                .unwrap()
                .iter()
                .map(|claim| claim["id"].clone())
                .collect();
            for passage in unit["passages"].as_array().unwrap() {
                checks.insert(passage["id"].as_str().unwrap().into(), json!({"claimIds":ids,"nonFactualReason":if ids.is_empty() {"The fixture contains only a stipulated instruction."} else {""},"missingClaims":[]}));
            }
        }
        return Some(Value::Object(checks).to_string());
    }
    None
}
#[async_trait::async_trait]
impl LLMPort for Model {
    async fn generate(&self, prompt: &str, _: &[String], _: Option<Vec<String>>) -> Result<String> {
        if prompt.starts_with("Audit claim coverage independently.") && self.incomplete_coverage {
            return Ok(json!({"units":[]}).to_string());
        }
        if let Some(response) = fixture_response(prompt) {
            return Ok(response);
        }
        if prompt.starts_with("Repair verified lesson defects.") {
            assert!(prompt.contains(GOOD));
            *self.repaired.lock().unwrap() = true;
            return Ok(candidate(GOOD).to_string());
        }
        if prompt.starts_with("Review instructional quality.") {
            let data = context(prompt);
            return Ok(json!({"issues":[],"blockChecks":data["candidate"]["blocks"].as_array().unwrap().iter().enumerate().map(|(index,block)|json!({"index":index,"passageId":block["bodyPassages"][0]["id"],"finding":"The corrected field-name statement agrees with the concrete example.","hasDefect":false})).collect::<Vec<_>>()} ).to_string());
        }
        if prompt.starts_with("Source passages:") {
            *self.judge_calls.lock().unwrap() += 1;
            if self.failing_judge {
                return Err(invalid("fixture offline"));
            }
            let bad = prompt.split("\n\nClaim: ").nth(1).unwrap().starts_with(BAD);
            return Ok(format!("{}\nReason: The captured reference and runtime preserve the header spaces.\nSource quote: {GOOD}\nSource passage: passage-0", if bad { "contradicted" } else { "supported" }));
        }
        Err(invalid(format!(
            "Unexpected fixture prompt: {}",
            prompt.chars().take(100).collect::<String>()
        )))
    }
    async fn generate_streaming(
        &self,
        _: &str,
        _: &[String],
        _: Option<Vec<String>>,
    ) -> Result<Box<dyn futures::Stream<Item = Result<String>> + Send + Unpin + '_>> {
        Err(invalid("unused"))
    }
    fn model_name(&self) -> &str {
        "scripted-evidence-checker"
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
fn candidate(statement: &str) -> Value {
    json!({"blocks":[{"kind":"explanation","title":"CSV header names","body":format!("{} ", statement).repeat(24),"rubric":[]}],"questions":[]})
}
fn source() -> LearningSourceDto {
    LearningSourceDto {
        id: uuid::Uuid::new_v4().to_string(),
        title: "CSV reference fixture".into(),
        url: Some("https://docs.python.org/3.14/library/csv.html".into()),
        excerpt: SOURCE.into(),
        acquired_at: 0,
    }
}

#[tokio::test]
async fn unchanged_evidence_reuses_judgments_but_new_evidence_and_failed_calls_do_not() -> Result<()>
{
    let mut model = Model::new();
    let lesson = candidate(GOOD);
    let mut sources = vec![source()];
    let mut checks = ClaimChecks::default();
    let first = verify_with_references(
        &model,
        &lesson,
        &ReferenceCollection::lexical(&sources)?,
        &mut checks,
    )
    .await?;
    assert!(first.issues.is_empty());
    assert_eq!(*model.judge_calls.lock().unwrap(), 1);

    let mut unrelated = source();
    unrelated.excerpt = "Galaxies orbit stars.".into();
    sources.push(unrelated);
    let expanded = verify_with_references(
        &model,
        &lesson,
        &ReferenceCollection::lexical(&sources)?,
        &mut checks,
    )
    .await?;
    assert_eq!(expanded.sources.len(), 2);
    assert!(expanded.issues.is_empty());
    assert_eq!(*model.judge_calls.lock().unwrap(), 1, "Adding an unrelated source should still retrieve current evidence without repeating an identical judgment");

    let mut changed = source();
    changed.excerpt = BAD.into();
    sources.push(changed);
    model.failing_judge = true;
    let failed = verify_with_references(
        &model,
        &lesson,
        &ReferenceCollection::lexical(&sources)?,
        &mut checks,
    )
    .await?;
    assert_eq!(failed.findings[0].verdict, ClaimVerdict::Unverified);
    assert!(
        !failed.issues.is_empty(),
        "New evidence cannot inherit an earlier approval"
    );
    assert_eq!(*model.judge_calls.lock().unwrap(), 2);
    model.failing_judge = false;
    verify_with_references(
        &model,
        &lesson,
        &ReferenceCollection::lexical(&sources)?,
        &mut checks,
    )
    .await?;
    assert_eq!(
        *model.judge_calls.lock().unwrap(),
        3,
        "Failed checks must be attempted again"
    );

    let edited = verify_with_references(
        &model,
        &candidate(BAD),
        &ReferenceCollection::lexical(&sources)?,
        &mut checks,
    )
    .await?;
    assert_eq!(edited.findings[0].verdict, ClaimVerdict::Contradicted);
    assert_eq!(*model.judge_calls.lock().unwrap(), 4);
    Ok(())
}

#[tokio::test]
async fn repairing_one_section_preserves_only_unchanged_claim_comparisons() -> Result<()> {
    let model = Model::new();
    let sources = vec![source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let mut lesson = candidate(GOOD);
    lesson["blocks"]
        .as_array_mut()
        .unwrap()
        .push(candidate(BAD)["blocks"][0].clone());
    let mut checks = ClaimChecks::default();
    let first = verify_with_references(&model, &lesson, &references, &mut checks).await?;
    assert_eq!(*model.judge_calls.lock().unwrap(), 2);
    assert_eq!(first.findings[1].verdict, ClaimVerdict::Contradicted);
    lesson["blocks"][1] = candidate(GOOD)["blocks"][0].clone();
    let repaired = verify_with_references(&model, &lesson, &references, &mut checks).await?;
    assert!(repaired.issues.is_empty());
    assert_eq!(
        *model.judge_calls.lock().unwrap(),
        3,
        "Repairing one section must not repeat the other section's identical comparison"
    );
    // A changed original passage invalidates the comparison even when this
    // fixture's extractor returns the same standalone factual statement.
    lesson["blocks"][0]["body"] = json!(format!(
        "{} Additional wording.",
        lesson["blocks"][0]["body"].as_str().unwrap()
    ));
    verify_with_references(&model, &lesson, &references, &mut checks).await?;
    assert_eq!(*model.judge_calls.lock().unwrap(), 4);
    Ok(())
}

#[tokio::test]
async fn reused_comparisons_keep_current_retrieval_metadata_and_exact_source_identity() -> Result<()>
{
    let model = Model::new();
    let lesson = candidate(GOOD);
    let report = verify(&model, &lesson, &[source()]).await?;
    let finding = &report.findings[0];
    let claim = Claim {
        quote: finding.quote.clone(),
        statement: finding.statement.clone(),
    };
    let key = ClaimChecks::key(finding.unit, &claim, &finding.evidence);
    let mut checks = ClaimChecks::default();
    checks.record(key.clone(), finding);
    let mut evidence = finding.evidence.clone();
    evidence[0].score += 1.0;
    assert_eq!(key, ClaimChecks::key(finding.unit, &claim, &evidence));
    assert_eq!(
        checks.get(&key, &evidence).unwrap().evidence[0].score,
        evidence[0].score
    );
    evidence[0].source_id.push_str("-new-version");
    assert!(checks
        .get(
            &ClaimChecks::key(finding.unit, &claim, &evidence),
            &evidence
        )
        .is_none());
    evidence = finding.evidence.clone();
    evidence[0].text.push(' ');
    assert_ne!(key, ClaimChecks::key(finding.unit, &claim, &evidence));
    assert_ne!(
        key,
        ClaimChecks::key(finding.unit + 1, &claim, &finding.evidence)
    );
    assert!(
        ClaimChecks::default()
            .get(&key, &finding.evidence)
            .is_none(),
        "Judgments are not inherited across preparation operations"
    );
    Ok(())
}

#[test]
fn pending_repairs_require_identical_content_sources_and_instructions() -> Result<()> {
    let model = Model::new();
    let sources = vec![source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let original = candidate(BAD);
    let schema = json!({"type":"object"});
    let pending = PendingRepair {
        fingerprint: repair_fingerprint(&model, "instructions", &schema, &original, &references),
        defects: 1,
        context: json!({"candidate":original,"failedClaims":[{"statement":BAD}]}),
    };
    let restored: PendingRepair = serde_json::from_str(&serde_json::to_string(&pending)?)?;
    assert!(restored.matches(&model, "instructions", &schema, &original, &references));
    assert!(!restored.matches(
        &model,
        "changed instructions",
        &schema,
        &original,
        &references
    ));
    assert!(!restored.matches(
        &model,
        "instructions",
        &schema,
        &candidate(GOOD),
        &references
    ));
    assert!(!restored.matches(
        &model,
        "instructions",
        &json!({"type":"array"}),
        &original,
        &references
    ));
    let mut changed = sources.clone();
    changed[0].excerpt.push_str(" Changed evidence.");
    assert!(!restored.matches(
        &model,
        "instructions",
        &schema,
        &original,
        &ReferenceCollection::lexical(&changed)?
    ));
    let mut expanded = sources.clone();
    expanded.push(source());
    assert!(!restored.matches(
        &model,
        "instructions",
        &schema,
        &original,
        &ReferenceCollection::lexical(&expanded)?
    ));
    let mut approved = restored;
    approved.defects = 0;
    assert!(!approved.matches(&model, "instructions", &schema, &original, &references));
    Ok(())
}

#[tokio::test]
async fn failed_rewrite_resumes_its_checkpoint_and_checks_the_new_candidate() -> Result<()> {
    use crate::features::learning::{
        curriculum::LearningGenerationJobKind,
        curriculum_repository::LearningCurriculumRepository,
        dto::AcceptLearningProgramRequestDto,
        lesson_drafts,
        plan_dto::{LearningGenerationJobActionRequestDto, StartLearningGenerationJobRequestDto},
        repository::LearningRepository,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct InterruptedRepair {
        model: Model,
        repairs: AtomicUsize,
        bad_checks: AtomicUsize,
        good_checks: AtomicUsize,
    }
    #[async_trait::async_trait]
    impl LLMPort for InterruptedRepair {
        async fn generate(
            &self,
            prompt: &str,
            context: &[String],
            images: Option<Vec<String>>,
        ) -> Result<String> {
            if prompt.starts_with("Repair verified lesson defects.") {
                if self.repairs.fetch_add(1, Ordering::Relaxed) == 1 {
                    return Err(AppError::Network(
                        "fixture server failure during the second section repair".into(),
                    ));
                }
                let mut patch = self::context(prompt)["candidate"].clone();
                for block in patch["blocks"].as_array_mut().unwrap() {
                    block["body"] = json!(block["body"].as_str().unwrap().replace(BAD, GOOD));
                }
                return Ok(patch.to_string());
            }
            if prompt.starts_with("Source passages:") {
                if prompt.split("\n\nClaim: ").nth(1).unwrap().starts_with(BAD) {
                    self.bad_checks.fetch_add(1, Ordering::Relaxed);
                } else {
                    self.good_checks.fetch_add(1, Ordering::Relaxed);
                }
            }
            self.model.generate(prompt, context, images).await
        }
        async fn generate_streaming(
            &self,
            _: &str,
            _: &[String],
            _: Option<Vec<String>>,
        ) -> Result<Box<dyn futures::Stream<Item = Result<String>> + Send + Unpin + '_>> {
            unreachable!()
        }
        fn model_name(&self) -> &str {
            self.model.model_name()
        }
        fn count_tokens(&self, text: &str) -> usize {
            self.model.count_tokens(text)
        }
        fn max_context_tokens(&self) -> usize {
            self.model.max_context_tokens()
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }
    let pool = crate::features::learning::tests::pool().await?;
    let repository = LearningRepository::new(pool.clone());
    let program = crate::features::learning::tests::fixture();
    repository.create(&program).await?;
    repository
        .accept(&AcceptLearningProgramRequestDto {
            program_id: program.summary.id.clone(),
            expected_revision: 0,
            title: program.summary.title.clone(),
        })
        .await?;
    let program = repository.get(&program.summary.id).await?;
    let jobs = LearningCurriculumRepository::new(pool);
    jobs.plan(&program.summary.id).await?;
    let target = &program.modules[0].lessons[0].id;
    let job = jobs
        .start_job(&StartLearningGenerationJobRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: program.summary.id.clone(),
            expected_revision: program.summary.revision,
            kind: LearningGenerationJobKind::LessonPreparation,
            request_json: json!({"lessonIds":[target]}).to_string(),
            progress_total: 1,
        })
        .await?;
    jobs.begin_job(&job.id).await?;
    let model = InterruptedRepair {
        model: Model::new(),
        repairs: AtomicUsize::new(0),
        bad_checks: AtomicUsize::new(0),
        good_checks: AtomicUsize::new(0),
    };
    let sources = vec![source()];
    let mut original = candidate(BAD);
    let mut second = original["blocks"][0].clone();
    second["title"] = json!("A second section needing repair");
    original["blocks"].as_array_mut().unwrap().push(second);
    let raw = original.to_string();
    let first = lesson_drafts::run(&jobs, &job.id, target, async {
        lesson_drafts::resume("same-inputs".into()).await?;
        lesson_drafts::save(&raw).await?;
        verify_and_repair(&model, "{}", &json!({}), raw.clone(), &sources, 4000).await
    })
    .await;
    let error = first.unwrap_err();
    jobs.fail_job(&job.id, &error.to_string()).await?;
    let partial = jobs.lesson_draft(&job.id, target).await?.unwrap();
    assert!(partial.pending_repair.is_some());
    let partial: Value = serde_json::from_str(&partial.candidate)?;
    assert!(partial["blocks"][0]["body"]
        .as_str()
        .unwrap()
        .contains(GOOD));
    assert!(partial["blocks"][1]["body"].as_str().unwrap().contains(BAD));
    let retry = jobs
        .retry_job(&LearningGenerationJobActionRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: program.summary.id.clone(),
            job_id: job.id,
            expected_revision: program.summary.revision,
        })
        .await?;
    jobs.begin_job(&retry.id).await?;
    let (_, report) = lesson_drafts::run(&jobs, &retry.id, target, async {
        let saved = lesson_drafts::resume("same-inputs".into()).await?.unwrap();
        assert!(
            has_pending_repair(
                &model,
                "{}",
                &json!({}),
                &saved,
                &ReferenceCollection::lexical(&sources)?
            )
            .await?
        );
        verify_and_repair(&model, "{}", &json!({}), saved, &sources, 4000).await
    })
    .await?;
    assert_eq!(model.repairs.load(Ordering::Relaxed), 3);
    assert_eq!(
        model.bad_checks.load(Ordering::Relaxed),
        2,
        "Retry must resume the rewrite, not repeat its old diagnosis"
    );
    assert_eq!(
        model.good_checks.load(Ordering::Relaxed),
        2,
        "Rewritten content must undergo fresh evidence checking"
    );
    assert!(report.issues.is_empty());
    assert!(report
        .findings
        .iter()
        .all(|f| f.verdict == ClaimVerdict::Supported));
    Ok(())
}

#[tokio::test]
async fn repairs_continue_with_fewer_defects_and_stop_when_they_stall() -> Result<()> {
    const SECOND: &str = "DictReader removes all whitespace from CSV values by default.";
    struct GradualRepair {
        inner: Model,
        repairs: std::sync::atomic::AtomicUsize,
        stall: bool,
    }
    #[async_trait::async_trait]
    impl LLMPort for GradualRepair {
        async fn generate(
            &self,
            prompt: &str,
            context: &[String],
            images: Option<Vec<String>>,
        ) -> Result<String> {
            if prompt.starts_with("Extract lesson claims.") {
                let data = self::context(prompt);
                let unit = &data["units"][0];
                let passage = unit["passages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|p| p["field"] == "/body")
                    .unwrap();
                let text = passage["text"].as_str().unwrap();
                let claims: Vec<_> = [BAD, SECOND, GOOD]
                    .into_iter()
                    .filter(|s| text.contains(s))
                    .map(|statement| json!({"passageId":passage["id"],"statement":statement}))
                    .collect();
                return Ok(
                    json!({"units":[{"index":0,"claims":claims,"nonFactualReason":""}]})
                        .to_string(),
                );
            }
            if prompt.starts_with("Repair verified lesson defects.") {
                let repair = self
                    .repairs
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                assert!(repair < 2, "No progress must stop automatic repair");
                return Ok(candidate(if repair == 0 || self.stall {
                    SECOND
                } else {
                    GOOD
                })
                .to_string());
            }
            if prompt.starts_with("Source passages:")
                && prompt
                    .split("\n\nClaim: ")
                    .nth(1)
                    .unwrap()
                    .starts_with(SECOND)
            {
                return Ok("contradicted\nReason: The fixture reference preserves whitespace rather than removing it.\nSource passage: passage-0".into());
            }
            self.inner.generate(prompt, context, images).await
        }
        async fn generate_streaming(
            &self,
            _: &str,
            _: &[String],
            _: Option<Vec<String>>,
        ) -> Result<Box<dyn futures::Stream<Item = Result<String>> + Send + Unpin + '_>> {
            unreachable!()
        }
        fn model_name(&self) -> &str {
            "gradual-repair-fixture"
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
    for stall in [false, true] {
        let model = GradualRepair {
            inner: Model::new(),
            repairs: Default::default(),
            stall,
        };
        let result = verify_and_repair(
            &model,
            "{}",
            &json!({}),
            candidate(&format!("{BAD} {SECOND}")).to_string(),
            &[source()],
            3000,
        )
        .await;
        assert_eq!(model.repairs.load(std::sync::atomic::Ordering::Relaxed), 2);
        if stall {
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("stopped making progress"));
        } else {
            let (_, report) = result?;
            assert!(report.issues.is_empty());
            assert!(report
                .findings
                .iter()
                .all(|finding| finding.verdict == ClaimVerdict::Supported));
        }
    }
    Ok(())
}

#[tokio::test]
async fn contradictory_claim_is_repaired_and_rechecked_against_references() -> Result<()> {
    let model = Model::new();
    let bad = verify(&model, &candidate(BAD), &[source()]).await?;
    assert!(!bad.issues.is_empty());
    assert_eq!(bad.findings[0].verdict, ClaimVerdict::Contradicted);
    assert!(bad.executions.is_empty());
    let (raw, report) = verify_and_repair(
        &model,
        "{}",
        &json!({"type":"object"}),
        candidate(BAD).to_string(),
        &[source()],
        3000,
    )
    .await?;
    assert!(*model.repaired.lock().unwrap());
    assert!(raw.contains(GOOD));
    assert!(report.issues.is_empty());
    assert!(report
        .findings
        .iter()
        .all(|f| f.verdict == ClaimVerdict::Supported));
    Ok(())
}

#[tokio::test]
async fn repair_shares_exact_passages_without_dropping_conflicting_evidence() -> Result<()> {
    let model = Model::new();
    let candidate = candidate(BAD);
    let mut report = verify(&model, &candidate, &[source()]).await?;
    let mut finding = report.findings[0].clone();
    let original = finding.evidence[0].clone();
    let mut conflicting = original.clone();
    conflicting.source_id = "independent-conflicting-source".into();
    conflicting.text = "Keep conflicting evidence too.\r\n  Preserve whitespace.".into();
    finding.evidence = vec![original.clone(), conflicting.clone()];
    report.findings = (0..107)
        .map(|index| {
            let mut copy = finding.clone();
            copy.statement = format!("Claim {index}: {BAD}");
            copy
        })
        .collect();
    let payload = repair_context("{}", &candidate, &report)?;
    assert_eq!(payload["failedClaims"].as_array().unwrap().len(), 107);
    assert_eq!(payload["evidence"].as_array().unwrap().len(), 2);
    assert_eq!(payload["evidence"][0]["text"], original.text);
    assert_eq!(payload["evidence"][1]["text"], conflicting.text);
    assert_eq!(payload["evidence"][1]["sourceId"], conflicting.source_id);
    for claim in payload["failedClaims"].as_array().unwrap() {
        assert_eq!(claim["evidence"], json!(["evidence-0", "evidence-1"]));
    }
    assert!(!payload["failedClaims"][0]
        .as_object()
        .unwrap()
        .contains_key("quote"));
    Ok(())
}

#[tokio::test]
async fn missing_claim_coverage_is_corrected_without_rewriting_lesson_content() -> Result<()> {
    struct CoverageModel {
        inner: Model,
        audits: std::sync::atomic::AtomicUsize,
        extracted: Mutex<Vec<Vec<usize>>>,
        audited: Mutex<Vec<Vec<usize>>>,
        resolves: bool,
    }
    #[async_trait::async_trait]
    impl LLMPort for CoverageModel {
        async fn generate(
            &self,
            prompt: &str,
            context_data: &[String],
            history: Option<Vec<String>>,
        ) -> Result<String> {
            if prompt.starts_with("Extract lesson claims.") {
                let data = context(prompt);
                self.extracted.lock().unwrap().push(
                    data["requestedIndices"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_u64().unwrap() as usize)
                        .collect(),
                );
            }
            if prompt.starts_with("Audit claim coverage independently.") {
                self.audited.lock().unwrap().push(
                    context(prompt)["units"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|unit| unit["index"].as_u64().unwrap() as usize)
                        .collect(),
                );
            }
            if prompt.starts_with("Audit claim coverage independently.") {
                let pass = self
                    .audits
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let mut response: Value = serde_json::from_str(&fixture_response(prompt).unwrap())?;
                let remaining = if self.resolves {
                    2_usize.saturating_sub(pass)
                } else {
                    2
                };
                for (_, check) in response
                    .as_object_mut()
                    .unwrap()
                    .iter_mut()
                    .filter(|(id, _)| id.starts_with("unit-0-"))
                    .take(remaining)
                {
                    check["missingClaims"] = json!(["Include the asserted condition explicitly."]);
                }
                return Ok(response.to_string());
            }
            self.inner.generate(prompt, context_data, history).await
        }
        async fn generate_streaming(
            &self,
            _: &str,
            _: &[String],
            _: Option<Vec<String>>,
        ) -> Result<Box<dyn futures::Stream<Item = Result<String>> + Send + Unpin + '_>> {
            Err(invalid("unused"))
        }
        fn model_name(&self) -> &str {
            self.inner.model_name()
        }
        fn max_context_tokens(&self) -> usize {
            self.inner.max_context_tokens()
        }
        fn count_tokens(&self, text: &str) -> usize {
            self.inner.count_tokens(text)
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }
    let model = CoverageModel {
        inner: Model::new(),
        audits: std::sync::atomic::AtomicUsize::new(0),
        extracted: Mutex::new(Vec::new()),
        audited: Mutex::new(Vec::new()),
        resolves: true,
    };
    let mut lesson = candidate(GOOD);
    let block = lesson["blocks"][0].clone();
    lesson["blocks"].as_array_mut().unwrap().push(block);
    let original = lesson.clone();
    let report = verify(&model, &lesson, &[source()]).await?;
    assert!(report.issues.is_empty());
    assert_eq!(lesson, original);
    assert!(!*model.inner.repaired.lock().unwrap());
    assert_eq!(
        *model.extracted.lock().unwrap(),
        vec![vec![0, 1], vec![0], vec![0]]
    );
    assert_eq!(
        *model.audited.lock().unwrap(),
        vec![vec![0, 1], vec![0], vec![0]]
    );
    assert_eq!(model.audits.load(std::sync::atomic::Ordering::Relaxed), 3);

    let stalled = CoverageModel {
        inner: Model::new(),
        audits: std::sync::atomic::AtomicUsize::new(0),
        extracted: Mutex::new(Vec::new()),
        audited: Mutex::new(Vec::new()),
        resolves: false,
    };
    let error = verify(&stalled, &lesson, &[source()]).await.err().unwrap();
    assert!(error.to_string().contains("stopped making progress"));
    assert_eq!(stalled.audits.load(std::sync::atomic::Ordering::Relaxed), 2);
    assert_eq!(*stalled.inner.judge_calls.lock().unwrap(), 0);
    assert_eq!(*stalled.audited.lock().unwrap(), vec![vec![0, 1], vec![0]]);

    // Resume an interrupted audit with one completed section. Only the other
    // section is audited, then corrected until its passage count reaches zero.
    let resume = CoverageModel {
        inner: Model::new(),
        audits: std::sync::atomic::AtomicUsize::new(1),
        extracted: Mutex::new(Vec::new()),
        audited: Mutex::new(Vec::new()),
        resolves: true,
    };
    let content = units(&lesson)?;
    let extracted = inventory::extract(&Model::new(), &content).await?;
    let partial = Coverage {
        units: vec![CoverageUnit {
            index: 1,
            complete: true,
            reason: "Already audited".into(),
            unresolved_passages: Vec::new(),
        }],
    };
    assert!(valid_saved_coverage(&partial, &content));
    let (_, completed) = complete_inventory(
        &resume,
        &content,
        extracted,
        &lesson.to_string(),
        Some(partial),
    )
    .await?;
    assert!(completed.units.iter().all(|unit| unit.complete));
    assert_eq!(*resume.audited.lock().unwrap(), vec![vec![0], vec![0]]);
    assert_eq!(*resume.extracted.lock().unwrap(), vec![vec![0]]);
    Ok(())
}

#[test]
fn saved_coverage_rejects_foreign_duplicate_or_contradictory_locations() {
    let content = units(&candidate(GOOD)).unwrap();
    let valid = CoverageUnit {
        index: 0,
        complete: false,
        reason: "Missing condition".into(),
        unresolved_passages: vec!["unit-0-passage-0".into()],
    };
    assert!(valid_saved_coverage(
        &Coverage {
            units: vec![valid.clone()]
        },
        &content
    ));
    assert!(!valid_saved_coverage(
        &Coverage {
            units: vec![valid.clone(), valid.clone()]
        },
        &content
    ));
    for changed in [
        CoverageUnit {
            index: 1,
            ..valid.clone()
        },
        CoverageUnit {
            complete: true,
            ..valid.clone()
        },
        CoverageUnit {
            unresolved_passages: vec!["unit-1-passage-0".into()],
            ..valid.clone()
        },
        CoverageUnit {
            unresolved_passages: vec!["unit-0-passage-0".into(); 2],
            ..valid
        },
    ] {
        assert!(!valid_saved_coverage(
            &Coverage {
                units: vec![changed]
            },
            &content
        ));
    }
}

#[test]
fn assessment_policy_changes_expire_assessments_without_repeating_identical_teaching_checks() {
    let content = vec![json!({"kind":"teaching"}), json!({"kind":"assessment"})];
    let original = Coverage {
        units: (0..2)
            .map(|index| CoverageUnit {
                index,
                complete: true,
                reason: "Previously checked".into(),
                unresolved_passages: Vec::new(),
            })
            .collect(),
    };
    for policy in [None, Some("previous-assessment-protocol")] {
        let mut saved = original.clone();
        retain_current_assessment_checks(&mut saved, &content, policy);
        assert_eq!(
            saved.units.iter().map(|u| u.index).collect::<Vec<_>>(),
            vec![0]
        );
    }
    let mut current = original;
    retain_current_assessment_checks(&mut current, &content, Some(coverage::ASSESSMENT_POLICY));
    assert_eq!(current.units.len(), 2);
}

#[tokio::test]
async fn missing_evidence_failed_judge_and_incomplete_coverage_never_approve() -> Result<()> {
    let model = Model::new();
    let no_evidence = verify(&model, &candidate("A short factual definition."), &[]).await?;
    assert_eq!(no_evidence.findings[0].verdict, ClaimVerdict::Unsupported);
    assert!(!no_evidence.issues.is_empty());
    let offline = Model {
        failing_judge: true,
        ..Model::new()
    };
    let report = verify(&offline, &candidate(GOOD), &[source()]).await?;
    assert_eq!(report.findings[0].verdict, ClaimVerdict::Unverified);
    assert!(verify_and_repair(
        &offline,
        "{}",
        &json!({}),
        candidate(GOOD).to_string(),
        &[source()],
        3000
    )
    .await
    .is_err());
    assert!(!*offline.repaired.lock().unwrap());
    let incomplete = Model {
        incomplete_coverage: true,
        ..Model::new()
    };
    assert!(verify(&incomplete, &candidate(GOOD), &[source()])
        .await
        .is_err());
    Ok(())
}

#[test]
fn omitted_duplicate_and_fabricated_inventory_units_are_rejected() {
    let content = units(&candidate(GOOD)).unwrap();
    let valid = UnitClaims {
        index: 0,
        claims: vec![Claim {
            quote: GOOD.into(),
            statement: GOOD.into(),
        }],
        non_factual_reason: String::new(),
    };
    assert!(validate_inventory(
        &Inventory {
            units: vec![valid.clone()]
        },
        &content
    )
    .is_ok());
    assert!(validate_inventory(&Inventory { units: vec![] }, &content).is_err());
    assert!(validate_inventory(
        &Inventory {
            units: vec![valid.clone(), valid.clone()]
        },
        &content
    )
    .is_err());
    let mut fabricated = valid;
    fabricated.claims[0].quote = "invented".into();
    assert!(validate_inventory(
        &Inventory {
            units: vec![fabricated]
        },
        &content
    )
    .is_err());
}

/// Persistence-only tests use an explicit synthetic attestation. This is
/// compiled only for tests and does not bypass production verification.
pub(crate) fn attest(lesson_id: &str, lesson: &PreparedLearningLesson) -> LessonVerificationReport {
    LessonVerificationReport {
        policy: POLICY.into(),
        lesson_id: lesson_id.into(),
        content_sha256: content_hash(lesson).unwrap(),
        checker_model: "persistence-fixture".into(),
        checked_at: 0,
        sources: vec![],
        coverage: (0..lesson.blocks.len() + lesson.questions.len())
            .map(|index| UnitClaims {
                index,
                claims: vec![],
                non_factual_reason: "Persistence fixture".into(),
            })
            .collect(),
        coverage_audit: (0..lesson.blocks.len() + lesson.questions.len())
            .map(|index| CoverageUnit {
                index,
                complete: true,
                reason: "Persistence fixture".into(),
                unresolved_passages: Vec::new(),
            })
            .collect(),
        findings: vec![Finding {
            unit: 0,
            quote: "fixture".into(),
            statement: "fixture".into(),
            verdict: ClaimVerdict::Supported,
            reason: "fixture".into(),
            evidence: vec![],
            supporting_quote: None,
        }],
        executions: vec![],
        issues: vec![],
        retrieval_mode: "lexical_fallback".into(),
        embedding_model: None,
        unexecuted_languages: vec![],
    }
}

#[tokio::test]
async fn publication_requires_current_report_and_source_and_rolls_back_on_rejection() -> Result<()>
{
    use crate::features::learning::{dto::*, repository::LearningRepository};
    let pool = crate::features::learning::tests::pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let mut program = crate::features::learning::tests::fixture();
    let reference = source();
    program.sources = vec![reference];
    repo.create(&program).await?;
    repo.accept(&AcceptLearningProgramRequestDto {
        program_id: program.summary.id.clone(),
        expected_revision: 0,
        title: program.summary.title.clone(),
    })
    .await?;
    let program = repo.get(&program.summary.id).await?;
    let lesson_id = &program.modules[0].lessons[0].id;
    let sources = repo.verification_sources(&program.summary.id).await?;
    let (raw, report) = verify_and_repair(
        &Model::new(),
        "{}",
        &json!({}),
        candidate(BAD).to_string(),
        &sources,
        3000,
    )
    .await?;
    let value: Value = serde_json::from_str(&raw)?;
    let mut prepared = PreparedLearningLesson {
        blocks: vec![LearningBlockDto {
            kind: LearningBlockKind::Explanation,
            title: "CSV header names".into(),
            body: value["blocks"][0]["body"].as_str().unwrap().into(),
            rubric: vec![],
            source_ids: vec![sources[0].id.clone()],
        }],
        questions: vec![],
        keys: vec![],
        verification: None,
    };
    assert!(repo
        .prepare(
            &program.summary.id,
            lesson_id,
            program.summary.revision,
            &prepared
        )
        .await
        .is_err());
    prepared.verification = Some(report.bind(lesson_id, &prepared)?);
    let mut changed = prepared.clone();
    changed.blocks[0].body = BAD.into();
    assert!(repo
        .prepare(
            &program.summary.id,
            lesson_id,
            program.summary.revision,
            &changed
        )
        .await
        .is_err());
    assert_eq!(
        repo.get(&program.summary.id).await?.summary.revision,
        program.summary.revision
    );
    assert!(prepared
        .verification
        .as_ref()
        .unwrap()
        .validate("different-lesson", &prepared)
        .is_err());
    sqlx::query("UPDATE learning_source_library SET deleted_at=1 WHERE program_id=?")
        .bind(&program.summary.id)
        .execute(&pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    assert!(repo
        .prepare(
            &program.summary.id,
            lesson_id,
            program.summary.revision,
            &prepared
        )
        .await
        .is_err());
    sqlx::query("UPDATE learning_source_library SET deleted_at=NULL WHERE program_id=?")
        .bind(&program.summary.id)
        .execute(&pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    repo.prepare(
        &program.summary.id,
        lesson_id,
        program.summary.revision,
        &prepared,
    )
    .await?;
    let saved = repo.get(&program.summary.id).await?;
    assert_eq!(
        saved.modules[0].lessons[0].preparation,
        LearningPreparation::Ready
    );
    let report_json: String = sqlx::query_scalar(
        "SELECT report_json FROM learning_lesson_verifications WHERE lesson_id=?",
    )
    .bind(lesson_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;
    assert!(report_json.contains("lexical_fallback"));
    assert!(!serde_json::to_string(&saved)?.contains("checker_model"));
    Ok(())
}

#[tokio::test]
async fn exact_worked_code_is_executed_and_bad_output_cannot_hide_a_failed_run() -> Result<()> {
    let good = json!({"blocks":[{"kind":"worked_example","body":"```javascript\nconsole.log(2 + 3);\n```"}]});
    let observed = execution::observe(&good).await?;
    assert!(observed[0].passed());
    assert!(observed[0].evidence().contains('5'));
    let bad = json!({"blocks":[{"kind":"worked_example","body":"```javascript\nthrow new Error('broken example');\n```"}]});
    assert!(!execution::observe(&bad).await?[0].passed());
    assert!(execution::observe(
        &json!({"blocks":[{"kind":"worked_example","body":"```python\nprint(1)"}]})
    )
    .await
    .is_err());
    Ok(())
}

#[tokio::test]
async fn unlabeled_data_and_code_are_labeled_without_changing_their_bytes() -> Result<()> {
    use crate::features::learning::lessons::course_generation_tests::ScriptedModel;
    let model=ScriptedModel {outputs:Mutex::new(vec![json!({"fences":[{"id":"block-0-fence-0","language":"text"},{"id":"block-0-fence-1","language":"javascript"}]}).to_string()].into()),prompts:Mutex::new(Vec::new())};
    let tree = "project/\n└── main.rs\n";
    let code = "console.log(' a  b ');\n";
    let body = format!("Directory tree:\n```\n{tree}```\nRunnable example:\n```\n{code}```\n");
    let lesson = json!({"blocks":[{"kind":"worked_example","title":"Tree and program","body":body}],"questions":[]});
    let corrected: Value =
        serde_json::from_str(&normalize_example_fences(&model, lesson.to_string()).await?)?;
    assert_eq!(
        corrected["blocks"][0]["body"],
        format!(
            "Directory tree:\n```text\n{tree}```\nRunnable example:\n```javascript\n{code}```\n"
        )
    );
    let observed = execution::observe(&corrected).await?;
    assert_eq!(observed.len(), 1);
    assert!(observed[0].passed());
    assert!(observed[0].evidence().contains(" a  b "));
    let titled = json!({"blocks":[{"kind":"worked_example","body":"```javascript title=example.js\nconsole.log(42);\n```"}]});
    assert!(execution::observe(&titled).await?[0].passed());
    Ok(())
}

#[tokio::test]
async fn invalid_fence_classification_cannot_hide_an_unexecuted_program() {
    use crate::features::learning::lessons::course_generation_tests::ScriptedModel;
    let lesson = json!({"blocks":[{"kind":"worked_example","body":"```\nconsole.log(42);\n```"}],"questions":[]});
    for response in [
        json!({"fences":[]}),
        json!({"fences":[{"id":"invented","language":"text"}]}),
        json!({"fences":[{"id":"block-0-fence-0","language":"invalid-label"}]}),
    ] {
        let model = ScriptedModel {
            outputs: Mutex::new(vec![response.to_string()].into()),
            prompts: Mutex::new(Vec::new()),
        };
        assert!(normalize_example_fences(&model, lesson.to_string())
            .await
            .is_err());
    }
    assert!(execution::observe(&lesson).await.is_err());
}

struct TypedModel {
    response: CompletionResponse,
}

#[tokio::test]
async fn durable_judge_recovers_truncated_output_without_using_it_as_approval() {
    struct TruncatedJudge(std::sync::atomic::AtomicUsize);
    #[async_trait::async_trait]
    impl LLMPort for TruncatedJudge {
        fn supports_typed_completions(&self) -> bool {
            true
        }
        async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
            assert!(request.no_time_limit);
            assert!(request.max_output_tokens.unwrap() > 512);
            assert_eq!(request.reasoning_effort.as_deref(), Some("low"));
            assert!(!request.want_logprobs);
            let call = self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if call == 0 {
                return Ok(CompletionResponse {
                    text: "Reason: Incomplete and not trustworthy".into(),
                    finish_reason: "length".into(),
                    ..Default::default()
                });
            }
            assert!(serde_json::to_string(&request.input)?.contains("Correct the response format"));
            Ok(CompletionResponse {
                text: "Reason: The reference does not state the claimed date.\nSource passage: none\nVerdict: unsupported".into(),
                finish_reason: "stop".into(),
                ..Default::default()
            })
        }
        async fn generate(&self, _: &str, _: &[String], _: Option<Vec<String>>) -> Result<String> {
            unreachable!()
        }
        async fn generate_streaming(
            &self,
            _: &str,
            _: &[String],
            _: Option<Vec<String>>,
        ) -> Result<Box<dyn futures::Stream<Item = Result<String>> + Send + Unpin + '_>> {
            unreachable!()
        }
        fn model_name(&self) -> &str {
            "truncated-judge"
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
    let model = TruncatedJudge(Default::default());
    let result = ClaimChecker::new(
        &model,
        SamplingOverride::deterministic(),
        512,
        CheckPolicy::Strict,
    )
    .check_passages_without_deadline(
        "Rust was released on the claimed date.",
        &["Rust is a language.".into()],
    )
    .await;
    assert!(
        matches!(result,ClaimJudgment::Judged(outcome) if outcome.verdict == ClaimVerdict::Unsupported)
    );
    assert_eq!(model.0.load(std::sync::atomic::Ordering::Relaxed), 2);
}

#[async_trait::async_trait]
impl LLMPort for TypedModel {
    async fn generate(&self, _: &str, _: &[String], _: Option<Vec<String>>) -> Result<String> {
        Ok(self.response.text.clone())
    }
    async fn generate_streaming(
        &self,
        _: &str,
        _: &[String],
        _: Option<Vec<String>>,
    ) -> Result<Box<dyn futures::Stream<Item = Result<String>> + Send + Unpin + '_>> {
        Err(invalid("unused"))
    }
    fn supports_typed_completions(&self) -> bool {
        true
    }
    async fn complete(&self, _: &CompletionRequest) -> Result<CompletionResponse> {
        Ok(self.response.clone())
    }
    fn model_name(&self) -> &str {
        "typed-fixture"
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
#[tokio::test]
async fn located_evidence_preserves_source_bytes_and_rejects_invented_locations() {
    let source = "A wrapped factual\nstatement.\n```text\na  b\n```".to_string();
    for (citation, valid) in [
        ("passage-0", true),
        ("passage-9", false),
        ("invented", false),
        ("none", false),
    ] {
        let model=TypedModel {response:CompletionResponse {text:format!("Reason: The source contains the wrapped factual statement.\nSource passage: {citation}\nVerdict: supported"),finish_reason:"stop".into(),..Default::default()}};
        let result = ClaimChecker::new(
            &model,
            SamplingOverride::deterministic(),
            512,
            CheckPolicy::Strict,
        )
        .check_passages_without_deadline(
            "A wrapped factual statement.",
            std::slice::from_ref(&source),
        )
        .await;
        if valid {
            assert!(
                matches!(result,ClaimJudgment::Judged(outcome) if outcome.quote.as_deref()==Some(source.as_str()))
            );
        } else {
            assert!(matches!(result, ClaimJudgment::Unusable));
        }
    }
    let misleading_first_token = TypedModel {
        response: CompletionResponse {
            text: "Reason: The source matches.\nSource passage: passage-0\nVerdict: supported"
                .into(),
            finish_reason: "stop".into(),
            first_token_logprobs: Some(vec![
                ("supported".into(), 0.5_f32.ln()),
                ("unsupported".into(), 0.5_f32.ln()),
            ]),
            ..Default::default()
        },
    };
    assert!(matches!(
        ClaimChecker::new(
            &misleading_first_token,
            SamplingOverride::deterministic(),
            512,
            CheckPolicy::Strict
        )
        .check_passages_without_deadline("A wrapped factual statement.", &[source])
        .await,
        ClaimJudgment::Judged(outcome) if outcome.verdict == ClaimVerdict::Supported && outcome.confidence.is_none()
    ));
}

#[tokio::test]
async fn durable_judgment_requires_one_final_verdict_after_its_comparison() {
    for text in [
        "supported\nReason: The source instead says it is a script, not a binary.\nSource passage: passage-0",
        "Verdict: supported\nReason: The comparison is not finished.\nSource passage: passage-0",
        "Reason: The source agrees.\nSource passage: passage-0\nVerdict: supported\nVerdict: unsupported",
        "Reason: The source agrees. Source passage: passage-0 Verdict: supported Verdict: unsupported",
        "Reason: First reason. Reason: Replacement reason. Source passage: passage-0 Verdict: supported",
        "Reason: First reason. Reason: Replacement reason.\nSource passage: passage-0\nVerdict: supported",
        "Reason: The source agrees. Source passage: passage-0 Source passage: passage-1 Verdict: supported",
        "Reason: The source agrees. Verdict: supported Source passage: passage-0",
        "Reason: The source agrees. Source passage: passage-0 Verdict: supported Additional text.",
    ] {
        let model = TypedModel {
            response: CompletionResponse { text: text.into(), finish_reason: "stop".into(), ..Default::default() },
        };
        let result = ClaimChecker::new(&model, SamplingOverride::deterministic(), 512, CheckPolicy::Strict)
            .check_passages_without_deadline("The command downloads a binary.", &["The command downloads a script.".into()]).await;
        assert!(matches!(result, ClaimJudgment::Unusable));
    }
}

#[tokio::test]
async fn durable_judgment_accepts_inline_fields_without_changing_its_evidence() {
    let source = "cargo: the Rust dependency manager and build tool".to_owned();
    for text in [
        "Reason: Passage-0 explicitly describes cargo as the Rust dependency manager and build tool. Source passage: passage-0\nVerdict: supported",
        "Reason: Passage-0 explicitly describes cargo as the Rust dependency manager and build tool. Source passage: passage-0 Verdict: supported",
        "Reason: Passage-0 explicitly describes cargo as the Rust dependency\nmanager and build tool.\nSource passage: passage-0\nVerdict: supported",
    ] {
        let model = TypedModel {
            response: CompletionResponse { text: text.into(), finish_reason: "stop".into(), ..Default::default() },
        };
        let result = ClaimChecker::new(&model, SamplingOverride::deterministic(), 512, CheckPolicy::Strict)
            .check_passages_without_deadline("cargo is the Rust dependency manager and build tool.", std::slice::from_ref(&source)).await;
        assert!(matches!(result, ClaimJudgment::Judged(outcome) if outcome.verdict == ClaimVerdict::Supported && outcome.quote.as_deref() == Some(source.as_str())));
    }
    for citation in ["passage-9", "none", "invented"] {
        let model = TypedModel {
            response: CompletionResponse {
                text: format!(
                    "Reason: The source agrees. Source passage: {citation} Verdict: supported"
                ),
                finish_reason: "stop".into(),
                ..Default::default()
            },
        };
        let result = ClaimChecker::new(
            &model,
            SamplingOverride::deterministic(),
            512,
            CheckPolicy::Strict,
        )
        .check_passages_without_deadline(
            "cargo is the Rust dependency manager and build tool.",
            std::slice::from_ref(&source),
        )
        .await;
        assert!(matches!(result, ClaimJudgment::Unusable));
    }
}

#[tokio::test]
async fn source_passage_prose_does_not_create_a_second_protocol_field() {
    let source = "The measured value is 12.".to_owned();
    for text in [
        "Reason: The factual assertion maps directly to a source passage: the measured value is 12.\nSource passage: passage-0\nVerdict: supported",
        "Reason: One source passage: the measured value is 12. Another source passage: the measurement is recorded.\nSource passage: passage-0\nVerdict: supported",
    ] {
        let model = TypedModel { response: CompletionResponse { text: text.into(), finish_reason: "stop".into(), ..Default::default() } };
        let result = ClaimChecker::new(&model, SamplingOverride::deterministic(), 512, CheckPolicy::Fidelity)
            .check_passages_without_deadline("The measured value is 12.", std::slice::from_ref(&source)).await;
        assert!(matches!(result, ClaimJudgment::Judged(outcome) if outcome.verdict == ClaimVerdict::Supported && outcome.quote.as_deref() == Some(source.as_str())));
    }
    let model = TypedModel { response: CompletionResponse { text: "Reason: The source agrees.\nSource passage: passage-0\nSource passage: passage-1\nVerdict: supported".into(), finish_reason: "stop".into(), ..Default::default() } };
    let result = ClaimChecker::new(
        &model,
        SamplingOverride::deterministic(),
        512,
        CheckPolicy::Fidelity,
    )
    .check_passages_without_deadline("The measured value is 12.", &[source])
    .await;
    assert!(matches!(result, ClaimJudgment::Unusable));
}

#[tokio::test]
async fn malformed_verdict_gets_one_format_correction_without_approving_missing_evidence() {
    use crate::features::learning::lessons::course_generation_tests::ScriptedModel;
    let malformed="The passages describe Rust tools but do not mention Visual Studio.\nReason: Visual Studio is absent.\nSource quote: none";
    for corrected in [
        "unsupported\nReason: Visual Studio is absent.\nSource quote: none",
        malformed,
    ] {
        let model = ScriptedModel {
            outputs: Mutex::new(vec![malformed.into(), corrected.into()].into()),
            prompts: Mutex::new(vec![]),
        };
        let evidence = ClaimEvidence {
            text: SOURCE.into(),
            quote: None,
        };
        let result = ClaimChecker::new(
            &model,
            SamplingOverride::deterministic(),
            512,
            CheckPolicy::Strict,
        )
        .check_without_deadline("Visual Studio includes a package manager.", &evidence)
        .await;
        if corrected.starts_with("unsupported") {
            assert!(
                matches!(result,ClaimJudgment::Judged(outcome) if outcome.verdict==ClaimVerdict::Unsupported)
            );
        } else {
            assert!(matches!(result, ClaimJudgment::Unusable));
        }
        let prompts = model.prompts.lock().unwrap();
        assert_eq!(prompts.len(), 2);
        assert!(prompts[1].contains(SOURCE));
        assert!(prompts[1].contains("Correct the response format"));
    }
    use crate::application::services::claim_verification::parse_verdict_word;
    assert_eq!(
        parse_verdict_word("Label: unsupported\nReason: Missing source."),
        Some(ClaimVerdict::Unsupported)
    );
    assert_eq!(
        parse_verdict_word("Verdict: supported\nReason: Matching source."),
        Some(ClaimVerdict::Supported)
    );
}

#[tokio::test]
async fn strict_checker_preserves_long_exact_quotes_and_rejects_reflowed_code() {
    let quote = format!(
        "{} The field contains two spaces: a  b.",
        "A factual reference sentence. ".repeat(20)
    );
    let evidence = ClaimEvidence {
        text: quote.clone(),
        quote: None,
    };
    for (cited, valid) in [(quote.clone(), true), (quote.replace("a  b", "a b"), false)] {
        let model=TypedModel {response:CompletionResponse {
            text:format!("supported\nReason: The exact reference contains the statement.\nSource quote: {cited}"),
            finish_reason:"stop".into(),..Default::default()
        }};
        let checker = ClaimChecker::new(
            &model,
            SamplingOverride::deterministic(),
            512,
            CheckPolicy::Strict,
        );
        let result = checker
            .check_without_deadline("The field contains two spaces.", &evidence)
            .await;
        if valid {
            assert!(
                matches!(result,ClaimJudgment::Judged(outcome) if outcome.quote.as_deref()==Some(quote.as_str()))
            );
        } else {
            assert!(matches!(result, ClaimJudgment::Unusable));
        }
    }
}

#[tokio::test]
async fn strict_checker_rejects_missing_invented_quotes_and_truncated_replies() {
    let evidence = ClaimEvidence {
        text: SOURCE.into(),
        quote: Some(GOOD.into()),
    };
    for (text, finish) in [("supported","stop"),("supported\nReason: a match\nSource quote: fabricated", "stop"),("supported\nReason: a match\nSource quote: DictReader preserves field-name whitespace by default.","length")] {
        let model=TypedModel { response:CompletionResponse { text:text.into(),finish_reason:finish.into(), ..Default::default() } };
        let strict=ClaimChecker::new(&model,SamplingOverride::deterministic(),512,CheckPolicy::Strict).check(GOOD,&evidence,Instant::now()+Duration::from_secs(5)).await;
        assert!(matches!(strict,ClaimJudgment::Unusable));
    }
    for alternatives in [
        vec![
            ("contradicted".into(), 0.9_f32.ln()),
            ("supported".into(), 0.1_f32.ln()),
        ],
        vec![
            ("supported".into(), 0.5_f32.ln()),
            ("contradicted".into(), 0.5_f32.ln()),
        ],
    ] {
        let model = TypedModel {
            response: CompletionResponse {
                text: format!(
                    "supported\nReason: The source preserves spaces.\nSource quote: {GOOD}"
                ),
                finish_reason: "stop".into(),
                first_token_logprobs: Some(alternatives),
                ..Default::default()
            },
        };
        let checker = ClaimChecker::new(
            &model,
            SamplingOverride::deterministic(),
            512,
            CheckPolicy::Strict,
        );
        assert!(matches!(
            checker
                .check(GOOD, &evidence, Instant::now() + Duration::from_secs(5))
                .await,
            ClaimJudgment::Judged(outcome) if outcome.verdict == ClaimVerdict::Unsupported && outcome.quote.is_none()
        ));
        assert!(matches!(
            checker.check(GOOD, &evidence, Instant::now()).await,
            ClaimJudgment::OutOfTime
        ));
    }
}

pub(crate) async fn install_source(
    pool: &sqlx::SqlitePool,
    program_id: &str,
    text: &str,
) -> Result<()> {
    use crate::features::learning::{
        dto::{LearningSourceKind, LearningSourcePolicy},
        source_library::{CapturedLearningSource, LearningSourceLibraryRepository},
    };
    let id = || uuid::Uuid::new_v4().to_string();
    LearningSourceLibraryRepository::new(pool.clone())
        .add(
            &id(),
            &id(),
            &id(),
            program_id,
            LearningSourceKind::Pasted,
            "add_text",
            "Test reference",
            None,
            LearningSourcePolicy::Fixed,
            CapturedLearningSource {
                title: "Test reference".into(),
                publisher: None,
                requested_url: None,
                resolved_url: None,
                text: text.into(),
                truncated: false,
                extraction_version: "test".into(),
            },
            "fixture".into(),
        )
        .await
}

#[tokio::test]
async fn unsupported_languages_are_disclosed_and_csv_fixture_is_test_only() -> Result<()> {
    let rust = json!({"blocks":[{"kind":"worked_example","body":"```rust\nfn main() {}\n```"}]});
    assert!(execution::observe(&rust).await?.is_empty());
    assert_eq!(execution::unexecuted_languages(&rust), vec!["rust"]);
    let csv = json!({"blocks":[{"kind":"worked_example","body":format!("```python\n{}\n```", include_str!("csv_reference.py"))}]});
    let observed = execution::observe(&csv).await?;
    assert!(observed[0].passed());
    assert!(observed[0].evidence().contains("header_whitespace"));
    Ok(())
}

#[tokio::test]
async fn evidence_view_hides_assessment_claims_and_marks_source_changes() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let program = crate::features::learning::tests::fixture();
    let repo = crate::features::learning::repository::LearningRepository::new(pool.clone());
    repo.create(&program).await?;
    let lesson = &program.modules[0].lessons[0];
    // This fixture deliberately puts a private key in an assessment unit. The
    // public projection must omit it even when there are zero teaching blocks.
    let source = &program.sources[0];
    let saved = repo.verification_sources(&program.summary.id).await?;
    let report = json!({"policy":POLICY,"checked_at":1,"checker_model":"fixture","sources":[{"id":source.id,"sha256":digest(&saved[0].excerpt)}],"findings":[{"unit":0,"statement":"PRIVATE ANSWER KEY","reason":"PRIVATE RATIONALE","evidence":[]}],"executions":[],"retrieval_mode":"hybrid"});
    sqlx::query("INSERT INTO learning_lesson_verifications(lesson_id,program_id,policy,content_sha256,report_json,checked_at) VALUES(?,?,?,?,?,?)").bind(&lesson.id).bind(&program.summary.id).bind(POLICY).bind("fixture").bind(report.to_string()).bind(1_i64).execute(&pool).await.map_err(|e|AppError::Database(e.to_string()))?;
    let view =
        crate::features::learning::lesson_evidence::get(&pool, &program.summary.id, &lesson.id)
            .await?
            .unwrap();
    assert!(view.teaching_claims.is_empty());
    assert!(!serde_json::to_string(&view)?.contains("PRIVATE"));
    assert!(view.sources_current);
    sqlx::query("UPDATE learning_source_library SET deleted_at=1 WHERE program_id=?")
        .bind(&program.summary.id)
        .execute(&pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let view =
        crate::features::learning::lesson_evidence::get(&pool, &program.summary.id, &lesson.id)
            .await?
            .unwrap();
    assert!(!view.sources_current);
    Ok(())
}
