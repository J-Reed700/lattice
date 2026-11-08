//! The Chat Completions body one request becomes: messages, the output
//! reservation, sampling for the thinking mode, the thinking switch and tools.
use super::*;
use crate::features::llm::engine::vision::not_set_up_to_see;
use crate::features::llm::scheduler::rejection::{window_refusal, RequestSize};
use lattice_core::application::ports::llm_port::{
    tool_specs, ThinkingMode, TEMPLATE_TOKENS_FOR_ANSWER, TEMPLATE_TOKENS_PER_MESSAGE,
};
use std::borrow::Cow;

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

    /// [`Self::plain_prompt_tokens`] scaled by 3/2, for sizing the answer's
    /// reservation only: four characters a token reads low on code, JSON and
    /// most non-English text, and a low count overruns the window. Pictures are
    /// added at their declared cost. It never refuses a request on its own (see
    /// [`Self::measured_body`]).
    fn estimated_prompt_tokens(input: &[CompletionInput]) -> usize {
        /// The margin: estimated prompt tokens are scaled by this fraction.
        const ESTIMATE_SAFETY: usize = 3;
        const ESTIMATE_SAFETY_DIVISOR: usize = 2;
        let images: usize = input.iter().map(CompletionInput::image_tokens).sum();
        (text_tokens(input) * ESTIMATE_SAFETY / ESTIMATE_SAFETY_DIVISOR).saturating_add(images)
    }

    /// What the server can still be asked to generate once this prompt is in its
    /// context, by the estimate: llama.cpp holds prompt and `n_predict` in one
    /// slot, so `max_tokens` is a reservation that must leave the prompt room.
    pub(super) fn output_room_for(&self, input: &[CompletionInput], requested: u32) -> u32 {
        self.output_room_after(Self::estimated_prompt_tokens(input), requested)
    }

    /// The output reservation once `prompt_tokens` are in the configured
    /// window: what was asked, cut to the room left, and never more. A cap the
    /// caller asked for is kept when it fits: a 512-token verdict stays 512.
    fn output_room_after(&self, prompt_tokens: usize, requested: u32) -> u32 {
        let window = self.config.context_window;
        let room = u32::try_from(window.saturating_sub(prompt_tokens)).unwrap_or(u32::MAX);
        let allowed = requested.min(room);
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

    /// [`Self::body`] with an output reservation the configured window holds, and
    /// the request's size; or the refusal of a request whose prompt alone fills
    /// the window. `max_output_tokens` is a ceiling, never a demand. A request
    /// that does not fit by the estimate is counted with the server's tokenizer
    /// before anything is cut or refused; a server that cannot count is judged by
    /// the plain estimate.
    pub(super) async fn measured_body(
        &self,
        request: &CompletionRequest,
    ) -> Result<(Value, RequestSize)> {
        let mut body = self.body(request)?;
        let configured = u32::try_from(self.config.generation.max_tokens).unwrap_or(u32::MAX);
        let requested = request.effective_max_output_tokens(configured);
        // A caller's own ceiling is counted for exactly before the margin cuts
        // it; the configured default is not worth a round trip to the server.
        let wanted = if request.max_output_tokens.is_some_and(|tokens| tokens > 0) {
            requested
        } else {
            1
        };
        let input = &request.input;
        let pictures = RequestSize {
            image_tokens: request.image_tokens(),
            image_count: request.image_count(),
            ..RequestSize::default()
        };
        let estimated = Self::estimated_prompt_tokens(input);
        let mut size = RequestSize {
            prompt_tokens: estimated,
            counted_exactly: false,
            output_tokens: self.output_room_after(estimated, requested),
            ..pictures
        };
        if size.output_tokens < wanted {
            let (prompt_tokens, counted_exactly) = match self.exact_prompt_tokens(input).await {
                Some(exact) => (exact, true),
                None => (Self::plain_prompt_tokens(input), false),
            };
            size = RequestSize {
                prompt_tokens,
                counted_exactly,
                output_tokens: self.output_room_after(prompt_tokens, requested),
                ..pictures
            };
        }
        if size.output_tokens == 0 {
            return Err(window_refusal(
                &self.config.model,
                size,
                self.config.context_window,
            ));
        }
        if let Some(fields) = body.as_object_mut() {
            fields.insert("max_tokens".into(), json!(size.output_tokens));
        }
        Ok((body, size))
    }

    /// The prompt's tokens as this server counts them, with the chat
    /// template's framing ([`TEMPLATE_TOKENS_PER_MESSAGE`] for each message,
    /// [`TEMPLATE_TOKENS_FOR_ANSWER`] for the answer's opening), the same
    /// allowance a learning request is measured with before it is built.
    /// Pictures are added at their declared cost: a tokenizer reads text, not
    /// images. `None` when the server cannot tokenize.
    async fn exact_prompt_tokens(&self, input: &[CompletionInput]) -> Option<usize> {
        let text = input
            .iter()
            .map(|item| match item {
                CompletionInput::Native { value } => Cow::Owned(value.to_string()),
                _ => item.text(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        let images: usize = input.iter().map(CompletionInput::image_tokens).sum();
        let tokenizer = crate::features::llm::scheduler::ServerTokenizer::new(
            self.client.clone(),
            self.server_root(),
        );
        match tokenizer.count(&text).await {
            Ok(tokens) => Some(
                tokens
                    + TEMPLATE_TOKENS_PER_MESSAGE * input.len()
                    + TEMPLATE_TOKENS_FOR_ANSWER
                    + images,
            ),
            Err(error) => {
                tracing::debug!(%error, "The server could not count the prompt; keeping the estimate");
                None
            }
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
        let configured_output = u32::try_from(generation.max_tokens).unwrap_or(u32::MAX);
        let requested_output = request.effective_max_output_tokens(configured_output);
        let max_tokens = self.output_room_for(&request.input, requested_output);
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
