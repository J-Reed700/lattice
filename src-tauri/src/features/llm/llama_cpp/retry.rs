//! Retry generation only: never execute tools or persist an unfinished attempt.
use super::*;

const MAX_ATTEMPTS: usize = 5;

/// Floor on how long a bundled server may take to send its first frame. It
/// may be loading the model into VRAM or prefilling the prompt; on a 7B
/// Q4_K_M GGUF that is sub-second on Apple Silicon but can be 10s+ on cold
/// Windows Vulkan. [`prefill_allowance`] adds time for the prompt on top.
pub(super) const FIRST_FRAME_FLOOR: Duration = Duration::from_secs(60);

/// llama-server's default logical batch. With `return_progress` it reports
/// after each batch it processes, so one batch is the most prompt that can
/// pass between two frames however long the prompt is.
pub(super) const PREFILL_BATCH_TOKENS: usize = 2048;

/// Slowest prefill rate the allowance plans for: a CPU-only box runs a
/// near-full RAG prompt at roughly 30-60 tok/s, so a batch there takes over a
/// minute and a flat 60 s failed the turn before its first token.
pub(super) const PREFILL_FLOOR_TOKENS_PER_SEC: usize = 20;

/// How long a bundled server may stay silent before it starts generating:
/// the floor plus one batch of this prompt at the slowest prefill rate.
/// Progress frames restart the wait, so it only has to cover the gap between
/// two of them — or, from a server that sends none, the whole prompt, which is
/// why short prompts are not given the full batch.
pub(super) fn prefill_allowance(messages: &[Value]) -> Duration {
    let chars: usize = messages.iter().map(|m| m.to_string().chars().count()).sum();
    let tokens = crate::application::ports::llm_port::tokens_for_chars(
        chars,
        crate::application::ports::llm_port::DEFAULT_CHARS_PER_TOKEN,
    )
    .min(PREFILL_BATCH_TOKENS);
    FIRST_FRAME_FLOOR + Duration::from_secs((tokens / PREFILL_FLOOR_TOKENS_PER_SEC) as u64)
}

pub(super) enum Failure {
    Retry(AppError, Option<Duration>),
    Permanent(AppError),
}

/// Tells prompt-processing events apart from generated tokens so that only the
/// latter arms stall detection. Classification only; the decoder owns parsing,
/// and anything it will reject is reported as generation rather than excused.
#[derive(Default)]
pub(crate) struct ProgressWatch {
    partial: Vec<u8>,
}

impl ProgressWatch {
    pub(crate) fn carries_generated_delta(&mut self, chunk: &[u8]) -> bool {
        self.partial.extend_from_slice(chunk);
        let mut generated = false;
        while let Some(end) = self.partial.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.partial.drain(..=end).collect();
            let Ok(line) = std::str::from_utf8(&line) else {
                return true;
            };
            let Some(data) = line.trim().strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data == "[DONE]" {
                return true;
            }
            let Ok(event) = serde_json::from_str::<Value>(data) else {
                return true;
            };
            let Some(choice) = event
                .get("choices")
                .and_then(Value::as_array)
                .and_then(|choices| choices.first())
            else {
                continue;
            };
            let delta = choice.get("delta").unwrap_or(&Value::Null);
            generated |= choice
                .get("finish_reason")
                .is_some_and(|reason| !reason.is_null())
                || ["content", "reasoning_content", "reasoning"]
                    .iter()
                    .any(|key| {
                        delta
                            .get(key)
                            .and_then(Value::as_str)
                            .is_some_and(|text| !text.is_empty())
                    })
                || delta
                    .get("tool_calls")
                    .and_then(Value::as_array)
                    .is_some_and(|calls| !calls.is_empty());
        }
        generated
    }
}

impl LlamaCppLlm {
    pub(super) async fn complete_reliably(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_reasoning: Option<&(dyn Fn(String) -> Result<()> + Send + Sync)>,
        on_retry: Option<&(dyn Fn(usize) -> Result<()> + Send + Sync)>,
    ) -> Result<CompletionResponse> {
        // Failed drafts and partial tool calls must never enter history.
        let mut body = self.body(request)?;
        let first_frame = self.config.prefill_guard.then(|| {
            prefill_allowance(
                body.get("messages")
                    .and_then(Value::as_array)
                    .map_or(&[][..], Vec::as_slice),
            )
        });
        let deadline = request
            .wall_clock_budget()
            .map(|budget| tokio::time::Instant::now() + budget);
        let generation = async {
            for attempt in 1..=MAX_ATTEMPTS {
                let mut emitted = false;
                match self
                    .attempt(&body, first_frame, on_text, on_reasoning, &mut emitted)
                    .await
                {
                    Ok(response) => return Ok(response),
                    Err(Failure::Permanent(error)) => return Err(error),
                    Err(Failure::Retry(error, retry_after)) => {
                        if attempt == MAX_ATTEMPTS
                            || (emitted && on_retry.is_none())
                            || self.process_exited()
                        {
                            return Err(error);
                        }
                        // A failed inference may leave unusable server-side
                        // prompt state. Reprocess the same complete input on the
                        // next attempt; do not repeat a broken cached prefix.
                        // Network outages and rate limits keep cache reuse.
                        if matches!(error, AppError::ServiceNotAvailable(_)) {
                            if let Some(object) = body.as_object_mut() {
                                object.insert("cache_prompt".into(), json!(false));
                            }
                        }
                        let ceiling_ms = (500u64 << (attempt - 1)).min(30_000);
                        let jitter = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .subsec_nanos() as u64;
                        let delay = retry_after
                            .unwrap_or_else(|| {
                                Duration::from_millis(
                                    ceiling_ms / 2 + jitter % (ceiling_ms / 2 + 1),
                                )
                            })
                            .min(Duration::from_secs(30));
                        // A backoff the budget cannot pay for would be reported as an
                        // expiry, hiding the cause the server already told us.
                        if deadline.is_some_and(|deadline| {
                            deadline.saturating_duration_since(tokio::time::Instant::now()) <= delay
                        }) {
                            return Err(error);
                        }
                        if let Some(reset) = on_retry {
                            reset(attempt + 1)?;
                        }
                        tracing::warn!(
                            attempt,
                            max_attempts = MAX_ATTEMPTS,
                            error = %error,
                            "Retrying llama.cpp generation"
                        );
                        // Dropping this future cancels both the request and backoff.
                        tokio::time::sleep(delay).await;
                    }
                }
            }
            unreachable!("bounded attempts always return")
        };
        request.within_time_budget(generation).await.map_err(|_| {
            AppError::ServiceNotAvailable("llama.cpp generation exceeded its time budget".into())
        })?
    }

    async fn attempt(
        &self,
        body: &Value,
        first_frame: Option<Duration>,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_reasoning: Option<&(dyn Fn(String) -> Result<()> + Send + Sync)>,
        emitted: &mut bool,
    ) -> std::result::Result<CompletionResponse, Failure> {
        let send = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .json(body)
            .send();
        // The server may hold the headers until its first frame.
        let response = match first_frame {
            Some(wait) => tokio::time::timeout(wait, send)
                .await
                .map_err(|_| silent(wait))?,
            None => send.await,
        }
        .map_err(|error| Failure::Retry(network_error(error), None))?;
        let transient = matches!(
            response.status().as_u16(),
            408 | 429 | 500 | 502 | 503 | 504
        );
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .map(Duration::from_secs);
        let response = check_status(response).await.map_err(|error| {
            if transient {
                Failure::Retry(error, retry_after)
            } else {
                Failure::Permanent(error)
            }
        })?;
        let response = read_stream(
            response.bytes_stream(),
            Patience {
                first_frame,
                stall: self.config.stall_timeout,
            },
            network_error,
            on_text,
            on_reasoning,
            emitted,
        )
        .await?;
        // The same request would hit the same limit again; retrying only
        // spends the budget several times over.
        if response.finish_reason == "length" && response.text.trim().is_empty() {
            return Err(Failure::Permanent(AppError::InvalidState(
                "the model used its whole answer budget before writing an answer \
                 (llama.cpp stopped: length)"
                    .into(),
            )));
        }
        if response.text.trim().is_empty() && response.tool_calls.is_empty() {
            return Err(Failure::Retry(AppError::Network(
                "llama.cpp returned no public answer or tool calls (empty or reasoning-only response)".into()
            ), None));
        }
        Ok(response)
    }
}

/// How long a stream may go quiet.
#[derive(Clone, Copy)]
pub(super) struct Patience {
    /// Before generation starts: the first-frame allowance a bundled server
    /// is held to, or `None` to wait as long as the time budget allows.
    pub first_frame: Option<Duration>,
    /// Once generation has started: silence for this long is a stall.
    pub stall: Duration,
}

fn silent(wait: Duration) -> Failure {
    Failure::Permanent(AppError::ServiceNotAvailable(format!(
        "llama-server sent nothing for {}s before starting its answer",
        wait.as_secs()
    )))
}

/// Read one streamed attempt to its end, handing text and reasoning to the
/// callbacks as they arrive.
///
/// Prompt-progress events prove the server is working and must not make the
/// request less patient than silence would, so before generation each frame
/// restarts the first-frame wait. Once a generated delta arrives, silence is
/// a stall.
pub(super) async fn read_stream<S, B, E>(
    mut bytes: S,
    patience: Patience,
    transport_error: fn(E) -> AppError,
    on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
    on_reasoning: Option<&(dyn Fn(String) -> Result<()> + Send + Sync)>,
    emitted: &mut bool,
) -> std::result::Result<CompletionResponse, Failure>
where
    S: futures::Stream<Item = std::result::Result<B, E>> + Unpin,
    B: AsRef<[u8]>,
{
    let mut decoder = streaming::Decoder::for_completion();
    let mut generating = false;
    let mut watch = ProgressWatch::default();
    loop {
        let next = if generating {
            tokio::time::timeout(patience.stall, bytes.next())
                .await
                .map_err(|_| {
                    Failure::Retry(
                        AppError::Network(format!(
                            "llama.cpp stopped streaming for {}s",
                            patience.stall.as_secs()
                        )),
                        None,
                    )
                })?
        } else if let Some(wait) = patience.first_frame {
            tokio::time::timeout(wait, bytes.next())
                .await
                .map_err(|_| silent(wait))?
        } else {
            bytes.next().await
        };
        let Some(chunk) = next else { break };
        let chunk = chunk.map_err(|error| Failure::Retry(transport_error(error), None))?;
        let chunk = chunk.as_ref();
        generating |= watch.carries_generated_delta(chunk);
        // Malformed protocol and invalid tools are permanent; explicit
        // transient server statuses inside SSE follow the HTTP retry path.
        let deltas = decoder.push(chunk).map_err(|error| {
            if decoder.retryable_error() {
                Failure::Retry(error, None)
            } else {
                Failure::Permanent(error)
            }
        })?;
        for text in deltas {
            *emitted = true;
            on_text(text).map_err(Failure::Permanent)?;
        }
        let reasoning = decoder.take_reasoning_delta();
        if !reasoning.is_empty() {
            if let Some(deliver) = on_reasoning {
                *emitted = true;
                deliver(reasoning).map_err(Failure::Permanent)?;
            }
        }
        if decoder.done() {
            break;
        }
    }
    decoder
        .finish()
        .map_err(|error| Failure::Retry(error, None))?;
    decoder.into_response().map_err(Failure::Permanent)
}
