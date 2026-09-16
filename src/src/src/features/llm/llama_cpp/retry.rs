//! Retry generation only: never execute tools or persist an unfinished attempt.
use super::*;

const MAX_ATTEMPTS: usize = 5;
const TOTAL_TIMEOUT: Duration = Duration::from_secs(300);

enum Failure {
    Retry(AppError, Option<Duration>),
    Permanent(AppError),
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
                        if let Some(reset) = on_retry {
                            reset(attempt + 1)?;
                        }
                        tracing::warn!(
                            attempt,
                            max_attempts = MAX_ATTEMPTS,
                            "Retrying llama.cpp generation"
                        );
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
                        // Dropping this future cancels both the request and backoff.
                        tokio::time::sleep(delay).await;
                    }
                }
            }
            unreachable!("bounded attempts always return")
        };
        tokio::time::timeout(TOTAL_TIMEOUT, generation)
            .await
            .map_err(|_| {
                AppError::ServiceNotAvailable(
                    "llama.cpp generation exceeded its five-minute retry budget".into(),
                )
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
        while let Some(chunk) = bytes.next().await {
            let chunk = chunk.map_err(|error| Failure::Retry(network_error(error), None))?;
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
