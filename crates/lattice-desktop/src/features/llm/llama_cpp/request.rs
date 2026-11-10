//! The Chat Completions body one request becomes: messages, the output
//! reservation, sampling for the thinking mode, the thinking switch and tools.
use super::*;
use crate::features::llm::engine::vision::not_set_up_to_see;
use lattice_core::application::ports::llm_port::RequestSize;
use lattice_core::application::ports::llm_port::{tool_specs, ThinkingMode};

/// The characters a tokenizer reads in `input`: each message's role and
/// text, each call's name and arguments, each result, and opaque provider
/// state as its JSON. A picture's bytes are not characters; its tokens come
/// from the model's declared geometry (see [`CompletionInput::image_tokens`]).
pub(super) fn input_chars(input: &[CompletionInput]) -> usize {
    input
        .iter()
        .map(|item| match item {
            CompletionInput::Native { value } => value.to_string().chars().count(),
            CompletionInput::Message { role, .. } | CompletionInput::Multimodal { role, .. } => {
                role.chars().count() + item.text().chars().count()
            }
            CompletionInput::ToolCall { .. } | CompletionInput::ToolResult { .. } => {
                item.text().chars().count()
            }
        })
        .sum()
}

/// The text tokens of `input` by the rough estimate the budgeter uses: four
/// characters a token.
pub(super) fn text_tokens(input: &[CompletionInput]) -> usize {
    lattice_core::application::ports::llm_port::tokens_for_chars(
        input_chars(input),
        lattice_core::application::ports::llm_port::DEFAULT_CHARS_PER_TOKEN,
    )
}

impl LlamaCppLlm {
    /// The prompt's tokens by the rough estimate the budgeter uses: four
    /// characters a token over its text, plus each picture's declared cost.
    fn plain_prompt_tokens(input: &[CompletionInput]) -> usize {
        text_tokens(input).saturating_add(input.iter().map(CompletionInput::image_tokens).sum())
    }

    /// The mode the model runs `request` in: the user's pin first, then the
    /// model's own, then the request.
    pub(super) fn thinking_mode(&self, request: &CompletionRequest) -> ThinkingMode {
        self.config
            .generation
            .thinking_override
            .or(self.config.model_thinking)
            .mode_for(request)
    }

    /// What `request` asks of the window, for the log and for a rejection's
    /// diagnosis: the scheduler's measure when it made one, else the plain
    /// estimate.
    pub(super) fn request_size(&self, request: &CompletionRequest) -> RequestSize {
        let (prompt_tokens, counted_exactly) = match request.window_fit {
            Some(fit) => (fit.prompt_tokens, fit.counted_exactly),
            None => (Self::plain_prompt_tokens(&request.input), false),
        };
        RequestSize {
            prompt_tokens,
            counted_exactly,
            output_tokens: self.output_ceiling_for(request).unwrap_or(0),
            image_tokens: request.image_tokens(),
            image_count: request.image_count(),
        }
    }

    /// The streamed Chat Completions body for `request`, refusing tool traffic
    /// a model without a tool-call template would render as text, and
    /// pictures a model nobody declared able to see would never receive.
    pub(super) fn body(&self, request: &CompletionRequest) -> Result<Value> {
        let carries_tools = !request.tools.is_empty()
            || request.input.iter().any(|item| {
                matches!(
                    item,
                    CompletionInput::ToolCall { .. } | CompletionInput::ToolResult { .. }
                )
            });
        if carries_tools && !self.config.supports_tools {
            return Err(AppError::InvalidConfig(
                "This local model does not support tool calling".into(),
            ));
        }
        if request.has_images() && self.config.vision.is_none() {
            return Err(not_set_up_to_see(&self.config.model));
        }
        let messages = chat_messages(&request.input)?;
        let generation = &self.config.generation;
        let thinking = self.thinking_mode(request);
        // A caller's explicit override wins field by field; what it leaves
        // unset comes from the profile of the mode the model runs in.
        let profile = generation.profile(thinking);
        let sampling = request.sampling.unwrap_or_default();
        let mut body = serde_json::Map::from_iter([
            ("model".into(), json!(self.config.model)),
            ("messages".into(), json!(messages)),
            ("stream".into(), json!(true)),
            // A final usage frame, so a streamed completion reports tokens.
            ("stream_options".into(), json!({"include_usage": true})),
            // Prompt-processing events keep a long or queued prompt exempt from
            // stall detection rather than arming it, so a slow first batch is not
            // mistaken for a stalled server. Servers without support ignore the field.
            ("return_progress".into(), json!(true)),
            (
                "temperature".into(),
                json!(sampling.temperature.unwrap_or(profile.temperature)),
            ),
            (
                "top_p".into(),
                json!(sampling.top_p.unwrap_or(profile.top_p)),
            ),
            (
                "top_k".into(),
                json!(sampling.top_k.unwrap_or(profile.top_k)),
            ),
            (
                "presence_penalty".into(),
                json!(sampling
                    .presence_penalty
                    .unwrap_or(profile.presence_penalty)),
            ),
            ("repeat_penalty".into(), json!(profile.repeat_penalty)),
        ]);
        if let Some(max_tokens) = self.output_ceiling_for(request) {
            body.insert("max_tokens".into(), json!(max_tokens));
        }
        // Templates disagree on their default, and llama-server turns an
        // absent switch into `enable_thinking: true`, so it is always sent;
        // Qwen 3.5, Qwen 3.8 and Laguna templates all read it. An effort level
        // goes only to a request that asked to think and does: a template may
        // read it (Qwen 3.8 refuses anything but low to xhigh, "none" included),
        // and a level a caller set for cloud models must not ride along when a
        // pinned policy turns thinking on.
        body.insert(
            "chat_template_kwargs".into(),
            json!({"enable_thinking": thinking == ThinkingMode::On}),
        );
        if let Some(effort) = request.reasoning_effort.as_deref().filter(|effort| {
            thinking == ThinkingMode::On
                && request.thinking == ThinkingMode::On
                && *effort != "none"
        }) {
            body.insert("reasoning_effort".into(), json!(effort));
        }
        if !request.tools.is_empty() {
            body.insert("tools".into(), tool_specs(&request.tools));
            // llama-server lets a reply hold one call unless asked for more,
            // so a round that needs three pages would take three generations.
            body.insert("parallel_tool_calls".into(), json!(true));
            if let Some(choice) = request.tool_choice {
                body.insert("tool_choice".into(), json!(choice.as_str()));
            }
        }
        if !request.stop.is_empty() {
            body.insert("stop".into(), json!(request.stop));
        }
        if request.want_logprobs {
            body.insert("logprobs".into(), json!(true));
            body.insert(
                "top_logprobs".into(),
                json!(lattice_core::application::ports::llm_port::FIRST_TOKEN_TOP_LOGPROBS),
            );
        }
        if let Some(schema) = &request.json_schema {
            body.insert(
                "response_format".into(),
                json!({"type":"json_schema","json_schema":{"name":"response","schema":schema}}),
            );
        }
        // The scheduler's slot: a conversation returns to the slot whose KV
        // cache holds its prefix. The retry path may still turn reuse off.
        if let Some(slot) = request.assigned_slot {
            body.insert("id_slot".into(), json!(slot));
            body.insert("cache_prompt".into(), json!(true));
        }
        Ok(Value::Object(body))
    }
}
