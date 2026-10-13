//! The Chat Completions body one request becomes: messages, the output
//! reservation, sampling for the thinking mode, the thinking switch and tools.
use super::*;
use crate::application::ports::llm_port::ThinkingMode;

impl LlamaCppLlm {
    /// What the server can still be asked to generate once this prompt is in
    /// its context.
    ///
    /// `max_tokens` is a *reservation*: llama.cpp holds prompt + n_predict in
    /// one slot, so asking for the whole window on every turn works only while
    /// prompts stay small. A turn carrying an attachment and six web pages ran
    /// 75k prompt tokens into a 163k slot, asked for 131k more on top, and the
    /// server failed the batch — a 500 forty-eight seconds in, after it had
    /// already done the work.
    ///
    /// The prompt is measured with the same rough estimate the budgeter uses,
    /// which reads low on dense text, so the estimate is inflated before it is
    /// subtracted. `MIN_OUTPUT_TOKENS` keeps a nearly-full window answerable
    /// rather than silently truncated to nothing; a prompt that leaves less
    /// room than that is over budget for reasons this clamp cannot fix.
    pub(super) fn output_room_for(&self, messages: &[Value], requested: u32) -> u32 {
        /// Estimated prompt tokens are scaled by this before being subtracted.
        const ESTIMATE_SAFETY: usize = 3;
        const ESTIMATE_SAFETY_DIVISOR: usize = 2;
        const MIN_OUTPUT_TOKENS: u32 = 1024;

        let window = self.config.context_window;
        let prompt_chars: usize = messages
            .iter()
            .map(|message| message.to_string().chars().count())
            .sum();
        let prompt_tokens = crate::application::ports::llm_port::tokens_for_chars(
            prompt_chars,
            crate::application::ports::llm_port::DEFAULT_CHARS_PER_TOKEN,
        ) * ESTIMATE_SAFETY
            / ESTIMATE_SAFETY_DIVISOR;
        let room = u32::try_from(window.saturating_sub(prompt_tokens)).unwrap_or(u32::MAX);
        // The floor guards against the window clamp, never against a cap
        // the caller asked for: a 512-token verdict stays 512.
        let allowed = requested.min(room.max(MIN_OUTPUT_TOKENS));
        if allowed < requested {
            tracing::debug!(
                requested,
                allowed,
                prompt_tokens,
                window,
                "Capped the llama.cpp output reservation to what the window has left"
            );
        }
        allowed
    }

    /// The streamed Chat Completions body for `request`, refusing tool traffic
    /// a model without a tool-call template would render as text.
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
        let messages = chat_messages(&request.input)?;
        let generation = &self.config.generation;
        let configured_output = u32::try_from(generation.max_tokens).unwrap_or(u32::MAX);
        let requested_output = request.effective_max_output_tokens(configured_output);
        let max_tokens = self.output_room_for(&messages, requested_output);
        // The user's pin first, then the model's own, then the request.
        let thinking = generation
            .thinking_override
            .or(self.config.model_thinking)
            .mode_for(request);
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
            ("max_tokens".into(), json!(max_tokens)),
        ]);
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
        if request.want_logprobs {
            body.insert("logprobs".into(), json!(true));
            body.insert(
                "top_logprobs".into(),
                json!(crate::application::ports::llm_port::FIRST_TOKEN_TOP_LOGPROBS),
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
