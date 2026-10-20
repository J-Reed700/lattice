//! Claim-checker evaluation harness.
//!
//! Runs the grounding verifier chat ships — the lexical pre-check, then the
//! first-token logprob judge for every sentence the pre-check cannot settle —
//! over the labelled claim/passage pairs in `evals/grounding/`, one pair at a
//! time, and writes one JSON line per pair: the verdict, each judge call it
//! took, and the wall-clock latency.
//!
//! Run:
//! ```text
//! cargo run --example claim_checker_eval --release -- \
//!     [--judge-url http://127.0.0.1:8092] [--model Qwen3.5-9B-Q4_K_M.gguf] \
//!     [--context-window 4096] [--lexical-only] DATASET_JSONL OUTPUT_JSONL
//! ```
//!
//! `--lexical-only` builds the verifier with no judge, which is what a turn
//! gets when no model can be loaded; it needs no server.
//!
//! Each pair reaches the verifier the way a chat answer does: the claim as one
//! answer paragraph closed by a `[1]` marker, the passage as source 1. Nothing
//! here re-implements a verdict rule. The only code that reads the judge's
//! logprobs is the analysis below, which turns them into a continuous
//! P(supported) for the threshold sweep; the verdict itself is whatever
//! `GroundingVerifier::verify` returned.
//!
//! `GroundingVerifier::new` decodes greedily, which is exactly what the default
//! verification settings ask for (temperature 0, top-k 1), so no tuning is
//! applied. One difference from a chat turn is deliberate: the judge's
//! 90-second budget covers one pair here, where in chat it covers every claim
//! in the answer.

use async_trait::async_trait;
use lattice::features::conversation::chat::verification::GroundingVerifier;
use lattice::features::llm::llama_cpp::LlamaCppLlm;
use lattice_core::application::contracts::settings::{LLMSettingsDto, LlamaCppSettingsDto};
use lattice_core::application::contracts::sources::SourceDto;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::application::ports::LLMPort;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

type BoxError = Box<dyn std::error::Error>;

const DEFAULT_JUDGE_URL: &str = "http://127.0.0.1:8092";
const DEFAULT_MODEL: &str = "Qwen3.5-9B-Q4_K_M.gguf";
const DEFAULT_CONTEXT_WINDOW: u32 = 4096;
/// Characters of each judge reply kept in the output. Enough for the label,
/// the reason and the quote; the explanation is not what is being measured.
const REPLY_CHARS_KEPT: usize = 600;

/// One labelled pair, as `evals/grounding/claim-pairs-v1.jsonl` stores it.
#[derive(Deserialize)]
struct Pair {
    id: String,
    claim: String,
    passage: String,
    document_id: String,
    label: String,
    #[serde(default)]
    difficulty: Vec<String>,
}

struct Args {
    dataset: PathBuf,
    output: PathBuf,
    judge_url: String,
    model: String,
    context_window: u32,
    lexical_only: bool,
}

fn parse_args(raw: impl IntoIterator<Item = String>) -> Result<Args, BoxError> {
    let mut positional = Vec::new();
    let mut judge_url = DEFAULT_JUDGE_URL.to_string();
    let mut model = DEFAULT_MODEL.to_string();
    let mut context_window = DEFAULT_CONTEXT_WINDOW;
    let mut lexical_only = false;
    let mut raw = raw.into_iter();
    while let Some(arg) = raw.next() {
        match arg.as_str() {
            "--judge-url" => judge_url = raw.next().ok_or("--judge-url needs a value")?,
            "--model" => model = raw.next().ok_or("--model needs a value")?,
            "--context-window" => {
                context_window = raw
                    .next()
                    .ok_or("--context-window needs a value")?
                    .parse()?;
            }
            "--lexical-only" => lexical_only = true,
            flag if flag.starts_with("--") => return Err(format!("unknown flag {flag}").into()),
            _ => positional.push(PathBuf::from(arg)),
        }
    }
    let mut positional = positional.into_iter();
    let (Some(dataset), Some(output), None) =
        (positional.next(), positional.next(), positional.next())
    else {
        return Err("usage: claim_checker_eval [flags] DATASET_JSONL OUTPUT_JSONL".into());
    };
    Ok(Args {
        dataset,
        output,
        judge_url,
        model,
        context_window,
        lexical_only,
    })
}

/// The three labels' share of the judge's first-token distribution.
#[derive(Debug, Clone, Copy, Serialize)]
struct LabelMass {
    supported: f32,
    unsupported: f32,
    contradicted: f32,
    /// Whether the most likely first token began a label. When it did not,
    /// the shipped judge ignores these numbers and reads the reply text.
    top_token_is_label: bool,
}

/// Which label a first token begins. Mirrors the judge's prefix rule so the
/// sweep reads the same distribution the verdict was taken from: "uns" is
/// unsupported, "so" is nothing.
fn label_of_token(token: &str) -> Option<usize> {
    let word: String = token
        .chars()
        .skip_while(|c| !c.is_alphabetic())
        .take_while(|c| c.is_alphabetic())
        .flat_map(char::to_lowercase)
        .collect();
    if word.is_empty() {
        return None;
    }
    ["supported", "unsupported", "contradicted"]
        .iter()
        .position(|label| label.starts_with(&word) || word.starts_with(label))
}

fn label_mass(alternatives: &[(String, f32)]) -> Option<LabelMass> {
    let mut mass = [0.0f32; 3];
    for (token, logprob) in alternatives {
        if let Some(slot) = label_of_token(token).and_then(|index| mass.get_mut(index)) {
            *slot += logprob.exp();
        }
    }
    let total: f32 = mass.iter().sum();
    if !(total.is_finite() && total > 0.0) {
        return None;
    }
    let top_token_is_label = alternatives
        .iter()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .is_some_and(|(token, _)| label_of_token(token).is_some());
    let [supported, unsupported, contradicted] = mass.map(|p| p / total);
    Some(LabelMass {
        supported,
        unsupported,
        contradicted,
        top_token_is_label,
    })
}

/// One request the judge sent, as the server answered it.
#[derive(Debug, Clone, Serialize)]
struct JudgeCall {
    latency_ms: f64,
    finish_reason: String,
    input_tokens: u64,
    output_tokens: u64,
    /// Characters of hidden reasoning the model produced before its label.
    reasoning_chars: usize,
    reply: String,
    label_mass: Option<LabelMass>,
    error: Option<String>,
}

/// The real backend, with every completion it serves written down.
///
/// The verifier owns its judge and reports only the verdicts, so this is the
/// only place that can see whether a pair reached the model at all, how long
/// each call took and what the first token's distribution was.
struct RecordingLlm {
    inner: Arc<dyn LLMPort>,
    calls: Mutex<Vec<JudgeCall>>,
}

impl RecordingLlm {
    fn take_calls(&self) -> Vec<JudgeCall> {
        self.calls
            .lock()
            .map(|mut calls| std::mem::take(&mut *calls))
            .unwrap_or_default()
    }
}

#[async_trait]
impl LLMPort for RecordingLlm {
    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> lattice_core::shared::error::Result<CompletionResponse> {
        let started = Instant::now();
        let result = self.inner.complete(request).await;
        let latency_ms = started.elapsed().as_secs_f64() * 1000.0;
        let call = match &result {
            Ok(response) => JudgeCall {
                latency_ms,
                finish_reason: response.finish_reason.clone(),
                input_tokens: response.input_tokens,
                output_tokens: response.output_tokens,
                reasoning_chars: response
                    .reasoning
                    .as_deref()
                    .map_or(0, |text| text.chars().count()),
                reply: response.text.chars().take(REPLY_CHARS_KEPT).collect(),
                label_mass: response
                    .first_token_logprobs
                    .as_deref()
                    .and_then(label_mass),
                error: None,
            },
            Err(error) => JudgeCall {
                latency_ms,
                finish_reason: String::new(),
                input_tokens: 0,
                output_tokens: 0,
                reasoning_chars: 0,
                reply: String::new(),
                label_mass: None,
                error: Some(error.to_string()),
            },
        };
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(call);
        }
        result
    }

    fn model_name(&self) -> &str {
        self.inner.model_name()
    }

    fn max_context_tokens(&self) -> usize {
        self.inner.max_context_tokens()
    }

    fn chars_per_token(&self) -> f64 {
        self.inner.chars_per_token()
    }

    fn pace(&self) -> Option<lattice_core::application::ports::llm_port::Pace> {
        self.inner.pace()
    }

    async fn is_ready(&self) -> lattice_core::shared::error::Result<bool> {
        self.inner.is_ready().await
    }

    fn provider_name(&self) -> &str {
        self.inner.provider_name()
    }
}

/// The claim as a one-paragraph answer citing source 1.
///
/// The marker goes before the closing full stop, where chat answers put it. A
/// claim of several sentences carries one marker at the end: the lexical pass
/// gives a sentence without its own marker the next one in its paragraph, so
/// every sentence is still checked against the passage.
fn as_cited_answer(claim: &str) -> String {
    let claim = claim.trim();
    for stop in ['.', '!'] {
        if let Some(body) = claim.strip_suffix(stop) {
            return format!("{body} [1]{stop}");
        }
    }
    format!("{claim} [1]")
}

fn source_for(pair: &Pair) -> SourceDto {
    SourceDto {
        document_id: pair.document_id.clone(),
        chunk_id: format!("{}-passage", pair.id),
        content: pair.passage.clone(),
        score: 1.0,
        path: None,
        position: None,
        // The file name joins every window's tokens in the lexical pass, as a
        // retrieved document's would; the document id is the honest stand-in.
        file_name: format!("{}.md", pair.document_id),
        file_path: format!("vault/{}.md", pair.document_id),
        mime_type: "text/markdown".to_string(),
        category: "Markdown".to_string(),
        file_size_bytes: i64::try_from(pair.passage.len()).unwrap_or(i64::MAX),
        modified_at: "2026-10-12T00:00:00Z".to_string(),
        excerpt: None,
        highlights: None,
        section: None,
        chunk_index: None,
        page_number: None,
        from_ocr: false,
        chunk_excerpts: None,
        citation_id: Some(1),
        web_snapshot: None,
    }
}

/// How bad a sentence verdict is for the pair as a whole. A pair is only as
/// good as its worst sentence: one contradicted clause makes the claim wrong,
/// and an unchecked one means the claim as a whole was not established.
fn severity(verdict: &str) -> u8 {
    match verdict {
        "contradicted" => 3,
        "unsupported" => 2,
        "unverified" => 1,
        _ => 0,
    }
}

fn pair_verdict(sentences: &[Value]) -> String {
    sentences
        .iter()
        .filter_map(|sentence| sentence.get("verdict").and_then(Value::as_str))
        .max_by_key(|verdict| severity(verdict))
        .unwrap_or("no_claim")
        .to_string()
}

/// P(supported) for the pair, for the threshold sweep: the weakest judged
/// sentence, or 1/0 when the lexical pass settled everything.
fn pair_score(verdict: &str, calls: &[JudgeCall]) -> Option<f32> {
    if calls.is_empty() {
        return Some(if verdict == "supported" { 1.0 } else { 0.0 });
    }
    calls
        .iter()
        .filter_map(|call| call.label_mass.map(|mass| mass.supported))
        .min_by(f32::total_cmp)
}

fn read_pairs(path: &PathBuf) -> Result<Vec<Pair>, BoxError> {
    let file = std::fs::File::open(path)?;
    let mut pairs = Vec::new();
    for (number, line) in std::io::BufReader::new(file).lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let pair: Pair =
            serde_json::from_str(&line).map_err(|error| format!("line {}: {error}", number + 1))?;
        // The marker's scope is its line; a claim spanning two would leave
        // its first line uncited and never judged.
        if pair.claim.contains('\n') {
            return Err(format!("{}: a claim must be a single line", pair.id).into());
        }
        pairs.push(pair);
    }
    Ok(pairs)
}

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let args = parse_args(std::env::args().skip(1))?;
    let pairs = read_pairs(&args.dataset)?;

    let recorder = if args.lexical_only {
        None
    } else {
        let mut settings = LLMSettingsDto {
            context_window: args.context_window,
            ..LLMSettingsDto::default()
        };
        settings.llama_cpp = LlamaCppSettingsDto {
            url: args.judge_url.clone(),
            model: args.model.clone(),
            ..LlamaCppSettingsDto::default()
        };
        let backend: Arc<dyn LLMPort> = Arc::new(LlamaCppLlm::new(&settings)?);
        Some(Arc::new(RecordingLlm {
            inner: backend,
            calls: Mutex::new(Vec::new()),
        }))
    };
    let verifier = GroundingVerifier::new(
        recorder
            .as_ref()
            .map(|recorder| Arc::clone(recorder) as Arc<dyn LLMPort>),
    );
    let checker = if args.lexical_only {
        "lattice-lexical"
    } else {
        "lattice"
    };

    let mut output = std::io::BufWriter::new(std::fs::File::create(&args.output)?);
    let mut reached_judge = 0usize;
    let started_all = Instant::now();
    for (position, pair) in pairs.iter().enumerate() {
        let response = as_cited_answer(&pair.claim);
        let source = source_for(pair);
        let started = Instant::now();
        let report = verifier
            .verify(&response, std::slice::from_ref(&source))
            .await;
        let latency_ms = started.elapsed().as_secs_f64() * 1000.0;
        let calls = recorder
            .as_ref()
            .map(|recorder| recorder.take_calls())
            .unwrap_or_default();
        let metadata = report.metadata_json();
        let sentences: Vec<Value> = metadata
            .get("claimVerdicts")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let verdict = pair_verdict(&sentences);
        if !calls.is_empty() {
            reached_judge += 1;
        }
        let row = json!({
            "id": pair.id,
            "checker": checker,
            "label": pair.label,
            "difficulty": pair.difficulty,
            "verdict": verdict,
            "score_supported": pair_score(&verdict, &calls),
            "latency_ms": latency_ms,
            "judge_calls": calls.len(),
            "sentences": sentences,
            "calls": calls,
        });
        writeln!(output, "{row}")?;
        eprintln!(
            "[{}/{}] {} gold={} verdict={} judge_calls={} {:.0} ms",
            position + 1,
            pairs.len(),
            pair.id,
            pair.label,
            verdict,
            calls.len(),
            latency_ms
        );
    }
    output.flush()?;
    eprintln!(
        "{checker}: {} pairs in {:.1} s; {} of them reached the judge, {} never did",
        pairs.len(),
        started_all.elapsed().as_secs_f64(),
        reached_judge,
        pairs.len() - reached_judge,
    );
    Ok(())
}
