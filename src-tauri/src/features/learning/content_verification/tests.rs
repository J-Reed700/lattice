#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
use super::*;
use crate::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use std::sync::Mutex;

const BAD: &str = "DictReader strips field-name whitespace by default.";
const GOOD: &str = "DictReader preserves field-name whitespace by default.";
const SOURCE: &str = "DictReader preserves field-name whitespace by default. Missing values in nonblank rows default to None. The optional skipinitialspace setting ignores spaces immediately after delimiters; it defaults to false.";

/// A deterministic protocol fixture, not evidence of a live model's accuracy.
/// Execution behavior is covered separately with real bundled guests.
struct Model {
    repaired: Mutex<bool>,
    failing_judge: bool,
    incomplete_coverage: bool,
}
impl Model {
    fn new() -> Self {
        Self {
            repaired: Mutex::new(false),
            failing_judge: false,
            incomplete_coverage: false,
        }
    }
}
fn context(prompt: &str) -> Value {
    serde_json::from_str(prompt.rsplit("\n\n").next().unwrap()).unwrap()
}
fn first_text(v: &Value) -> Option<&str> {
    v.get("body")
        .or_else(|| v.get("explanation"))
        .and_then(Value::as_str)
}
/// Shared with course fixtures so they exercise the additional protocol calls.
pub(crate) fn fixture_response(prompt: &str) -> Option<String> {
    if prompt.starts_with("Extract lesson claims.") {
        let data = context(prompt);
        let units = data["units"].as_array().unwrap();
        return Some(json!({"units":units.iter().enumerate().map(|(index, v)| {
            let quote = first_text(v).unwrap();
            let statement = if quote.contains(BAD) { BAD } else if quote.contains(GOOD) { GOOD } else { quote };
            json!({"index":index,"claims":[{"quote":quote,"statement":statement}],"nonFactualReason":""})
        }).collect::<Vec<_>>()} ).to_string());
    }
    if prompt.starts_with("Audit claim coverage independently.") {
        let data = context(prompt);
        return Some(json!({"units":data["units"].as_array().unwrap().iter().enumerate().map(|(index,_)|json!({"index":index,"complete":true,"reason":"All fixture assertions are represented."})).collect::<Vec<_>>()} ).to_string());
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
            return Ok(json!({"issues":[],"blockChecks":[{"index":0,"quote":GOOD,"finding":"The corrected field-name statement agrees with the concrete example.","hasDefect":false}]}).to_string());
        }
        if prompt.starts_with("Source passages:") {
            if self.failing_judge {
                return Err(invalid("fixture offline"));
            }
            let bad = prompt.split("\n\nClaim: ").nth(1).unwrap().starts_with(BAD);
            return Ok(format!("{}\nReason: The captured reference and runtime preserve the header spaces.\nSource quote: {GOOD}", if bad { "contradicted" } else { "supported" }));
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
async fn missing_evidence_failed_judge_and_incomplete_coverage_never_approve() -> Result<()> {
    let model = Model::new();
    let no_evidence = verify(&model, &candidate("A short factual definition."), &[]).await?;
    assert_eq!(no_evidence.findings[0].verdict, ClaimVerdict::Unverified);
    let offline = Model {
        failing_judge: true,
        ..Model::new()
    };
    let report = verify(
        &offline,
        &candidate("A short factual definition."),
        &[source()],
    )
    .await?;
    assert_eq!(report.findings[0].verdict, ClaimVerdict::Unverified);
    assert!(verify_and_repair(
        &offline,
        "{}",
        &json!({}),
        candidate("A short factual definition.").to_string(),
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
    use super::super::{dto::*, repository::LearningRepository};
    let pool = super::super::tests::pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let mut program = super::super::tests::fixture();
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

struct TypedModel {
    response: CompletionResponse,
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
            ClaimJudgment::Unusable
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
    use super::super::{
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
