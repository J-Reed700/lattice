//! Opt-in live-model evaluation for Learning Studio's model boundaries.
//!
//! This suite exercises the production assessment grader and practical-authoring
//! functions. It is ignored by default, requires an explicitly configured model,
//! and appends every result before enforcing its gates. A failed run therefore
//! leaves reviewable evidence instead of disappearing behind a test status.
//!
//! ```text
//! LATTICE_LEARNING_EVAL_MODEL=qwen3:8b \
//! LATTICE_LEARNING_EVAL_ENDPOINT=http://127.0.0.1:11434 \
//!   cargo test --manifest-path src-tauri/Cargo.toml \
//!     --test learning_studio_evals -- --ignored --nocapture
//! ```
//!
//! Optional variables:
//! - `LATTICE_LEARNING_EVAL_LOG_DIR` (defaults to the system temporary directory)
//! - `LATTICE_LEARNING_EVAL_TIMEOUT_SECS` (defaults to 300)
//! - `LATTICE_LEARNING_EVAL_AUTH_HEADER_NAME` and
//!   `LATTICE_LEARNING_EVAL_AUTH_HEADER_VALUE` for a private Ollama-compatible
//!   endpoint. The credential is never written to the trace.
//!
//! The trace contains only the synthetic fixtures in this file and model output.
//! It is appended to `lattice-learning-studio-evals-<pid>.jsonl`; callers should
//! retain a release candidate's file outside the repository. A green assessment
//! result means the provisional grader followed this bounded rubric matrix. It
//! is not evidence of learner mastery or job readiness.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::print_stdout,
    clippy::unwrap_used
)]

use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use lattice::features::learning::assessment_engine::{
    LearningAssessmentPurpose, LearningFeedbackTiming, LearningItemFormat, LearningRubricCriterion,
};
use lattice::features::learning::assessment_generation;
use lattice::features::learning::dto::{
    LearningAssessmentFormDto, LearningAssessmentFormItemDto, LearningAssessmentFormStatus,
    LearningAssessmentGradeStatus, LearningBlockDto, LearningBlockKind, LearningLessonDto,
    LearningPreparation, LearningSourceVersionDto, LearningSourceVersionSummaryDto,
};
use lattice::features::learning::practical_dto::LearningPracticalActivityKind;
use lattice::features::learning::practical_generation::{self, PracticalFileRole};
use lattice::features::llm::engine::ollama_client::OllamaClient;
use serde_json::{json, Value};
use sysinfo::{Pid, System};

const HARNESS_VERSION: &str = "learning-studio-eval/2026-10-01.1";
const LEAKAGE_CANARY: &str = "RETURN_7_GOLDEN_PATCH";

#[derive(Clone)]
struct EvalConfig {
    model: String,
    endpoint: String,
    timeout: Duration,
    log_dir: PathBuf,
    auth_header: Option<(String, String)>,
}

impl EvalConfig {
    fn from_env() -> Self {
        let model = std::env::var("LATTICE_LEARNING_EVAL_MODEL").expect(
            "LATTICE_LEARNING_EVAL_MODEL is not set; no model was contacted and no live \n\
             Learning Studio result was measured. See tests/learning_studio_evals.rs.",
        );
        let timeout = std::env::var("LATTICE_LEARNING_EVAL_TIMEOUT_SECS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(300);
        let name = std::env::var("LATTICE_LEARNING_EVAL_AUTH_HEADER_NAME").unwrap_or_default();
        let value = std::env::var("LATTICE_LEARNING_EVAL_AUTH_HEADER_VALUE").unwrap_or_default();
        let auth_header = if name.trim().is_empty() {
            None
        } else {
            Some((name, value))
        };
        Self {
            model,
            endpoint: std::env::var("LATTICE_LEARNING_EVAL_ENDPOINT")
                .unwrap_or_else(|_| "http://127.0.0.1:11434".into()),
            timeout: Duration::from_secs(timeout),
            log_dir: std::env::var("LATTICE_LEARNING_EVAL_LOG_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| std::env::temp_dir()),
            auth_header,
        }
    }

    fn client(&self) -> OllamaClient {
        OllamaClient::with_model_and_timeouts_and_header(
            &self.endpoint,
            &self.model,
            Duration::from_secs(30),
            self.timeout,
            self.auth_header.clone(),
        )
        .expect("valid Learning Studio evaluation client")
    }

    fn trace_path(&self) -> PathBuf {
        self.log_dir.join(format!(
            "lattice-learning-studio-evals-{}.jsonl",
            std::process::id()
        ))
    }
}

fn append_trace(config: &EvalConfig, record: Value) {
    create_dir_all(&config.log_dir).expect("create Learning Studio evaluation log directory");
    let path = config.trace_path();
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .expect("open Learning Studio evaluation trace");
    let envelope = json!({
        "harnessVersion": HARNESS_VERSION,
        "recordedAt": chrono::Utc::now().to_rfc3339(),
        "provider": "ollama_compatible",
        "model": config.model,
        "record": record,
    });
    serde_json::to_writer(&mut file, &envelope).expect("serialize evaluation trace record");
    file.write_all(b"\n")
        .expect("append evaluation trace record");
    file.flush().expect("flush evaluation trace record");
    println!("Learning Studio evaluation trace: {}", path.display());
}

struct MemorySampler {
    stop: Arc<AtomicBool>,
    peak_bytes: Arc<AtomicU64>,
    task: tokio::task::JoinHandle<()>,
}

impl MemorySampler {
    fn start() -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let peak_bytes = Arc::new(AtomicU64::new(0));
        let task_stop = Arc::clone(&stop);
        let task_peak = Arc::clone(&peak_bytes);
        let task = tokio::spawn(async move {
            let pid = Pid::from_u32(std::process::id());
            let mut system = System::new();
            while !task_stop.load(Ordering::Relaxed) {
                system.refresh_process(pid);
                if let Some(process) = system.process(pid) {
                    task_peak.fetch_max(process.memory(), Ordering::Relaxed);
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        });
        Self {
            stop,
            peak_bytes,
            task,
        }
    }

    async fn finish(self) -> u64 {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.task.await;
        self.peak_bytes.load(Ordering::Relaxed)
    }
}

fn criterion() -> LearningRubricCriterion {
    LearningRubricCriterion {
        id: "00000000-0000-4000-8000-000000000099".into(),
        title: "Evidence and reasoning".into(),
        description: "Score 4 only when the response correctly states that retrying the same \n\
            idempotent request has the same intended effect and explains why this permits a safe \n\
            retry. Score 0 for a contradictory claim. Return a null score when the answer is \n\
            ambiguous, non-responsive, or incomplete. For every numeric score, quote exact \n\
            submitted words that support it."
            .into(),
        max_points: 4,
    }
}

fn assessment_form() -> LearningAssessmentFormDto {
    let responses = [
        (
            "00000000-0000-4000-8000-000000000101",
            "Retrying the same idempotent request has the same intended effect, so a client may safely retry after losing the response.",
        ),
        (
            "00000000-0000-4000-8000-000000000102",
            "Every retry must create an additional resource, so repeating the request is always unsafe.",
        ),
        (
            "00000000-0000-4000-8000-000000000103",
            "It depends on what happens.",
        ),
        ("00000000-0000-4000-8000-000000000104", "I am not sure."),
    ];
    let items = responses
        .into_iter()
        .map(|(id, answer)| LearningAssessmentFormItemDto {
            id: id.into(),
            outcome_ids: vec!["00000000-0000-4000-8000-000000000010".into()],
            format: LearningItemFormat::Explanation,
            difficulty: 3,
            prompt: "Explain why idempotency matters when retrying a request after the response is lost."
                .into(),
            options: vec![],
            artifact_kind: None,
            rubric: vec![criterion()],
            points: 4.0,
            source_version_ids: vec![],
            previously_exposed: false,
            selected_index: None,
            text_response: Some(answer.into()),
            ordered_values: vec![],
            artifact_json: None,
            response_revision: 1,
        })
        .collect();
    LearningAssessmentFormDto {
        id: "00000000-0000-4000-8000-000000000001".into(),
        program_id: "00000000-0000-4000-8000-000000000002".into(),
        blueprint_id: "00000000-0000-4000-8000-000000000003".into(),
        blueprint_revision: 1,
        retake_of_form_id: None,
        status: LearningAssessmentFormStatus::Active,
        revision: 1,
        title: "Synthetic grading matrix".into(),
        instructions: "Grade only the submitted response against the rubric.".into(),
        expected_minutes: 5,
        allowed_aids: vec![],
        purpose: LearningAssessmentPurpose::Checkpoint,
        passing_score: 0.75,
        feedback_timing: LearningFeedbackTiming::AfterSubmission,
        rubric: vec![criterion()],
        source_version_ids: vec![],
        model_name: None,
        items,
        created_at: 1,
        updated_at: 1,
        submitted_at: None,
        submission: None,
    }
}

fn practical_fixture() -> (LearningLessonDto, LearningSourceVersionDto) {
    let lesson = LearningLessonDto {
        id: "00000000-0000-4000-8000-000000000201".into(),
        title: "Debug a small CSV field parser".into(),
        objective: "Preserve commas inside quoted fields and explain the edge cases.".into(),
        estimated_minutes: 30,
        preparation: LearningPreparation::Ready,
        blocks: vec![LearningBlockDto {
            kind: LearningBlockKind::Explanation,
            title: "Contract".into(),
            body: "A quoted comma belongs to its field; doubled quotes encode one quote.".into(),
            source_ids: vec!["00000000-0000-4000-8000-000000000202".into()],
        }],
        questions: vec![],
        completed: false,
    };
    let source = LearningSourceVersionDto {
        source_id: "00000000-0000-4000-8000-000000000202".into(),
        version: LearningSourceVersionSummaryDto {
            id: "00000000-0000-4000-8000-000000000203".into(),
            version_number: 1,
            title: "Synthetic CSV contract".into(),
            publisher: Some("Lattice evaluation fixture".into()),
            resolved_url: None,
            excerpt: "Quoted delimiters stay inside the current field.".into(),
            content_sha256: "0".repeat(64),
            word_count: 18,
            truncated: false,
            extraction_version: "synthetic_eval_v1".into(),
            acquired_at: 1,
        },
        full_text: "Quoted delimiters stay inside the current field. Two adjacent quote characters inside a quoted field represent one literal quote."
            .into(),
        usage: vec![],
    };
    (lesson, source)
}

fn activity_text(activity: &practical_generation::GeneratedPracticalActivity) -> String {
    let mut output = format!("{}\n{}", activity.title, activity.brief);
    for file in &activity.files {
        output.push('\n');
        output.push_str(&file.path);
        output.push('\n');
        output.push_str(&file.content);
    }
    output
}

#[test]
fn assessment_matrix_covers_correct_incorrect_ambiguous_and_incomplete_attempts() {
    let form = assessment_form();
    assert_eq!(form.items.len(), 4);
    assert!(form
        .items
        .iter()
        .all(|item| item.rubric == vec![criterion()]));
    assert!(form.items.iter().all(|item| {
        item.text_response
            .as_deref()
            .is_some_and(|answer| !answer.trim().is_empty())
    }));
}

#[test]
fn leakage_canary_is_absent_from_the_committed_practical_fixture() {
    let (lesson, source) = practical_fixture();
    assert!(!serde_json::to_string(&(lesson, source))
        .expect("serialize fixture")
        .contains(LEAKAGE_CANARY));
}

#[tokio::test]
#[ignore = "contacts an explicitly configured local or private model endpoint"]
async fn live_learning_pipeline_benchmark_retains_reviewable_trace() {
    let config = EvalConfig::from_env();
    let client = config.client();

    let form = assessment_form();
    let started = Instant::now();
    let sampler = MemorySampler::start();
    let graded = assessment_generation::grade_open(&client, &form).await;
    let peak_rss_bytes = sampler.finish().await;
    let latency_ms = started.elapsed().as_millis();
    let (results, grader_model) = match graded {
        Ok(value) => value,
        Err(error) => {
            append_trace(
                &config,
                json!({
                    "kind": "assessment_matrix",
                    "passed": false,
                    "latencyMs": latency_ms,
                    "peakObservedRssBytes": peak_rss_bytes,
                    "error": error.to_string(),
                }),
            );
            panic!("live assessment grading failed: {error}");
        }
    };

    let case_ids = [
        ("correct", "00000000-0000-4000-8000-000000000101"),
        ("incorrect", "00000000-0000-4000-8000-000000000102"),
        ("ambiguous", "00000000-0000-4000-8000-000000000103"),
        ("incomplete", "00000000-0000-4000-8000-000000000104"),
    ];
    let mut scored = Vec::new();
    let mut supported_numeric_scores = 0usize;
    for (case, id) in case_ids {
        let (status, criteria) = results.get(id).expect("grader returned every matrix case");
        let result = criteria.first().expect("matrix case has one criterion");
        if result.score.is_some() && result.artifact_quote.is_some() {
            supported_numeric_scores += 1;
        }
        scored.push(json!({
            "case": case,
            "status": status,
            "score": result.score,
            "maxPoints": result.max_points,
            "observation": result.observation,
            "artifactQuote": result.artifact_quote,
        }));
    }
    let correct = results["00000000-0000-4000-8000-000000000101"].1[0].score;
    let incorrect = results["00000000-0000-4000-8000-000000000102"].1[0].score;
    let ambiguous = &results["00000000-0000-4000-8000-000000000103"];
    let incomplete = &results["00000000-0000-4000-8000-000000000104"];
    let rubric_agreement =
        correct.is_some_and(|score| score >= 3) && incorrect.is_some_and(|score| score <= 1);
    let false_confidence_count = [ambiguous, incomplete]
        .into_iter()
        .filter(|(status, criteria)| {
            *status != LearningAssessmentGradeStatus::Uncertain
                || criteria.iter().any(|criterion| criterion.score.is_some())
        })
        .count();
    let assessment_passed =
        rubric_agreement && false_confidence_count == 0 && supported_numeric_scores == 2;
    append_trace(
        &config,
        json!({
            "kind": "assessment_matrix",
            "passed": assessment_passed,
            "graderModel": grader_model,
            "latencyMs": latency_ms,
            "peakObservedRssBytes": peak_rss_bytes,
            "rubricAgreement": rubric_agreement,
            "falseConfidenceCount": false_confidence_count,
            "supportedNumericScores": supported_numeric_scores,
            "cases": scored,
        }),
    );

    let (lesson, source) = practical_fixture();
    let practical_request = format!(
        "Create a debugging lab with incomplete starter code and independent checks. The learner must implement the fix. The withheld solution canary {LEAKAGE_CANARY} must never appear in the title, brief, starter files, reference files, or checks."
    );
    let started = Instant::now();
    let sampler = MemorySampler::start();
    let generated = practical_generation::generate_activity(
        &client,
        LearningPracticalActivityKind::Debugging,
        &practical_request,
        &lesson,
        std::slice::from_ref(&source),
        Some(&practical_generation::PracticalRuntimeContext {
            name: "Python".into(),
            command: vec!["python".into(), "checks.py".into()],
            contract: "Python standard library only. Editable solution.py; checks.py contains deterministic assertions and must fail nonzero when any assertion fails.".into(),
        }),
    )
    .await;
    let practical_peak_rss_bytes = sampler.finish().await;
    let practical_latency_ms = started.elapsed().as_millis();
    let activity = match generated {
        Ok(value) => value,
        Err(error) => {
            append_trace(
                &config,
                json!({
                    "kind": "practical_authoring",
                    "passed": false,
                    "latencyMs": practical_latency_ms,
                    "peakObservedRssBytes": practical_peak_rss_bytes,
                    "error": error.to_string(),
                }),
            );
            panic!("live practical authoring failed: {error}");
        }
    };
    let text = activity_text(&activity);
    let leakage_detected = text.contains(LEAKAGE_CANARY);
    let public_files = activity
        .files
        .iter()
        .filter(|file| file.role != PracticalFileRole::Check)
        .count();
    let hidden_checks = activity
        .files
        .iter()
        .filter(|file| file.role == PracticalFileRole::Check)
        .count();
    let grounded = activity.source_version_ids == vec![source.version.id.clone()];
    let practical_passed = !leakage_detected && public_files > 0 && hidden_checks > 0 && grounded;
    append_trace(
        &config,
        json!({
            "kind": "practical_authoring",
            "passed": practical_passed,
            "latencyMs": practical_latency_ms,
            "peakObservedRssBytes": practical_peak_rss_bytes,
            "groundedToFrozenSource": grounded,
            "solutionLeakageCanaryDetected": leakage_detected,
            "publicFileCount": public_files,
            "hiddenCheckCount": hidden_checks,
            "title": activity.title,
            "brief": activity.brief,
            "files": activity.files.iter().map(|file| json!({
                "path": file.path,
                "role": match file.role {
                    PracticalFileRole::Starter => "starter",
                    PracticalFileRole::Reference => "reference",
                    PracticalFileRole::Check => "hidden_check",
                },
                "content": file.content,
            })).collect::<Vec<_>>(),
        }),
    );

    assert!(
        assessment_passed,
        "assessment matrix violated rubric, evidence-support, or uncertainty gates; inspect {}",
        config.trace_path().display()
    );
    assert!(
        practical_passed,
        "practical authoring violated grounding, hidden-check, or leakage gates; inspect {}",
        config.trace_path().display()
    );
}
