//! A request fitted to its backend's window: the one decision about thinking,
//! the one reply ceiling and the one measure of the prompt, made once before
//! admission and read by everything that follows.
//!
//! The estimate is scaled by 3/2 to size the room, since four characters a
//! token reads low on code, JSON and most non-English text and a low count
//! overruns the window. A backend with a tokenizer counts exactly when the
//! scaled estimate and the ceiling together do not fit; a backend that cannot
//! count is judged by the plain estimate. Opaque provider state is reserved at
//! admission but not counted in the prompt; pictures count by their declared
//! geometry.

use std::borrow::Cow;

use async_trait::async_trait;
use lattice_core::application::ports::llm_port::{
    tokens_for_chars, window_refusal, CompletionInput, CompletionRequest, RequestSize,
    ThinkingMode, TEMPLATE_TOKENS_FOR_ANSWER, TEMPLATE_TOKENS_PER_MESSAGE,
};
use lattice_core::shared::error::Result;

use super::backend::ModelBackend;

/// The margin on a text estimate that sizes the room for the reply: the
/// estimate is scaled by this fraction.
const ESTIMATE_MARGIN: usize = 3;
const ESTIMATE_MARGIN_DIVISOR: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Thinking {
    Off,
    On,
    /// The request leaves it to the model and nothing says what this model does.
    Unknown,
}

impl Thinking {
    pub(crate) fn is_on(self) -> bool {
        self == Self::On
    }

    /// Whether thinking is spent out of the reply ceiling, so an answer's cap
    /// must not cut it.
    pub(crate) fn counts_toward_the_ceiling(self) -> bool {
        self != Self::Off
    }
}

impl From<ThinkingMode> for Thinking {
    fn from(mode: ThinkingMode) -> Self {
        match mode {
            ThinkingMode::On => Self::On,
            ThinkingMode::Off => Self::Off,
        }
    }
}

/// What a backend knows of the most it can write in one reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OutputLimits {
    ceiling: Option<u32>,
}

impl OutputLimits {
    pub(crate) fn known(ceiling: u32) -> Self {
        Self {
            ceiling: Some(ceiling),
        }
    }

    pub(crate) fn of(ceiling: Option<u32>) -> Self {
        Self { ceiling }
    }

    pub(crate) fn ceiling(self) -> Option<u32> {
        self.ceiling
    }

    /// The reply ceiling before the window is considered: the request's own
    /// cap within the model's ceiling; the model's ceiling alone when it
    /// thinks, since thinking counts toward it; `None` when neither is known.
    pub(crate) fn wanted(self, request: &CompletionRequest, thinking: Thinking) -> Option<u32> {
        let own = request.max_output_tokens.filter(|cap| *cap > 0);
        match (own, self.ceiling) {
            _ if thinking.counts_toward_the_ceiling() => self.ceiling,
            (Some(own), Some(model)) => Some(own.min(model)),
            (own, model) => own.or(model),
        }
    }
}

/// The reply ceiling cut to the room the window leaves.
fn cut(wanted: Option<u32>, room: u32) -> Option<u32> {
    wanted.map(|wanted| wanted.min(room))
}

/// How a prompt measured: its tokens, pictures included, and whether the
/// backend's tokenizer counted its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PromptMeasure {
    pub(crate) tokens: usize,
    pub(crate) counted_exactly: bool,
}

/// A request measured against its backend's window, with everything the
/// backend sends decided. Only [`fit`] makes one.
pub(crate) struct Fitted<'r> {
    request: &'r CompletionRequest,
    thinking: Thinking,
    ceiling: Option<u32>,
    prompt: PromptMeasure,
    room: u32,
    opaque_tokens: usize,
    slot: Option<u32>,
}

impl<'r> Fitted<'r> {
    pub(crate) fn request(&self) -> &'r CompletionRequest {
        self.request
    }

    pub(crate) fn thinking(&self) -> Thinking {
        self.thinking
    }

    /// The reply ceiling to send, cut to the room.
    pub(crate) fn ceiling(&self) -> Option<u32> {
        self.ceiling
    }

    /// The llama-server slot the scheduler admitted the request to.
    pub(crate) fn slot(&self) -> Option<u32> {
        self.slot
    }

    /// What the request asks of the window.
    pub(crate) fn size(&self) -> RequestSize {
        RequestSize {
            prompt_tokens: self.prompt.tokens,
            counted_exactly: self.prompt.counted_exactly,
            output_tokens: self.ceiling.unwrap_or(0),
            image_tokens: self.request.image_tokens(),
            image_count: self.request.image_count(),
        }
    }

    /// What admission reserves: the measured prompt, the opaque provider
    /// state, and the reply ceiling, or the whole room where there is none.
    pub(crate) fn reservation(&self) -> usize {
        let output = self.ceiling.unwrap_or(self.room);
        self.prompt
            .tokens
            .saturating_add(self.opaque_tokens)
            .saturating_add(usize::try_from(output).unwrap_or(usize::MAX))
    }

    /// This request pinned to the slot it was admitted to.
    pub(crate) fn in_slot(self, slot: Option<u32>) -> Self {
        Self { slot, ..self }
    }
}

/// How a backend's tokens are counted.
#[async_trait]
pub(crate) trait TokenCounter: Send + Sync {
    fn chars_per_token(&self) -> f64;
    /// The backend's own count of `text`, where it has a tokenizer and it answered.
    async fn exact(&self, text: &str) -> Option<usize>;
}

/// How the request's prompt measures against the backend's window, or, before
/// it is queued, the refusal of a prompt that leaves no room. The answer's
/// size is a ceiling, so nothing else is refused.
pub(crate) async fn fit<'r>(
    backend: &dyn ModelBackend,
    counter: &dyn TokenCounter,
    request: &'r CompletionRequest,
) -> Result<Fitted<'r>> {
    let thinking = backend.thinking(request);
    let wanted = backend.output_limits().wanted(request, thinking);
    let window = backend.window();
    let ratio = counter.chars_per_token();
    let image_tokens = request.image_tokens();
    let estimate = tokens_for_chars(
        prompt_texts(request).map(|text| text.chars().count()).sum(),
        ratio,
    );
    let fixed = TEMPLATE_TOKENS_PER_MESSAGE
        .saturating_mul(request.input.len())
        .saturating_add(TEMPLATE_TOKENS_FOR_ANSWER)
        .saturating_add(image_tokens);
    let plain = PromptMeasure {
        tokens: estimate.saturating_add(fixed),
        counted_exactly: false,
    };
    let margined =
        (estimate.saturating_mul(ESTIMATE_MARGIN) / ESTIMATE_MARGIN_DIVISOR).saturating_add(fixed);
    let reply = usize::try_from(wanted.unwrap_or(0).max(1)).unwrap_or(usize::MAX);
    let prompt = if margined.saturating_add(reply) <= window {
        plain
    } else {
        match counter.exact(&prompt_text(request)).await {
            Some(exact) => PromptMeasure {
                tokens: exact.saturating_add(fixed),
                counted_exactly: true,
            },
            None => plain,
        }
    };
    if prompt.tokens >= window {
        return Err(window_refusal(
            backend.model_name(),
            RequestSize {
                prompt_tokens: prompt.tokens,
                counted_exactly: prompt.counted_exactly,
                output_tokens: 0,
                image_tokens,
                image_count: request.image_count(),
            },
            window,
        ));
    }
    let room = u32::try_from(window - prompt.tokens).unwrap_or(u32::MAX);
    Ok(Fitted {
        request,
        thinking,
        ceiling: cut(wanted, room),
        prompt,
        room,
        opaque_tokens: tokens_for_chars(native_chars(request), ratio),
        slot: None,
    })
}

/// Characters a request puts in front of the model: every message, call and
/// result, plus the tool schemas, which are read like any other prompt text,
/// plus opaque provider state, which takes room however it is counted.
pub(crate) fn prompt_chars(request: &CompletionRequest) -> usize {
    let text: usize = prompt_texts(request).map(|text| text.chars().count()).sum();
    text + native_chars(request)
}

/// Characters of opaque provider state in a request, which are not prompt text.
fn native_chars(request: &CompletionRequest) -> usize {
    request
        .input
        .iter()
        .map(|item| match item {
            CompletionInput::Native { value } => value.to_string().chars().count(),
            _ => 0,
        })
        .sum()
}

/// The text a tokenizer reads in a request: every message with its role,
/// every call and result, then the tool schemas. Opaque provider state
/// (signed reasoning) and pictures' bytes are not text and are left out; a
/// picture's alt stands in for it.
fn prompt_texts(request: &CompletionRequest) -> impl Iterator<Item = Cow<'_, str>> {
    let input = request.input.iter().flat_map(|item| match item {
        CompletionInput::Message { role, content } => {
            vec![
                Cow::Borrowed(role.as_str()),
                Cow::Borrowed(content.as_str()),
            ]
        }
        CompletionInput::ToolCall {
            name, arguments, ..
        } => vec![
            Cow::Borrowed(name.as_str()),
            Cow::Owned(arguments.to_string()),
        ],
        CompletionInput::Multimodal { role, .. } => vec![Cow::Borrowed(role.as_str()), item.text()],
        CompletionInput::ToolResult { output, .. } => vec![Cow::Borrowed(output.as_str())],
        CompletionInput::Native { .. } => Vec::new(),
    });
    let tools = request.tools.iter().flat_map(|tool| {
        [
            Cow::Borrowed(tool.name.as_str()),
            Cow::Borrowed(tool.description.as_str()),
            Cow::Owned(tool.parameters.to_string()),
        ]
    });
    input.chain(tools)
}

/// [`prompt_texts`] as one text, for a tokenizer to count.
fn prompt_text(request: &CompletionRequest) -> String {
    prompt_texts(request).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
pub(crate) use support::{PortBackend, SendUnscheduled, Sent};

#[cfg(test)]
mod support {
    use std::sync::Arc;

    use async_trait::async_trait;
    use lattice_core::application::ports::llm_port::{
        CompletionRequest, CompletionResponse, LLMPort, RequestSize,
    };
    use lattice_core::domain::models::vision::VisionGeometry;
    use lattice_core::shared::error::Result;

    use super::{cut, Fitted, OutputLimits, PromptMeasure, Thinking};
    use crate::features::llm::backend::{
        ignore_retry, ModelBackend, Progress, RetrySink, TextSink,
    };

    impl<'r> Fitted<'r> {
        /// The backend's decision and first-stage ceiling, no window: what
        /// `body(&request)` computes today.
        pub(crate) fn uncut(backend: &dyn ModelBackend, request: &'r CompletionRequest) -> Self {
            let thinking = backend.thinking(request);
            Self {
                request,
                thinking,
                ceiling: backend.output_limits().wanted(request, thinking),
                prompt: PromptMeasure {
                    tokens: 0,
                    counted_exactly: false,
                },
                room: u32::MAX,
                opaque_tokens: 0,
                slot: None,
            }
        }

        /// As `uncut`, after a prompt of `prompt` that leaves `room`.
        pub(crate) fn with_room(
            backend: &dyn ModelBackend,
            request: &'r CompletionRequest,
            prompt: PromptMeasure,
            room: u32,
        ) -> Self {
            let thinking = backend.thinking(request);
            let wanted = backend.output_limits().wanted(request, thinking);
            Self {
                request,
                thinking,
                ceiling: cut(wanted, room),
                prompt,
                room,
                opaque_tokens: 0,
                slot: None,
            }
        }

        /// What a backend was handed, for a test double to record.
        pub(crate) fn sent(&self) -> Sent {
            Sent {
                request: self.request.clone(),
                thinking: self.thinking,
                ceiling: self.ceiling,
                size: self.size(),
                room: self.room,
                slot: self.slot,
            }
        }
    }

    #[derive(Debug, Clone)]
    pub(crate) struct Sent {
        pub request: CompletionRequest,
        pub thinking: Thinking,
        pub ceiling: Option<u32>,
        pub size: RequestSize,
        pub room: u32,
        pub slot: Option<u32>,
    }

    /// Lets a backend's own tests call `client.complete(&request)` as before,
    /// unscheduled.
    #[async_trait]
    pub(crate) trait SendUnscheduled: ModelBackend + Sized {
        async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
            self.send(&Fitted::uncut(self, request), Progress::None)
                .await
        }

        async fn complete_with_progress(
            &self,
            request: &CompletionRequest,
            on_text: &TextSink<'_>,
        ) -> Result<CompletionResponse> {
            let progress = Progress::Streamed {
                on_text,
                on_reasoning: None,
                on_retry: None,
            };
            self.send(&Fitted::uncut(self, request), progress).await
        }

        async fn complete_with_retry_progress(
            &self,
            request: &CompletionRequest,
            on_text: &TextSink<'_>,
            on_retry: &RetrySink<'_>,
        ) -> Result<CompletionResponse> {
            let progress = Progress::Streamed {
                on_text,
                on_reasoning: None,
                on_retry: Some(on_retry),
            };
            self.send(&Fitted::uncut(self, request), progress).await
        }

        async fn complete_with_reasoning_progress(
            &self,
            request: &CompletionRequest,
            on_text: &TextSink<'_>,
            on_reasoning: &TextSink<'_>,
            on_retry: &RetrySink<'_>,
        ) -> Result<CompletionResponse> {
            let progress = Progress::Streamed {
                on_text,
                on_reasoning: Some(on_reasoning),
                on_retry: Some(on_retry),
            };
            self.send(&Fitted::uncut(self, request), progress).await
        }
    }

    impl<T: ModelBackend> SendUnscheduled for T {}

    /// A port double behind the scheduler: thinking Off, ceiling unknown,
    /// `send` mapped back to the four port methods.
    pub(crate) struct PortBackend(pub Arc<dyn LLMPort>);

    #[async_trait]
    impl ModelBackend for PortBackend {
        fn model_name(&self) -> &str {
            self.0.model_name()
        }

        fn provider_name(&self) -> &str {
            self.0.provider_name()
        }

        fn window(&self) -> usize {
            self.0.max_context_tokens()
        }

        fn thinking(&self, _request: &CompletionRequest) -> Thinking {
            Thinking::Off
        }

        fn output_limits(&self) -> OutputLimits {
            OutputLimits::of(None)
        }

        fn supports_tool_calling(&self) -> bool {
            self.0.supports_tool_calling()
        }

        fn vision(&self) -> Option<VisionGeometry> {
            self.0.vision()
        }

        fn keeps_read_prompts(&self) -> bool {
            self.0.keeps_read_prompts()
        }

        fn is_alive(&self) -> bool {
            self.0.is_alive()
        }

        async fn is_ready(&self) -> Result<bool> {
            self.0.is_ready().await
        }

        async fn send(
            &self,
            fitted: &Fitted<'_>,
            progress: Progress<'_>,
        ) -> Result<CompletionResponse> {
            let request = fitted.request();
            match progress {
                Progress::None => self.0.complete(request).await,
                Progress::Streamed {
                    on_text,
                    on_reasoning: None,
                    on_retry: None,
                } => self.0.complete_with_progress(request, on_text).await,
                Progress::Streamed {
                    on_text,
                    on_reasoning: None,
                    on_retry: Some(on_retry),
                } => {
                    self.0
                        .complete_with_retry_progress(request, on_text, on_retry)
                        .await
                }
                Progress::Streamed {
                    on_text,
                    on_reasoning: Some(on_reasoning),
                    on_retry,
                } => {
                    self.0
                        .complete_with_reasoning_progress(
                            request,
                            on_text,
                            on_reasoning,
                            on_retry.unwrap_or(&ignore_retry),
                        )
                        .await
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
