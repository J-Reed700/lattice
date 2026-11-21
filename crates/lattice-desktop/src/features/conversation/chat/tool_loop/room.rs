//! How much of the window the loop has left, in characters: what the prompt
//! already carries, what one more result may take, and when there is no room
//! for another.

use super::tool_schema_text;
use lattice_core::application::ports::llm_port::{
    chars_within_tokens, CompletionInput, CompletionRequest, DEFAULT_CHARS_PER_TOKEN,
    TEMPLATE_TOKENS_FOR_ANSWER, TEMPLATE_TOKENS_PER_MESSAGE,
};
use lattice_core::application::ports::LLMPort;
use std::borrow::Cow;
use std::collections::HashSet;
use tracing::debug;

/// The characters the prompt may fill this turn: the plan's input budget at a
/// characters-per-token rate that starts at the backend's calibrated average
/// and is replaced by every count of this turn's prompt, since dense scripts
/// such as Chinese run nearer one character a token than the average.
#[derive(Debug, Clone, Copy)]
pub(super) struct Window {
    input_budget: usize,
    chars_per_token: f64,
}

impl Window {
    pub(super) fn new(input_budget: usize, chars_per_token: f64) -> Self {
        Self {
            input_budget,
            chars_per_token,
        }
    }

    /// What the prompt may fill, erring short (see [`window_chars`]). Zero
    /// when the provider reports no window.
    pub(super) fn chars(&self) -> usize {
        window_chars(self.input_budget, self.chars_per_token)
    }

    /// The tokens the prompt may fill.
    pub(super) fn tokens(&self) -> usize {
        self.input_budget
    }

    /// About how many characters `tokens` of this turn's prompt take.
    pub(super) fn chars_in(&self, tokens: usize) -> usize {
        (tokens as f64 * self.chars_per_token).ceil() as usize
    }

    /// Fold in a count of this turn's prompt: `chars` characters, as
    /// [`chars_in_flight`] measures them, that the backend counted as
    /// `tokens`. The count includes the template and the tool schemas as the
    /// server renders them, so the rate errs short.
    pub(super) fn measured(&mut self, chars: usize, tokens: usize) {
        if chars > 0 && tokens > 0 {
            self.chars_per_token = chars as f64 / tokens as f64;
        }
    }
}

/// Whether another tool result could still fit in the window, at the size
/// below which a result says too little to be worth a round. A provider that
/// reports no window is never treated as full.
pub(super) fn window_has_room_for_a_result(window_chars: usize, chars_in_flight: usize) -> bool {
    if window_chars == 0 {
        return true;
    }
    window_chars.saturating_sub(chars_in_flight) >= MIN_TOOL_RESULT_CHARS
}

/// The characters the prompt may fill: the plan's input budget, converted at
/// the backend's calibrated characters per token and erring short, so dense
/// text under-fills rather than overruns.
pub(super) fn window_chars(input_budget: usize, chars_per_token: f64) -> usize {
    chars_within_tokens(input_budget, chars_per_token)
}

/// A result cut shorter than this says too little to have been worth the round
/// that asked for it, so a nearly full window still gets this much.
pub(super) const MIN_TOOL_RESULT_CHARS: usize = 1_500;

/// Roughly how many characters the next generation already has to read. A
/// picture counts as the characters its declared tokens stand for, never as
/// its bytes.
pub(super) fn chars_in_flight(input: &[CompletionInput]) -> usize {
    input
        .iter()
        .map(|item| match item {
            CompletionInput::Message { content, .. } => content.chars().count(),
            CompletionInput::Multimodal { .. } => {
                item.text().chars().count()
                    + (item.image_tokens() as f64 * DEFAULT_CHARS_PER_TOKEN) as usize
            }
            CompletionInput::ToolResult { output, .. } => output.chars().count(),
            CompletionInput::ToolCall {
                name, arguments, ..
            } => name.len() + arguments.to_string().len(),
            CompletionInput::Native { value } => value.to_string().len(),
        })
        .sum()
}

/// How many characters one tool result may take, given the window, what is
/// already in it, and how many results this round still has to fit. The window
/// alone decides; a longer result is shown in part (see `format_tool_result`).
/// One share is always kept back so the next round has room to read with.
pub(super) fn tool_result_allowance(
    window_chars: usize,
    chars_in_flight: usize,
    calls_left: usize,
) -> usize {
    let room = window_chars.saturating_sub(chars_in_flight);
    let share = room / calls_left.saturating_add(1).max(2);
    // The floor is only kept while the window can still take it. Past that, a
    // result is given its share of what is left and no more: overrunning the
    // window fails the next round outright, which is worse than a short result.
    if room < MIN_TOOL_RESULT_CHARS {
        return share;
    }
    share.max(MIN_TOOL_RESULT_CHARS)
}

/// Said in place of a result taken out of a prompt that no longer fit.
pub(super) const SHED_RESULT: &str = "This result was taken out: with it, the prompt no longer fit in the context window. Answer from the results that are still here, and say so if the answer needed this one.";

/// Takes the newest tool results after `loop_start` out of `input` until about
/// `excess` characters are freed, leaving [`SHED_RESULT`] in each one's place
/// so every call keeps an answer. Returns the shed ids; once every result is
/// shed the prompt is the planned request, which fits.
pub(super) fn shed_newest_results(
    input: &mut [CompletionInput],
    loop_start: usize,
    excess: usize,
) -> HashSet<String> {
    let note = SHED_RESULT.chars().count();
    let mut freed = 0;
    let mut shed = HashSet::new();
    for item in input.iter_mut().skip(loop_start).rev() {
        if !shed.is_empty() && freed >= excess {
            break;
        }
        if let CompletionInput::ToolResult { id, output } = item {
            let chars = output.chars().count();
            if chars > note {
                freed += chars - note;
                *output = SHED_RESULT.to_string();
                shed.insert(id.clone());
            }
        }
    }
    shed
}

/// `request`'s prompt in tokens, counted as the send-time gates count it.
pub(super) async fn prompt_tokens(llm: &dyn LLMPort, request: &CompletionRequest) -> usize {
    let text = request
        .input()
        .iter()
        .flat_map(|item| match item {
            CompletionInput::Native { value } => vec![Cow::Owned(value.to_string())],
            CompletionInput::Message { role, .. } | CompletionInput::Multimodal { role, .. } => {
                vec![Cow::Borrowed(role.as_str()), item.text()]
            }
            _ => vec![item.text()],
        })
        .chain((!request.tools().is_empty()).then(|| Cow::Owned(tool_schema_text(request.tools()))))
        .collect::<Vec<_>>()
        .join("\n");
    tokens(llm, &text).await
        + TEMPLATE_TOKENS_PER_MESSAGE * request.input().len()
        + TEMPLATE_TOKENS_FOR_ANSWER
        + request.image_tokens()
}

/// Tokens in `text` as the backend's own tokenizer counts them where it has
/// one, by its estimate otherwise or when the count fails.
async fn tokens(llm: &dyn LLMPort, text: &str) -> usize {
    if !llm.counts_tokens_exactly() {
        return llm.count_tokens(text);
    }
    match llm.count_tokens_exact(text).await {
        Ok(tokens) => tokens,
        Err(error) => {
            debug!(%error, "The backend could not count the text; measuring by the estimate");
            llm.count_tokens(text)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::application::ports::llm_port::CompletionResponse;
    use lattice_core::application::ports::ToolDefinition;
    use lattice_core::application::services::model_contract::legacy;
    use lattice_core::application::services::model_contract::shape::Schema;
    use lattice_core::shared::error::{AppError, Result};
    use serde_json::json;

    struct FakeTokenizer {
        exact_supported: bool,
        exact_result: std::result::Result<usize, String>,
        estimate: usize,
    }

    #[async_trait::async_trait]
    impl LLMPort for FakeTokenizer {
        async fn complete(&self, _request: &CompletionRequest) -> Result<CompletionResponse> {
            unreachable!("not used in prompt_tokens tests")
        }

        fn model_name(&self) -> &str {
            "fake-tokenizer"
        }

        fn max_context_tokens(&self) -> usize {
            8_192
        }

        fn count_tokens(&self, _text: &str) -> usize {
            self.estimate
        }

        async fn count_tokens_exact(&self, _text: &str) -> Result<usize> {
            self.exact_result
                .clone()
                .map_err(AppError::ServiceNotAvailable)
        }

        fn counts_tokens_exactly(&self) -> bool {
            self.exact_supported
        }

        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }

        fn enforces(&self, _schema: &Schema) -> bool {
            false
        }
    }

    fn sample_request() -> CompletionRequest {
        legacy::completion(legacy::Completion {
            input: vec![
                CompletionInput::Message {
                    role: "system".into(),
                    content: "Answer carefully.".into(),
                },
                CompletionInput::Message {
                    role: "user".into(),
                    content: "When did the bridge open?".into(),
                },
            ],
            tools: vec![ToolDefinition {
                name: "search_documents".into(),
                description: "Search.".into(),
                parameters: json!({"type": "object", "properties": {}}),
            }],
            ..Default::default()
        })
    }

    #[tokio::test]
    async fn prompt_tokens_uses_exact_count_and_template_overhead() {
        let llm = FakeTokenizer {
            exact_supported: true,
            exact_result: Ok(42),
            estimate: 99,
        };
        let request = sample_request();
        let expected = 42 + TEMPLATE_TOKENS_PER_MESSAGE * 2 + TEMPLATE_TOKENS_FOR_ANSWER;
        assert_eq!(prompt_tokens(&llm, &request).await, expected);
    }

    #[tokio::test]
    async fn prompt_tokens_falls_back_to_estimate_when_exact_count_fails() {
        let llm = FakeTokenizer {
            exact_supported: true,
            exact_result: Err("tokenizer unavailable".into()),
            estimate: 77,
        };
        let request = sample_request();
        let expected = 77 + TEMPLATE_TOKENS_PER_MESSAGE * 2 + TEMPLATE_TOKENS_FOR_ANSWER;
        assert_eq!(prompt_tokens(&llm, &request).await, expected);
    }

    #[tokio::test]
    async fn prompt_tokens_uses_estimate_when_exact_counting_is_unsupported() {
        let llm = FakeTokenizer {
            exact_supported: false,
            exact_result: Ok(999),
            estimate: 31,
        };
        let request = sample_request();
        let expected = 31 + TEMPLATE_TOKENS_PER_MESSAGE * 2 + TEMPLATE_TOKENS_FOR_ANSWER;
        assert_eq!(prompt_tokens(&llm, &request).await, expected);
    }

    #[tokio::test]
    async fn prompt_tokens_accounts_for_message_and_answer_template_overhead() {
        let llm = FakeTokenizer {
            exact_supported: true,
            exact_result: Ok(10),
            estimate: 10,
        };
        let empty = legacy::completion(legacy::Completion::default());
        assert_eq!(
            prompt_tokens(&llm, &empty).await,
            10 + TEMPLATE_TOKENS_FOR_ANSWER
        );

        let three_messages = legacy::completion(legacy::Completion {
            input: vec![
                CompletionInput::Message {
                    role: "system".into(),
                    content: "a".into(),
                },
                CompletionInput::Message {
                    role: "user".into(),
                    content: "b".into(),
                },
                CompletionInput::ToolResult {
                    id: "c1".into(),
                    output: "c".into(),
                },
            ],
            ..Default::default()
        });
        assert_eq!(
            prompt_tokens(&llm, &three_messages).await,
            10 + TEMPLATE_TOKENS_PER_MESSAGE * 3 + TEMPLATE_TOKENS_FOR_ANSWER
        );
    }
}
