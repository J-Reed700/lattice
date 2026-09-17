//! Retry generation only: never execute tools or persist an unfinished attempt.
use super::*;

const MAX_ATTEMPTS: usize = 5;

enum Failure {
    Retry(AppError, Option<Duration>),
    Permanent(AppError),
}

/// Tells prompt-processing events apart from generated tokens so that only the
/// latter arms stall detection. Classification only; the decoder owns parsing,
/// and anything it will reject is reported as generation rather than excused.
#[derive(Default)]
struct ProgressWatch {
    partial: Vec<u8>,
}

impl ProgressWatch {
    fn carries_generated_delta(&mut self, chunk: &[u8]) -> bool {
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
        on_retry: Option<&(dyn Fn(usize) -> Result<()> + Send + Sync)>,
    ) -> Result<CompletionResponse> {
        // Failed drafts and partial tool calls must never enter history.
        let body = self.body(request, true)?;
        let budget = request.effective_time_budget();
        let deadline = tokio::time::Instant::now() + budget;
        let generation = async {
            for attempt in 1..=MAX_ATTEMPTS {
                let mut emitted = false;
                match self.attempt(&body, on_text, &mut emitted).await {
                    Ok(response) => return Ok(response),
                    Err(Failure::Permanent(error)) => return Err(error),
                    Err(Failure::Retry(error, retry_after)) => {
                        if attempt == MAX_ATTEMPTS || (emitted && on_retry.is_none()) {
                            return Err(error);
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
                        if deadline.saturating_duration_since(tokio::time::Instant::now()) <= delay
                        {
                            return Err(error);
                        }
                        if let Some(reset) = on_retry {
                            reset(attempt + 1)?;
                        }
                        tracing::warn!(
                            attempt,
                            max_attempts = MAX_ATTEMPTS,
                            "Retrying llama.cpp generation"
                        );
                        // Dropping this future cancels both the request and backoff.
                        tokio::time::sleep(delay).await;
                    }
                }
            }
            unreachable!("bounded attempts always return")
        };
        tokio::time::timeout_at(deadline, generation)
            .await
            .map_err(|_| {
                AppError::ServiceNotAvailable(format!(
                    "llama.cpp generation exceeded its {}-minute time budget",
                    budget.as_secs().div_ceil(60).max(1)
                ))
            })?
    }

    async fn attempt(
        &self,
        body: &Value,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        emitted: &mut bool,
    ) -> std::result::Result<CompletionResponse, Failure> {
        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .json(body)
            .send()
            .await
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
        let mut bytes = response.bytes_stream();
        let mut decoder = streaming::Decoder::for_completion();
        // Waiting for a busy single-slot server, or for it to process a large
        // prompt, is bounded only by the time budget: prompt-progress events prove
        // the server is working and must not make the request less patient than
        // silence would. Once a generated delta arrives, silence is a stall.
        let mut generating = false;
        let mut watch = ProgressWatch::default();
        loop {
            let next = if generating {
                tokio::time::timeout(self.stall_timeout, bytes.next())
                    .await
                    .map_err(|_| {
                        Failure::Retry(
                            AppError::Network(format!(
                                "llama.cpp stopped streaming for {}s",
                                self.stall_timeout.as_secs()
                            )),
                            None,
                        )
                    })?
            } else {
                bytes.next().await
            };
            let Some(chunk) = next else { break };
            let chunk = chunk.map_err(|error| Failure::Retry(network_error(error), None))?;
            generating |= watch.carries_generated_delta(&chunk);
            // Malformed protocol, output limits and invalid tools are not transient.
            for text in decoder.push(&chunk).map_err(Failure::Permanent)? {
                *emitted = true;
                on_text(text).map_err(Failure::Permanent)?;
            }
            if decoder.done() {
                break;
            }
        }
        decoder
            .finish()
            .map_err(|error| Failure::Retry(error, None))?;
        let response = decoder.into_response().map_err(Failure::Permanent)?;
        if response.text.trim().is_empty() && response.tool_calls.is_empty() {
            return Err(Failure::Retry(AppError::Network(
                "llama.cpp returned no public answer or tool calls (empty or reasoning-only response)".into()
            ), None));
        }
        Ok(response)
    }
}
