use super::*;
use crate::features::learning::assessment_engine::LearningBlueprintRequirement;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// Counts exactly at two characters a token where the estimate assumes
/// four, as on reaction notation, and refuses, as the scheduler's gate does,
/// a prompt its exact count says fills the window.
struct Notation {
    prompts: Mutex<Vec<Value>>,
    refusals: AtomicUsize,
}

fn notation() -> Notation {
    Notation {
        prompts: Mutex::new(Vec::new()),
        refusals: AtomicUsize::new(0),
    }
}

const WINDOW: usize = 8_192;

fn exact(text: &str) -> usize {
    text.chars().count() / 2
}

#[async_trait::async_trait]
impl LLMPort for Notation {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let rendered = request
            .input
            .iter()
            .flat_map(|item| match item {
                CompletionInput::Message { role, content } => vec![role.clone(), content.clone()],
                _ => Vec::new(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        if exact(&rendered) >= WINDOW {
            self.refusals.fetch_add(1, Ordering::SeqCst);
            return Err(AppError::PromptExceedsWindow {
                prompt_tokens: exact(&rendered),
                window: WINDOW,
                counted_exactly: true,
                message: "over the window".into(),
            });
        }
        let (_, user) = crate::features::learning::lessons::structured_call::task_messages(request);
        if let Ok(mut prompts) = self.prompts.lock() {
            prompts.push(serde_json::from_str(user)?);
        }
        Ok(CompletionResponse::from_text("{\"items\":[]}"))
    }
    fn model_name(&self) -> &str {
        "notation"
    }
    fn max_context_tokens(&self) -> usize {
        WINDOW
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.chars().count() / 4
    }
    async fn count_tokens_exact(&self, text: &str) -> Result<usize> {
        Ok(exact(text))
    }
    fn counts_tokens_exactly(&self) -> bool {
        true
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// About 1,100 characters of reaction notation.
fn reaction_notes(i: usize) -> String {
    (0..11)
        .map(|k| format!("In trial {i}-{k}, iron oxide reduction Fe₂O₃ + 3CO → 2Fe + 3CO₂ reaches equilibrium when ΔG° falls to zero."))
        .collect::<Vec<_>>()
        .join(" ")
}

fn version(i: usize) -> LearningSourceVersionDto {
    LearningSourceVersionDto {
        source_id: format!("source-{i}"),
        version: LearningSourceVersionSummaryDto {
            id: format!("version-{i}"),
            version_number: 1,
            title: format!("Run {i}"),
            publisher: None,
            resolved_url: None,
            excerpt: reaction_notes(i),
            content_sha256: String::new(),
            word_count: 0,
            truncated: false,
            extraction_version: "fixture".into(),
            acquired_at: 1,
            snapshot_hash: None,
        },
        full_text: reaction_notes(i),
        usage: vec![],
    }
}

fn blueprint(instructions: String) -> CreateLearningAssessmentBlueprintRequestDto {
    CreateLearningAssessmentBlueprintRequestDto {
        operation_id: "op".into(),
        program_id: "program".into(),
        blueprint_id: "blueprint".into(),
        revision: 1,
        predecessor_revision: None,
        purpose: LearningAssessmentPurpose::ModuleTest,
        title: "Iron oxide reduction".into(),
        instructions,
        expected_minutes: 30,
        allowed_aids: vec![],
        passing_score: 0.7,
        feedback_timing: LearningFeedbackTiming::AfterSubmission,
        rubric: vec![],
        source_version_ids: vec![],
        requirements: vec![LearningBlueprintRequirement {
            outcome_id: "reduction".into(),
            format: LearningItemFormat::Explanation,
            count: 1,
            difficulty_min: 2,
            difficulty_max: 3,
        }],
        change_reason: "Initial".into(),
    }
}

fn documents(sources: &[LearningSourceVersionDto]) -> Vec<LearningSourceDto> {
    sources
        .iter()
        .map(|source| LearningSourceDto {
            id: source.version.id.clone(),
            title: source.version.title.clone(),
            url: None,
            excerpt: source.full_text.clone(),
            acquired_at: 0,
        })
        .collect()
}

#[tokio::test]
async fn passages_twice_as_dense_as_estimated_are_packed_until_the_request_fits() -> Result<()> {
    let sources: Vec<_> = (0..16).map(version).collect();
    let documents = documents(&sources);
    let collection = ReferenceCollection::lexical(&documents)?;
    let request = blueprint("Explain iron oxide reduction and its equilibrium.".into());
    let batch = Batch::new(&request, &[], &[], 0, 1, sources.len());
    let llm = notation();

    let sent = batch.send(&llm, &collection, &sources).await?;

    assert!(sent.is_some());
    assert_eq!(llm.refusals.load(Ordering::SeqCst), 0);
    let prompts = llm
        .prompts
        .lock()
        .map_err(|_| AppError::InternalError("lock".into()))?;
    assert_eq!(prompts.len(), 1);
    let carried: usize = prompts
        .first()
        .and_then(|prompt| prompt["sources"].as_array())
        .into_iter()
        .flatten()
        .map(|source| source["passages"].as_array().map_or(0, Vec::len))
        .sum();
    assert!(carried > 0 && carried < sources.len(), "{carried} passages");
    Ok(())
}

#[tokio::test]
async fn instructions_the_window_cannot_hold_by_exact_count_name_the_window() {
    let sources: Vec<_> = (0..4).map(version).collect();
    // Within the window by the estimate, twice over it by the exact count.
    let instructions = format!(
        "Explain iron oxide reduction. {}",
        "Fe₂O₃ ⇌ 2Fe³⁺ + 3O²⁻ ".repeat(800)
    );
    let llm = notation();
    let error = generate(&llm, &blueprint(instructions), &[], &sources, &[])
        .await
        .err();
    let Some(AppError::InvalidConfig(message)) = error else {
        panic!("expected the window to be named, got {error:?}");
    };
    assert!(
        message.contains("notation's 8192-token context window"),
        "{message}"
    );
    assert!(llm.prompts.lock().is_ok_and(|prompts| prompts.is_empty()));
    assert_eq!(llm.refusals.load(Ordering::SeqCst), 0);
}
