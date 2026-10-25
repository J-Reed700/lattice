//! Opt-in research-to-guide control for a retained compiler-proven counterexample.
//! Web responses are recorded official-document excerpts, not live public search.
//! The model plans real queries; production research persists the fetched bodies.
//! Run with the usual LATTICE_LEARNING_EVAL_* settings and --ignored --nocapture.
//! LATTICE_INTERVIEW_REFERENCE_REASON_FIRST=1 enables a test-only two-step proxy.
//! Assisted results never erase the retained unassisted false acceptance.
#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::print_stdout,
    clippy::unwrap_used
)]

use std::{
    fs::{create_dir_all, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::Duration,
};

use lattice::features::learning::{
    dto::*,
    interview_research::research_interview_answers,
    practical_dto::*,
    practice::interview_answers::{generate_answer_guides, AnswerStandard},
    repository::LearningRepository,
    source_library::LearningSourceLibraryRepository,
};
use lattice_core::{
    application::{
        contracts::{
            settings::{LLMSettingsDto, LlamaCppSettingsDto},
            web::{FetchUrlContentOutput, WebSearchInput, WebSearchOutput, WebSearchResult},
        },
        ports::LLMPort,
        ports::WebServiceTrait,
    },
    domain::learning::claim_review::SourceAddedBy,
    shared::error::{AppError, Result},
};
use serde::Deserialize;
use serde_json::{json, Value};

#[path = "interview_reference_evals/reason_first.rs"]
mod reason_first;
#[path = "learning_studio_evals/recording.rs"]
mod recording;

const HARNESS_VERSION: &str = "interview-reference-eval/2026-10-24.1";
const SNIPPET_CANARY: &str = "SEARCH_SNIPPET_IS_NOT_REFERENCE_EVIDENCE";
const FIXTURE: &str = include_str!("fixtures/interview_reference_counterexample.json");

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    question: LearningInterviewQuestionDto,
    documents: Vec<Document>,
    retained_incorrect_standard: AnswerStandard,
}

#[derive(Deserialize)]
struct Document {
    url: String,
    title: String,
    content: String,
}

fn fixture() -> Fixture {
    serde_json::from_str(FIXTURE).expect("retained question and official-source fixture")
}

fn invalid(message: impl Into<String>) -> AppError {
    AppError::InvalidInput(message.into())
}

#[derive(Clone)]
struct EvalConfig {
    provider: String,
    model: String,
    log_dir: PathBuf,
}

fn client() -> Result<(Box<dyn LLMPort>, EvalConfig, bool)> {
    let model = std::env::var("LATTICE_LEARNING_EVAL_MODEL")
        .map_err(|_| invalid("An explicit LATTICE_LEARNING_EVAL_MODEL is required."))?;
    let provider =
        std::env::var("LATTICE_LEARNING_EVAL_PROVIDER").unwrap_or_else(|_| "ollama".into());
    let endpoint = std::env::var("LATTICE_LEARNING_EVAL_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:11434".into());
    let timeout = std::env::var("LATTICE_LEARNING_EVAL_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(300);
    let header_name = std::env::var("LATTICE_LEARNING_EVAL_AUTH_HEADER_NAME").unwrap_or_default();
    let header_value = std::env::var("LATTICE_LEARNING_EVAL_AUTH_HEADER_VALUE").unwrap_or_default();
    let inner: Box<dyn LLMPort> = match provider.as_str() {
        "llama_cpp" => Box::new(lattice::features::llm::llama_cpp::LlamaCppLlm::new(
            &LLMSettingsDto {
                context_window: std::env::var("LATTICE_LEARNING_EVAL_CONTEXT_TOKENS")
                    .ok()
                    .and_then(|value| value.parse::<u32>().ok())
                    .filter(|value| *value > 0)
                    .unwrap_or(32_768),
                timeout_seconds: timeout,
                llama_cpp: LlamaCppSettingsDto {
                    url: endpoint,
                    model: model.clone(),
                    auth_header_name: header_name,
                    auth_header_value: header_value,
                },
                ..Default::default()
            },
        )?),
        "ollama" => Box::new(
            lattice::features::llm::engine::ollama_client::OllamaClient::with_model_and_timeouts_and_header(
                endpoint,
                model.clone(),
                Duration::from_secs(30),
                Duration::from_secs(u64::from(timeout)),
                (!header_name.is_empty()).then_some((header_name, header_value)),
            )?,
        ),
        _ => return Err(invalid("Unsupported explicitly configured provider.")),
    };
    let config = EvalConfig {
        provider,
        model,
        log_dir: std::env::var("LATTICE_LEARNING_EVAL_LOG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir()),
    };
    let recording: Box<dyn LLMPort> = Box::new(recording::RecordingLlm::new(inner, config.clone()));
    let two_step =
        std::env::var("LATTICE_INTERVIEW_REFERENCE_REASON_FIRST").is_ok_and(|value| value == "1");
    let model = if two_step {
        reason_first::candidate(recording)
    } else {
        recording
    };
    Ok((model, config, two_step))
}

/// Recorded excerpts remain visibly distinct from live public-web discovery.
struct RecordedReferences {
    fixture: Fixture,
    searches: Mutex<Vec<Value>>,
    fetches: Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl WebServiceTrait for RecordedReferences {
    async fn search_web(&self, input: &WebSearchInput) -> Result<WebSearchOutput> {
        self.searches
            .lock()
            .map_err(|_| invalid("Search recording lock poisoned."))?
            .push(serde_json::to_value(input).map_err(|error| invalid(error.to_string()))?);
        let results = self
            .fixture
            .documents
            .iter()
            .map(|document| WebSearchResult {
                title: document.title.clone(),
                url: document.url.clone(),
                snippet: SNIPPET_CANARY.into(),
                published_date: None,
                source: None,
            })
            .collect::<Vec<_>>();
        serde_json::from_value(json!({
            "query":input.query,"result_count":results.len(),"results":results,
            "page":input.page,"offset":0,"has_more":false,
            "providers_used":["recorded_official_document_excerpts"]
        }))
        .map_err(|error| invalid(error.to_string()))
    }

    async fn fetch_url_content(&self, url: &str) -> Result<FetchUrlContentOutput> {
        self.validate_url(url)?;
        self.fetches
            .lock()
            .map_err(|_| invalid("Fetch recording lock poisoned."))?
            .push(url.into());
        let document = self
            .fixture
            .documents
            .iter()
            .find(|document| document.url == url)
            .ok_or_else(|| invalid("No recorded official document for the requested URL."))?;
        Ok(FetchUrlContentOutput {
            url: document.url.clone(),
            title: Some(document.title.clone()),
            content: document.content.clone(),
            content_truncated: false,
            word_count: document.content.split_whitespace().count(),
            fetch_time_ms: 0.0,
            content_type: Some("text/plain".into()),
            from_cache: false,
            read_via: Default::default(),
        })
    }

    fn validate_url(&self, address: &str) -> Result<()> {
        let url = url::Url::parse(address).map_err(|error| invalid(error.to_string()))?;
        if url.scheme() != "https" || url.host_str() != Some("doc.rust-lang.org") {
            return Err(invalid(
                "This recorded control only serves official Rust documents.",
            ));
        }
        Ok(())
    }
}

fn plan(question: LearningInterviewQuestionDto) -> LearningInterviewPlanDto {
    LearningInterviewPlanDto {
        company_name: "Parcel Lantern".into(),
        company_url: None,
        target_role: "Staff Engineer".into(),
        round_kind: LearningInterviewRoundKind::DebuggingAndCode,
        company_summary: "An invented company; the supplied Rust task is hypothetical.".into(),
        learner_role: "Candidate".into(),
        counterpart_role: "Interviewer".into(),
        researched_source_version_ids: vec![],
        questions: vec![question],
    }
}

fn compiler_control(question: &str) -> Result<String> {
    let body = question
        .split_once("fn apply_event")
        .and_then(|(_, rest)| rest.split_once("// Hypothetical failure trace:"))
        .map(|(body, _)| body)
        .ok_or_else(|| invalid("The retained compiler counterexample changed unexpectedly."))?;
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("counterexample.rs");
    std::fs::write(
        &source,
        format!(
            "struct Payload; struct SyncError; struct Event {{ id: u64, payload: Option<Payload> }}\nfn apply_event{body}"
        ),
    )?;
    let result =
        std::process::Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
            .args(["--crate-type=lib", "--edition=2021", "--emit=metadata"])
            .arg(&source)
            .arg("-o")
            .arg(directory.path().join("counterexample.rmeta"))
            .output()?;
    let stderr = String::from_utf8_lossy(&result.stderr).into_owned();
    if result.status.success()
        || !stderr.contains("error[E0594]")
        || stderr.matches("error[").count() != 1
    {
        return Err(invalid(format!(
            "The retained E0594 oracle did not reproduce: {stderr}"
        )));
    }
    Ok(stderr)
}

/// Bounded checks for this known failure; these never replace manual review.
fn diagnosis_gates(standard: &AnswerStandard) -> Value {
    let answer = standard.reference_answer.to_ascii_lowercase();
    let compiler_error = [
        "does not compile",
        "doesn't compile",
        "cannot compile",
        "fails to compile",
        "won't compile",
        "compilation error",
        "compiler error",
        "compile-time error",
    ]
    .iter()
    .any(|phrase| answer.contains(phrase));
    let immutable_result = answer.contains("iter")
        && [
            "immutable",
            "shared reference",
            "&event",
            "non-mutable",
            "cannot assign",
        ]
        .iter()
        .any(|phrase| answer.contains(phrase));
    let compile_credit = standard.scoring_criteria.iter().any(|criterion| {
        let full = criterion.full_credit.to_ascii_lowercase();
        ["compil", "immutab", "borrow", "iter"]
            .iter()
            .any(|word| full.contains(word))
    });
    let mandates_race = standard.scoring_criteria.iter().any(|criterion| {
        let full = criterion.full_credit.to_ascii_lowercase();
        [
            "identifies the race condition",
            "identifies a race condition",
            "requires a mutex",
            "must use a mutex",
            "lack of atomicity",
        ]
        .iter()
        .any(|phrase| full.contains(phrase))
            && ![
                "if ",
                "hypothetical",
                "unproven",
                "not established",
                "not demonstrated",
            ]
            .iter()
            .any(|phrase| full.contains(phrase))
    });
    json!({
        "actualCompilerFailureIdentified":compiler_error,
        "immutableIteratorResultIdentified":immutable_result,
        "compilerOrBorrowInsightCanEarnCredit":compile_credit,
        "raceDiagnosisNotMandatory":!mandates_race,
    })
}

fn all_pass(gates: &Value) -> bool {
    gates
        .as_object()
        .is_some_and(|fields| fields.values().all(|value| value == &json!(true)))
}

#[test]
fn retained_rust_counterexample_has_the_recorded_compiler_error() -> Result<()> {
    compiler_control(&fixture().question.question)?;
    Ok(())
}

#[test]
fn known_false_acceptance_cannot_pass_the_diagnosis_control() {
    assert!(!all_pass(&diagnosis_gates(
        &fixture().retained_incorrect_standard
    )));
}

#[tokio::test]
#[ignore = "contacts an explicitly selected model; web responses are recorded official excerpts"]
async fn live_research_rejects_or_corrects_the_retained_rust_counterexample() -> Result<()> {
    let (model, config, two_step) = client()?;
    create_dir_all(&config.log_dir)?;
    let fixture = fixture();
    let compiler_stderr = compiler_control(&fixture.question.question)?;
    let plan = plan(fixture.question.clone());
    let web = RecordedReferences {
        fixture,
        searches: Mutex::new(vec![]),
        fetches: Mutex::new(vec![]),
    };
    let directory = tempfile::tempdir()?;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(directory.path().join("references.sqlite"))
                .create_if_missing(true)
                .foreign_keys(true),
        )
        .await
        .map_err(|error| AppError::Database(error.to_string()))?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|error| AppError::Database(error.to_string()))?;
    let program_id = uuid::Uuid::new_v4().to_string();
    LearningRepository::new(pool.clone())
        .create(&LearningProgramDto {
            outline_review: None,
            summary: LearningProgramSummaryDto {
                id: program_id.clone(),
                title: "Rust reference control".into(),
                goal: "Test a retained counterexample.".into(),
                status: LearningProgramStatus::Draft,
                revision: 0,
                module_count: 0,
                lesson_count: 0,
                completed_lessons: 0,
                current_lesson_id: None,
                created_at: chrono::Utc::now().timestamp_millis(),
            },
            prior_knowledge: String::new(),
            minutes_per_session: 20,
            model_name: model.model_name().into(),
            modules: vec![],
            sources: vec![],
        })
        .await?;
    let library = LearningSourceLibraryRepository::new(pool.clone());
    let versions = research_interview_answers(
        model.as_ref(),
        &web,
        &library,
        &program_id,
        &uuid::Uuid::new_v4().to_string(),
        &plan,
        &[],
    )
    .await?;
    let workspace = library.workspace(&program_id).await?;
    let mut sources = Vec::new();
    let mut research_provenance = true;
    for item in workspace.sources {
        let Some(version) = item
            .active_version
            .filter(|version| versions.contains(&version.id))
        else {
            continue;
        };
        let source = library
            .get_version(&GetLearningSourceVersionRequestDto {
                program_id: program_id.clone(),
                source_id: item.id,
                version_id: version.id,
            })
            .await?;
        let saved = library
            .saved_with_digest(
                &program_id,
                &LearningSourceLibraryRepository::text_digest(&source.full_text),
            )
            .await?;
        research_provenance &= saved.is_some_and(|saved| saved.added_by == SourceAddedBy::Research);
        sources.push(source);
    }
    let result = generate_answer_guides(model.as_ref(), &plan, &sources).await;
    let searches = web
        .searches
        .lock()
        .map_err(|_| invalid("Search lock poisoned."))?
        .clone();
    let fetches = web
        .fetches
        .lock()
        .map_err(|_| invalid("Fetch lock poisoned."))?
        .clone();
    let outbound = serde_json::to_string(&searches)
        .map_err(|error| AppError::Serialization(error.to_string()))?
        .to_ascii_lowercase();
    let refused = result
        .as_ref()
        .err()
        .is_some_and(|error| error.to_string().contains("could not be verified"));
    let diagnosis = result
        .as_ref()
        .ok()
        .and_then(|guides| guides.questions.first())
        .map(|guide| diagnosis_gates(&guide.standard));
    let owned_quotes = result.as_ref().ok().is_none_or(|guides| {
        guides
            .questions
            .iter()
            .flat_map(|guide| &guide.standard.key_points)
            .flat_map(|point| &point.citations)
            .any(|citation| {
                sources.iter().any(|source| {
                    source.source_id == citation.source_id
                        && source.version.id == citation.version_id
                        && source.full_text.contains(&citation.quote)
                })
            })
    });
    let gates = json!({
        "compilerOracleReproduced":true,
        "publicRustConceptQueriesPlanned":!searches.is_empty() && ["rust","borrow","iterator"].iter().any(|term| outbound.contains(term)),
        "officialBodiesCaptured":sources.len() >= 2 && fetches.len() >= 2 && research_provenance
            && sources.iter().all(|source| web.fixture.documents.iter().any(|document| document.content == source.full_text))
            && sources.iter().any(|source| source.version.resolved_url.as_deref() == Some("https://doc.rust-lang.org/book/ch04-02-references-and-borrowing.html"))
            && sources.iter().any(|source| source.version.resolved_url.as_deref() == Some("https://doc.rust-lang.org/std/slice/struct.Iter.html")),
        "searchSnippetExcluded":sources.iter().all(|source| !source.full_text.contains(SNIPPET_CANARY)),
        "refusedOrCorrectBoundedDiagnosis":refused || diagnosis.as_ref().is_some_and(all_pass),
        "acceptedGuideUsesOwnedCapturedQuotes":owned_quotes,
    });
    let passed = all_pass(&gates);
    let record = json!({
        "harnessVersion":HARNESS_VERSION, "recordedAt":chrono::Utc::now().to_rfc3339(),
        "model":config.model, "provider":config.provider,
        "mode":if two_step {"experimental_reason_first_proxy"} else {"unchanged_production_calls"},
        "webMode":"recorded_official_document_excerpts_no_public_network",
        "passed":passed, "gates":gates, "diagnosisGates":diagnosis,
        "reviewedRefusal":refused, "error":result.as_ref().err().map(ToString::to_string),
        "question":plan.questions[0], "searches":searches, "fetches":fetches,
        "capturedVersions":sources, "guides":result.as_ref().ok(),
        "compilerStderr":compiler_stderr, "manualSemanticReviewRequired":true,
        "unassistedFailureStillRetained":true,
    });
    let path = config.log_dir.join(format!(
        "interview-reference-result-{}.json",
        std::process::id()
    ));
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&record)
            .map_err(|error| AppError::Serialization(error.to_string()))?,
    )?;
    pool.close().await;
    println!("Authoritative-reference control: {}", path.display());
    assert!(
        passed,
        "Reference control failed; inspect {}",
        path.display()
    );
    Ok(())
}
