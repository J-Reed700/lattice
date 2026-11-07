//! Opt-in local-model regressions for demonstrated review failures.
//! These narrow controls do not replace the full interview authoring matrix.
use super::*;
use crate::features::llm::llama_cpp::LlamaCppLlm;
use lattice_core::application::{
    contracts::settings::{LLMSettingsDto, LlamaCppSettingsDto},
    ports::llm_port::{Pace, RequestLoad},
};
use lattice_core::domain::models::vision::VisionGeometry;
use std::{fs::OpenOptions, io::Write, path::PathBuf};

struct RecordingClient {
    inner: LlamaCppLlm,
    log: PathBuf,
}

fn client() -> Result<RecordingClient> {
    let model = std::env::var("LATTICE_LEARNING_EVAL_MODEL")
        .map_err(|_| invalid("Set LATTICE_LEARNING_EVAL_MODEL for live review controls."))?;
    if std::env::var("LATTICE_LEARNING_EVAL_PROVIDER").as_deref() != Ok("llama_cpp") {
        return Err(invalid("These local review controls require llama_cpp."));
    }
    let directory = std::env::var("LATTICE_LEARNING_EVAL_LOG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    std::fs::create_dir_all(&directory)?;
    Ok(RecordingClient {
        inner: LlamaCppLlm::new(&LLMSettingsDto {
            context_window: std::env::var("LATTICE_LEARNING_EVAL_CONTEXT_TOKENS")
                .ok()
                .and_then(|value| value.parse().ok())
                .filter(|value: &u32| *value > 0)
                .unwrap_or(32_768),
            timeout_seconds: 300,
            llama_cpp: LlamaCppSettingsDto {
                url: std::env::var("LATTICE_LEARNING_EVAL_ENDPOINT")
                    .map_err(|_| invalid("Set LATTICE_LEARNING_EVAL_ENDPOINT."))?,
                model,
                ..Default::default()
            },
            ..Default::default()
        })?,
        log: directory.join(format!(
            "interview-review-calls-{}.jsonl",
            std::process::id()
        )),
    })
}

#[async_trait::async_trait]
impl LLMPort for RecordingClient {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let response = self.inner.complete(request).await?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log)?;
        serde_json::to_writer(
            &mut file,
            &json!({
                "request":{"input":request.input,"schema":request.json_schema,"sampling":request.sampling},
                "response":{"text":response.text,"finishReason":response.finish_reason,"outputTokens":response.output_tokens},
            }),
        )?;
        file.write_all(b"\n")?;
        Ok(response)
    }
    fn model_name(&self) -> &str {
        self.inner.model_name()
    }
    fn provider_name(&self) -> &str {
        self.inner.provider_name()
    }
    fn max_context_tokens(&self) -> usize {
        self.inner.max_context_tokens()
    }
    fn chars_per_token(&self) -> f64 {
        self.inner.chars_per_token()
    }
    fn count_tokens(&self, text: &str) -> usize {
        self.inner.count_tokens(text)
    }
    async fn count_tokens_exact(&self, text: &str) -> Result<usize> {
        self.inner.count_tokens_exact(text).await
    }
    fn counts_tokens_exactly(&self) -> bool {
        self.inner.counts_tokens_exactly()
    }
    async fn is_ready(&self) -> Result<bool> {
        self.inner.is_ready().await
    }
    fn supports_tool_calling(&self) -> bool {
        self.inner.supports_tool_calling()
    }
    fn vision(&self) -> Option<VisionGeometry> {
        self.inner.vision()
    }
    fn is_alive(&self) -> bool {
        self.inner.is_alive()
    }
    fn pace(&self) -> Option<Pace> {
        self.inner.pace()
    }
    fn requests(&self) -> Option<RequestLoad> {
        self.inner.requests()
    }
}

fn audio_standard() -> (Value, AnswerStandard, Vec<LearningSourceVersionDto>) {
    let mut reference = source();
    reference.full_text = "In this hypothetical design, work that blocks the interface thread delays input handling. Progressive loading can expose metadata and previews before the complete original recording is available; playback needs buffered data and an explicit underrun policy.".into();
    reference.version.title = "Synthetic audio design reference".into();
    reference.version.excerpt = reference.full_text.clone();
    let mut guide = standard();
    guide["referenceAnswer"] = json!("Assume large recordings exceed the memory available for a single decoded buffer. Load metadata and previews first, then bounded chunks for playback. Keep expensive processing off the interface thread and bound queued work so keyboard controls can respond. Define an underrun policy, such as pausing playback until sufficient audio is buffered. Measure input-to-response delay, memory, and underruns under slow network and CPU pressure. This is an example design; other architectures meeting those outcomes are valid.");
    guide["keyPoints"] = json!([
        {"point":"Work that blocks the interface thread delays input handling.","evidence":"sourced","citations":[{"sourceId":"source","versionId":"version","quote":"In this hypothetical design, work that blocks the interface thread delays input handling."}]},
        {"point":"Progressive loading can expose metadata and previews before the complete original recording is available; playback needs buffered data and an explicit underrun policy.","evidence":"sourced","citations":[{"sourceId":"source","versionId":"version","quote":"Progressive loading can expose metadata and previews before the complete original recording is available; playback needs buffered data and an explicit underrun policy."}]}
    ]);
    guide["acceptableAlternatives"] = json!(["Any design that keeps keyboard controls responsive and explains bounded progressive loading, playback readiness and verification."]);
    guide["commonMistakes"] =
        json!(["Doing expensive work on the interface thread without accounting for input delay."]);
    guide["scoringCriteria"] = json!([
        {"criterion":"Responsiveness","fullCredit":"Explains how expensive work avoids blocking keyboard input and how responsiveness is measured.","partialCredit":"Names background processing without explaining or measuring input responsiveness."},
        {"criterion":"Loading","fullCredit":"States resource assumptions and explains progressive availability, bounded buffering and behavior when playback lacks data.","partialCredit":"Mentions chunks without addressing memory limits or playback readiness."}
    ]);
    (
        json!({"round":"system_design","question":"How would you keep keyboard controls responsive while progressively loading and processing a large audio recording? State assumptions and verification."}),
        serde_json::from_value(guide).expect("audio control"),
        vec![reference],
    )
}

fn python_standard() -> (Value, AnswerStandard, Vec<LearningSourceVersionDto>) {
    let mut guide = standard();
    guide["referenceAnswer"] = json!("The shown function returns rows 1 and 3 for the supplied input. The identity check excludes None; it does not validate numeric types. A row with value 'bad' would be retained without a type error in this function. A missing value key would raise KeyError at the dictionary lookup. No downstream code or traceback is supplied, so a downstream crash is a hypothesis. Capture the failing input shape, stage and traceback without logging sensitive payloads. Test the supplied input, None, zero, a string and a missing key; decide any additional numeric validation against the downstream schema rather than assuming one.");
    guide["keyPoints"] = json!([
        {"point":"Tracing the given input yields rows 1 and 3; None is excluded and zero would be retained.","evidence":"reasoning","citations":[]},
        {"point":"The shown identity check does not validate numeric types. A downstream crash cannot be established without downstream code and a traceback.","evidence":"reasoning","citations":[]}
    ]);
    guide["acceptableAlternatives"] = json!(["Trace the code directly or describe tests with expected results; no execution claim is required."]);
    guide["commonMistakes"] = json!(["Treating is not None as numeric type validation."]);
    guide["scoringCriteria"] = json!([
        {"criterion":"Diagnosis","fullCredit":"Gives the actual returned rows and distinguishes the None check from numeric validation.","partialCredit":"Identifies None filtering but does not establish the returned rows or type limitation."},
        {"criterion":"Evidence","fullCredit":"Requests the failing input and traceback to locate a crash and specifies boundary test expectations.","partialCredit":"Speculates about a downstream cause without evidence that locates it."}
    ]);
    guide["evidenceGaps"] = json!(["No captured language reference or downstream implementation is supplied; no code execution is claimed."]);
    (
        json!({"round":"debugging_and_code","question":"Trace this Python function on the supplied input. Does its check validate numeric types, and what evidence would locate a reported downstream crash?\n\ndef safe_import(rows):\n    return [row for row in rows if row['value'] is not None]\n\nInput: [{'id': 1, 'value': 10.5}, {'id': 2, 'value': None}, {'id': 3, 'value': 12.0}]"}),
        serde_json::from_value(guide).expect("Python control"),
        vec![],
    )
}

#[tokio::test]
#[ignore = "Requires an explicitly configured local model; records synthetic prompts and answers"]
async fn live_review_accepts_correct_claims_and_rejects_unfair_scoring() -> Result<()> {
    let llm = client()?;
    let (audio_question, audio, audio_sources) = audio_standard();
    let (python_question, python, python_sources) = python_standard();
    let mut unfair = standard();
    unfair["scoringCriteria"][0]["fullCredit"] = json!("Requires every repeated request to return an identical response body; preserving only the intended effect is insufficient for full credit.");
    let unfair: AnswerStandard = serde_json::from_value(unfair)?;
    let mut failures = Vec::new();
    for (name, question, answer, sources, should_accept) in [
        ("audio_wording", audio_question, audio, audio_sources, true),
        (
            "python_control_flow",
            python_question,
            python,
            python_sources,
            true,
        ),
        (
            "unfair_idempotency_scoring",
            json!({"round":"technical_deep_dive","question":plan().questions[0].question}),
            unfair,
            vec![source()],
            false,
        ),
    ] {
        check_standard(&answer, &sources)?;
        let evidence = EvidenceBank::gather(
            &llm,
            &sources,
            question["question"].as_str().expect("question"),
            4_000,
        )
        .await?;
        let solution = interview_answer_review::independent_solution(
            &llm,
            &question,
            &source_catalog(&llm, &sources),
            evidence.view(),
        )
        .await?;
        match interview_answer_review::review(&llm, &question, &solution, &answer, &sources).await {
            Ok(review) if review.issues.is_empty() == should_accept => {}
            result => failures.push(format!("{name}: {result:?}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}
