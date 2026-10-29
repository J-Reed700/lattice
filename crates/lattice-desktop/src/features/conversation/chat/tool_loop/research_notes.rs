//! Condensing what a turn has read, so it can keep reading. When tool results
//! fill the window, the model rewrites everything the loop has added into one
//! notes message that keeps every fact, figure, address and citation number.
//! The latest round is condensed too, so a hybrid model's cache is not cut
//! mid-way. Sources, citation numbers and step cards live outside the
//! transcript, so an answer written after condensing cites as one written
//! before. Done at most twice a turn.

use super::clock::TurnClock;
use super::final_round::written_call_start;
use crate::features::conversation::chat::cancellation::cancelled_error;
use crate::features::conversation::chat::turn_record::{TurnRecorder, TurnStepKind};
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse, SamplingOverride, ThinkingMode,
    ToolChoice,
};
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::Result;
use std::collections::HashSet;
use std::ops::Range;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// Share of the window the loop's results may fill before they are condensed.
/// Condensing before the limit leaves the next round room for whole pages.
pub(super) const CONDENSE_AT_WINDOW_SHARE: f64 = 0.6;

/// Each condensing makes the next round re-read the transcript after the
/// planned request. Twice is room for a long investigation; more would spend
/// the turn re-reading its own notes.
pub(super) const MAX_CONDENSES_PER_TURN: usize = 2;

/// The request for the notes, said last.
pub(super) const CONDENSE_INSTRUCTION: &str = "Your context window is nearly full. Before you continue, rewrite all the tool results you have so far into compact research notes. Keep every fact, number, date, name, short quotation and URL that could matter to my question, and keep each citation number exactly as it is written, for example [3], next to the facts it supports. Drop navigation text, boilerplate, repetition and anything that does not bear on my question. Group the notes by source, one short line per fact. Do not answer my question yet and do not call any tools: write only the notes.";

/// Opens the message the notes replace the results with. Also how a later
/// condensing recognises the notes as results.
pub(super) const NOTES_HEADER: &str = "Research notes so far. These condense the tool results you gathered earlier in this turn, to make room in your context window. Every citation number in them still refers to the same source. Continue the research from them, or answer if they are enough.";

/// What one condensing did.
pub(super) struct Condensed {
    /// The tool results it replaced, by call id.
    pub(super) result_ids: HashSet<String>,
    pub(super) chars_before: usize,
    pub(super) chars_after: usize,
}

/// Characters of results the loop holds: every tool result after the planned
/// request, and notes standing in for earlier ones.
pub(super) fn loop_result_chars(input: &[CompletionInput], loop_start: usize) -> usize {
    input
        .get(loop_start..)
        .unwrap_or_default()
        .iter()
        .map(result_chars)
        .sum()
}

fn result_chars(item: &CompletionInput) -> usize {
    match item {
        CompletionInput::ToolResult { output, .. } => output.chars().count(),
        CompletionInput::Message { content, .. } if content.starts_with(NOTES_HEADER) => {
            content.chars().count()
        }
        _ => 0,
    }
}

fn holds_results(item: &CompletionInput) -> bool {
    match item {
        CompletionInput::ToolResult { .. } => true,
        CompletionInput::Message { content, .. } => content.starts_with(NOTES_HEADER),
        _ => false,
    }
}

/// Whether the loop's results have outgrown their share of the window, with
/// a condensing left to spend. A provider that reports no window is never
/// full.
pub(super) fn should_condense(
    window_chars: usize,
    input: &[CompletionInput],
    loop_start: usize,
    condensed_so_far: usize,
) -> bool {
    window_chars > 0
        && condensed_so_far < MAX_CONDENSES_PER_TURN
        && loop_result_chars(input, loop_start) as f64
            > window_chars as f64 * CONDENSE_AT_WINDOW_SHARE
}

/// What to condense: everything after the planned request, if it holds any
/// results.
fn condensable_span(input: &[CompletionInput], loop_start: usize) -> Option<Range<usize>> {
    let span = loop_start..input.len();
    input
        .get(span.clone())
        .is_some_and(|items| items.iter().any(holds_results))
        .then_some(span)
}

/// Rewrites the loop's results so far into one notes message. `Ok(None)` when
/// the notes are unusable or no shorter than what they replace; only Stop and
/// the turn's deadline are errors.
pub(super) async fn condense(
    llm: &dyn LLMPort,
    request: &mut CompletionRequest,
    loop_start: usize,
    recorder: &TurnRecorder,
    cancel: &CancellationToken,
    clock: &TurnClock,
) -> Result<Option<Condensed>> {
    let Some(span) = condensable_span(&request.input, loop_start) else {
        return Ok(None);
    };
    let condensable = request.input.get(span.clone()).unwrap_or_default();
    let chars_before = condensable.iter().map(result_chars).sum();
    let step = recorder.begin_guarded(
        TurnStepKind::Generate,
        "Condensing what it has read",
        Some(format!("{chars_before} characters of results")),
    );
    // In a message of its own: read as more of the last result, the
    // instruction got calls back from Qwen and dropped facts from Claude.
    let mut input = request.input.clone();
    input.push(CompletionInput::Message {
        role: "user".into(),
        content: CONDENSE_INSTRUCTION.into(),
    });
    let notes_request = CompletionRequest {
        input,
        // The same tools, forbidden. Without them the prompt would differ from
        // its first token and the server would read it all again; with them
        // this request begins with the prompt the server has cached.
        tools: request.tools.clone(),
        tool_choice: (!request.tools.is_empty()).then_some(ToolChoice::None),
        // Notes are a transcription, not a composition: the same results
        // should condense the same way every time.
        sampling: Some(SamplingOverride::deterministic()),
        // The round's own reservation: the request is the next round's prompt
        // and a few lines more, and the window's planned slack covers those.
        max_output_tokens: request.max_output_tokens,
        priority: request.priority,
        cancel: request.cancel.clone(),
        cache_key: request.cache_key.clone(),
        // A transcription: thinking would only spend the round's time.
        thinking: ThinkingMode::Off,
        ..Default::default()
    };
    let Some(response) = ask(llm, &notes_request, cancel, clock).await? else {
        step.failed(Some("The model could not write notes".into()));
        return Ok(None);
    };
    let Some(notes) = usable_notes(&response.text, &response.finish_reason) else {
        warn!(
            finish_reason = %response.finish_reason,
            "The model's notes were not usable; carrying on without condensing"
        );
        step.failed(Some("No usable notes".into()));
        return Ok(None);
    };
    let message = format!("{NOTES_HEADER}\n\n{notes}");
    let chars_after = message.chars().count();
    if chars_after >= chars_before {
        warn!(
            chars_before,
            chars_after, "Notes were no shorter than the results; carrying on without condensing"
        );
        step.failed(Some("The notes saved no room".into()));
        return Ok(None);
    }
    let result_ids = request
        .input
        .get(span.clone())
        .unwrap_or_default()
        .iter()
        .filter_map(|item| match item {
            CompletionInput::ToolResult { id, .. } => Some(id.clone()),
            _ => None,
        })
        .collect();
    request.input.splice(
        span,
        [CompletionInput::Message {
            role: "user".into(),
            content: message,
        }],
    );
    info!(
        chars_before,
        chars_after, "Condensed the turn's tool results into notes"
    );
    step.done(Some(format!("{chars_before} → {chars_after} characters")));
    Ok(Some(Condensed {
        result_ids,
        chars_before,
        chars_after,
    }))
}

/// The model's reply to `request` within the turn's time; `None` when the
/// provider fails it. Only Stop and the turn's deadline are errors.
async fn ask(
    llm: &dyn LLMPort,
    request: &CompletionRequest,
    cancel: &CancellationToken,
    clock: &TurnClock,
) -> Result<Option<CompletionResponse>> {
    let remaining = clock.remaining()?;
    let request = CompletionRequest {
        time_budget: Some(remaining),
        ..request.clone()
    };
    let response = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(cancelled_error()),
        result = timeout(remaining, llm.complete(&request)) => result.map_err(|_| clock.exhausted())?,
    };
    match response {
        Ok(response) => Ok(Some(response)),
        Err(_) if cancel.is_cancelled() => Err(cancelled_error()),
        Err(error) => {
            warn!(%error, "Could not condense the turn's tool results; carrying on without");
            Ok(None)
        }
    }
}

/// The notes, if they can stand in for the results: something written, not
/// cut off at the output limit (the facts at the end would be lost without a
/// trace), and not a tool call.
fn usable_notes(text: &str, finish_reason: &str) -> Option<String> {
    if matches!(
        finish_reason,
        "length" | "max_tokens" | "incomplete" | "failed"
    ) {
        return None;
    }
    let text = text.trim();
    let notes = match written_call_start(text) {
        Some(start) => text[..start].trim(),
        None => text,
    };
    (!notes.is_empty()).then(|| notes.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(role: &str, content: &str) -> CompletionInput {
        CompletionInput::Message {
            role: role.into(),
            content: content.into(),
        }
    }

    fn call(id: &str) -> CompletionInput {
        CompletionInput::ToolCall {
            id: id.into(),
            name: "fetch_url_content".into(),
            arguments: serde_json::json!({ "url": format!("https://example.test/{id}") }),
        }
    }

    fn result(id: &str, chars: usize) -> CompletionInput {
        CompletionInput::ToolResult {
            id: id.into(),
            output: "r".repeat(chars),
        }
    }

    fn transcript(rounds: &[&[usize]]) -> Vec<CompletionInput> {
        let mut input = vec![message("system", "policy"), message("user", "question")];
        for (round, results) in rounds.iter().enumerate() {
            for index in 0..results.len() {
                input.push(call(&format!("r{round}c{index}")));
            }
            for (index, chars) in results.iter().enumerate() {
                input.push(result(&format!("r{round}c{index}"), *chars));
            }
        }
        input
    }

    #[test]
    fn the_results_count_and_the_planned_request_does_not() {
        let input = transcript(&[&[100, 200], &[300]]);
        assert_eq!(loop_result_chars(&input, 2), 600);
        let mut with_notes = input.clone();
        with_notes.insert(2, message("user", &format!("{NOTES_HEADER}\n\nfacts")));
        assert_eq!(
            loop_result_chars(&with_notes, 2),
            600 + NOTES_HEADER.len() + 7
        );
    }

    #[test]
    fn it_condenses_past_the_share_and_at_most_twice() {
        let input = transcript(&[&[7_000]]);
        assert!(should_condense(10_000, &input, 2, 0));
        assert!(should_condense(10_000, &input, 2, 1));
        assert!(!should_condense(10_000, &input, 2, MAX_CONDENSES_PER_TURN));
        assert!(!should_condense(20_000, &input, 2, 0), "under the share");
        assert!(!should_condense(0, &input, 2, 0), "no window reported");
    }

    #[test]
    fn everything_after_the_planned_request_is_condensed_earlier_notes_included() {
        let mut input = transcript(&[&[3_000], &[2_000, 1_000]]);
        assert_eq!(condensable_span(&input, 2), Some(2..input.len()));
        input.splice(
            2..4,
            [message(
                "user",
                &format!("{NOTES_HEADER}\n\n{}", "n".repeat(400)),
            )],
        );
        assert_eq!(condensable_span(&input, 2), Some(2..input.len()));
    }

    #[test]
    fn nothing_to_condense_without_results() {
        let input = transcript(&[]);
        assert_eq!(condensable_span(&input, 2), None);
    }

    /// Answers from a script, keeping every request it was sent.
    struct Script {
        replies: std::sync::Mutex<std::collections::VecDeque<CompletionResponse>>,
        requests: std::sync::Mutex<Vec<CompletionRequest>>,
    }

    impl Script {
        fn new(replies: impl IntoIterator<Item = (&'static str, &'static str)>) -> Self {
            Self {
                replies: std::sync::Mutex::new(
                    replies
                        .into_iter()
                        .map(|(text, finish_reason)| CompletionResponse {
                            finish_reason: finish_reason.into(),
                            ..CompletionResponse::from_text(text)
                        })
                        .collect(),
                ),
                requests: std::sync::Mutex::default(),
            }
        }

        fn requests(&self) -> Vec<CompletionRequest> {
            self.requests.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl LLMPort for Script {
        async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
            self.requests.lock().unwrap().push(request.clone());
            let reply = self.replies.lock().unwrap().pop_front();
            Ok(reply.unwrap_or_else(|| CompletionResponse::from_text("")))
        }
        fn model_name(&self) -> &str {
            "scripted"
        }
        fn max_context_tokens(&self) -> usize {
            32_768
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }

    async fn condensed(model: &Script) -> Option<Condensed> {
        let mut request = CompletionRequest {
            input: transcript(&[&[3_000]]),
            ..Default::default()
        };
        condense(
            model,
            &mut request,
            2,
            &TurnRecorder::silent("conversation", "request"),
            &CancellationToken::new(),
            &TurnClock::new(
                std::time::Duration::from_secs(60),
                std::time::Duration::ZERO,
            ),
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn the_notes_are_asked_for_in_a_message_of_their_own() {
        let model = Script::new([("- fact [1]", "stop")]);
        let condensed = condensed(&model).await.expect("notes");
        assert_eq!(condensed.chars_before, 3_000);
        let sent = model.requests();
        assert_eq!(sent.len(), 1);
        let results_end = transcript(&[&[3_000]]).len();
        assert_eq!(sent[0].input.len(), results_end + 1);
        assert!(matches!(
            &sent[0].input[results_end - 1],
            CompletionInput::ToolResult { output, .. } if output.len() == 3_000
        ));
        assert!(matches!(
            sent[0].input.last(),
            Some(CompletionInput::Message { role, content })
                if role == "user" && content == CONDENSE_INSTRUCTION
        ));
    }

    #[tokio::test]
    async fn notes_written_as_a_call_are_not_asked_for_again() {
        let model = Script::new([("<tool_call>\n<function=web_search>", "stop")]);
        assert!(condensed(&model).await.is_none());
        assert_eq!(model.requests().len(), 1);
    }

    #[tokio::test]
    async fn notes_cut_off_are_not_asked_for_again() {
        let model = Script::new([("- fact [1]", "length")]);
        assert!(condensed(&model).await.is_none());
        assert_eq!(model.requests().len(), 1);
    }

    #[test]
    fn notes_cut_off_or_written_as_a_call_are_refused() {
        assert_eq!(
            usable_notes("- fact [1]", "stop").as_deref(),
            Some("- fact [1]")
        );
        assert_eq!(usable_notes("- fact [1]", "length"), None);
        assert_eq!(usable_notes("   ", "stop"), None);
        assert_eq!(
            usable_notes(r#"{"name": "web_search", "arguments": {}}"#, "stop"),
            None
        );
        assert_eq!(
            usable_notes("- fact [1]\n<tool_call>{}</tool_call>", "stop").as_deref(),
            Some("- fact [1]")
        );
    }
}
