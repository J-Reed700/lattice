use crate::infrastructure::web::ingestion::config::WebIngestionConfig;
use crate::infrastructure::web::ingestion::error::{Result, WebIngestionError};
use crate::infrastructure::web::ingestion::types::{is_safe_ip, is_safe_url, normalize_url};
use crate::shared::utils::reqwest_client_builder;
use futures::StreamExt;
use reqwest::{Client, Response};
use std::time::Duration;
use tokio::net::lookup_host;
use tokio::time::sleep;
use tracing::{debug, info, warn};

pub struct WebFetcher {
    client: Client,
    config: WebIngestionConfig,
}

impl WebFetcher {
    pub fn new(config: WebIngestionConfig) -> Result<Self> {
        let client = reqwest_client_builder()
            .timeout(config.timeout)
            .user_agent(&config.user_agent)
            .redirect(if config.follow_redirects {
                reqwest::redirect::Policy::limited(config.max_redirects)
            } else {
                reqwest::redirect::Policy::none()
            })
            .danger_accept_invalid_certs(!config.verify_ssl)
            .build()
            .map_err(|e| WebIngestionError::config_error(format!("Failed to build HTTP client: {}", e)))?;

        Ok(Self { client, config })
    }

    pub async fn fetch(&self, url: &str) -> Result<String> {
        info!("Fetching URL: {}", url);

        let parsed_url = normalize_url(url)?;

        // First check: Validate the URL itself (catches IP addresses in URL)
        if !is_safe_url(&parsed_url) {
            warn!("Blocked potentially unsafe URL: {}", url);
            return Err(WebIngestionError::blocked_url(url));
        }

        // Retry loop with exponential backoff
        let mut attempt = 0;
        let max_attempts = self.config.max_retries + 1; // +1 for initial attempt

        loop {
            attempt += 1;

            debug!(
                url = %url,
                attempt = attempt,
                max_attempts = max_attempts,
                "Attempting to fetch URL"
            );

            match self.fetch_once(&parsed_url, url).await {
                Ok(content) => {
                    if attempt > 1 {
                        info!(
                            url = %url,
                            attempt = attempt,
                            "Successfully fetched after retry"
                        );
                    }
                    return Ok(content);
                }
                Err(e) => {
                    // Check if error is retryable and if we have attempts left
                    if attempt >= max_attempts || !e.is_retryable() {
                        warn!(
                            url = %url,
                            attempt = attempt,
                            error = %e,
                            retryable = e.is_retryable(),
                            "Fetch failed, no more retries"
                        );

                        // Wrap retryable errors with context about retry exhaustion
                        if e.is_retryable() && attempt >= max_attempts {
                            return Err(WebIngestionError::fetch_error(
                                url,
                                format!(
                                    "Failed after {} attempts: {}. This error is temporary, please try again later.",
                                    max_attempts,
                                    e.user_message()
                                )
                            ));
                        }

                        return Err(e);
                    }

                    // Calculate backoff with exponential increase
                    // SECURITY FIX: Use saturating_sub(2) instead of (attempt - 1)
                    // - First retry (attempt=2): 2^0 = 1x initial_backoff
                    // - Second retry (attempt=3): 2^1 = 2x initial_backoff
                    // - Third retry (attempt=4): 2^2 = 4x initial_backoff
                    let backoff_ms = std::cmp::min(
                        self.config.initial_backoff_ms * 2_u64.pow(attempt.saturating_sub(2)),
                        self.config.max_backoff_ms
                    );

                    warn!(
                        url = %url,
                        attempt = attempt,
                        max_attempts = max_attempts,
                        backoff_ms = backoff_ms,
                        error = %e,
                        "Fetch failed, retrying after backoff"
                    );

                    sleep(Duration::from_millis(backoff_ms)).await;
                }
            }
        }
    }

    async fn fetch_once(&self, parsed_url: &url::Url, original_url: &str) -> Result<String> {
        // DNS resolution and IP validation
        // This prevents DNS rebinding attacks where a domain resolves to a private IP
        if let Some(host_str) = parsed_url.host_str() {
            // For domains, we need to add the port for proper DNS resolution
            let port = parsed_url.port_or_known_default().unwrap_or(80);
            let host_with_port = format!("{}:{}", host_str, port);

            debug!("Resolving DNS for: {}", host_str);

            // Resolve all IP addresses for the hostname
            let socket_addrs: Vec<_> = lookup_host(&host_with_port)
                .await
                .map_err(|e| {
                    WebIngestionError::fetch_error(
                        original_url,
                        format!("DNS resolution failed: {}", e),
                    )
                })?
                .collect();

            if socket_addrs.is_empty() {
                return Err(WebIngestionError::fetch_error(
                    original_url,
                    "DNS resolution returned no addresses".to_string(),
                ));
            }

            // Validate each resolved IP address
            for socket_addr in &socket_addrs {
                let ip = socket_addr.ip();
                debug!("Resolved IP: {}", ip);

                if !is_safe_ip(&ip) {
                    warn!(
                        "Blocked URL that resolved to unsafe IP: {} -> {}",
                        original_url, ip
                    );
                    return Err(WebIngestionError::blocked_url(original_url));
                }
            }

            debug!(
                "DNS validation passed: {} resolved to {} safe address(es)",
                host_str,
                socket_addrs.len()
            );

            // Store validated IPs for post-request verification
            let validated_ips: Vec<_> = socket_addrs.iter().map(|addr| addr.ip()).collect();

            debug!("Normalized URL: {}", parsed_url);

            // Make the HTTP request
            let response = self
                .client
                .get(parsed_url.as_str())
                .send()
                .await?;

            // SECURITY: Post-request DNS validation to prevent DNS rebinding attacks
            // DNS could have changed between validation and request (TOCTOU vulnerability)
            debug!("Performing post-request DNS validation to detect rebinding");

            let post_request_addrs: Vec<_> = lookup_host(&host_with_port)
                .await
                .map_err(|e| {
                    WebIngestionError::fetch_error(
                        original_url,
                        &format!("Post-request DNS check failed: {}", e)
                    )
                })?
                .collect();

            // Verify the IPs haven't changed to something unsafe
            for addr in &post_request_addrs {
                if !is_safe_ip(&addr.ip()) {
                    warn!(
                        "DNS rebinding attack detected: {} now resolves to unsafe IP {}",
                        original_url, addr.ip()
                    );
                    return Err(WebIngestionError::fetch_error(
                        original_url,
                        "DNS rebinding attack detected: IP changed to unsafe address after validation"
                    ));
                }
            }

            debug!(
                url = %original_url,
                validated_ips = ?validated_ips,
                post_request_ips = ?post_request_addrs.iter().map(|a| a.ip()).collect::<Vec<_>>(),
                "DNS rebinding check passed"
            );

            self.validate_response(&response, original_url).await?;
        } else {
            debug!("Normalized URL: {}", parsed_url);

            let response = self
                .client
                .get(parsed_url.as_str())
                .send()
                .await?;

            self.validate_response(&response, original_url).await?;
        }

        // SECURITY FIX: Check Content-Length BEFORE reading body
        if self.config.require_content_length {
            // Strict mode: Require Content-Length header
            let content_length = response
                .content_length()
                .ok_or_else(|| {
                    WebIngestionError::fetch_error(
                        original_url,
                        "Server did not provide Content-Length header. Cannot safely download content. \
                         This is a security measure to prevent memory exhaustion attacks. \
                         If you trust this server, you can disable this check in the configuration."
                    )
                })?;

            debug!("Content-Length header: {} bytes", content_length);

            if content_length > self.config.max_content_size as u64 {
                return Err(WebIngestionError::content_too_large(
                    content_length as usize,
                    self.config.max_content_size,
                ));
            }

            // Safe to read now that size is validated
            let text = response.text().await.map_err(|e| {
                WebIngestionError::fetch_error(original_url, format!("Failed to read response body: {}", e))
            })?;

            info!("Successfully fetched {} bytes from {}", text.len(), original_url);
            Ok(text)
        } else {
            // Default mode: Stream with size enforcement (safer, works without Content-Length)
            self.fetch_with_streaming(response, original_url).await
        }
    }

    async fn fetch_with_streaming(&self, response: Response, url: &str) -> Result<String> {
        // Check Content-Length if present (optional optimization)
        if let Some(content_length) = response.content_length() {
            debug!("Content-Length header: {} bytes", content_length);

            if content_length > self.config.max_content_size as u64 {
                return Err(WebIngestionError::content_too_large(
                    content_length as usize,
                    self.config.max_content_size,
                ));
            }
        } else {
            debug!("No Content-Length header, will enforce size limit during streaming");
        }

        // Stream with size limit enforcement
        let mut stream = response.bytes_stream();
        let mut body = Vec::new();
        let mut total_size = 0;

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.map_err(|e| {
                WebIngestionError::fetch_error(url, format!("Stream error while downloading: {}", e))
            })?;

            total_size += chunk.len();

            // Enforce size limit during streaming (prevents memory exhaustion)
            if total_size > self.config.max_content_size {
                return Err(WebIngestionError::content_too_large(
                    total_size,
                    self.config.max_content_size,
                ));
            }

            body.extend_from_slice(&chunk);
        }

        // Convert to UTF-8 string
        let text = String::from_utf8(body).map_err(|e| {
            WebIngestionError::fetch_error(
                url,
                format!("Invalid UTF-8 content: {}. The page may not be text-based.", e)
            )
        })?;

        info!("Successfully streamed {} bytes from {}", text.len(), url);
        Ok(text)
    }

    async fn validate_response(&self, response: &Response, url: &str) -> Result<()> {
        let status = response.status();

        if !status.is_success() {
            let message = status
                .canonical_reason()
                .unwrap_or("Unknown error")
                .to_string();
            return Err(WebIngestionError::http_error(
                url,
                status.as_u16(),
                message,
            ));
        }

        if let Some(content_type) = response.headers().get(reqwest::header::CONTENT_TYPE) {
            let content_type_str = content_type
                .to_str()
                .map_err(|_| WebIngestionError::unsupported_content_type("Invalid content-type header"))?;

            debug!("Content-Type: {}", content_type_str);

            if !Self::is_html_or_text(content_type_str) {
                return Err(WebIngestionError::unsupported_content_type(
                    content_type_str,
                ));
            }
        }

        Ok(())
    }

    fn is_html_or_text(content_type: &str) -> bool {
        let content_type_lower = content_type.to_lowercase();

        content_type_lower.contains("text/html")
            || content_type_lower.contains("application/xhtml+xml")
            || content_type_lower.contains("text/plain")
            || content_type_lower.contains("application/xml")
    }

    pub fn with_config(config: WebIngestionConfig) -> Result<Self> {
        Self::new(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_is_html_or_text() {
        assert!(WebFetcher::is_html_or_text("text/html; charset=utf-8"));
        assert!(WebFetcher::is_html_or_text("application/xhtml+xml"));
        assert!(WebFetcher::is_html_or_text("text/plain"));
        assert!(!WebFetcher::is_html_or_text("application/pdf"));
        assert!(!WebFetcher::is_html_or_text("image/jpeg"));
    }

    #[test]
    fn test_fetcher_creation() {
        let config = WebIngestionConfig::builder()
            .timeout(Duration::from_secs(10))
            .build();

        let fetcher = WebFetcher::new(config);
        assert!(fetcher.is_ok());
    }

    #[tokio::test]
    async fn test_blocked_url() {
        let config = WebIngestionConfig::default();
        let fetcher = WebFetcher::new(config).unwrap();

        let result = fetcher.fetch("http://localhost:8080").await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            WebIngestionError::BlockedUrl { .. }
        ));
    }

    #[tokio::test]
    async fn test_invalid_url() {
        let config = WebIngestionConfig::default();
        let fetcher = WebFetcher::new(config).unwrap();

        let result = fetcher.fetch("not-a-valid-url").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_streaming_mode_enabled_by_default() {
        let config = WebIngestionConfig::default();
        let fetcher = WebFetcher::new(config).unwrap();
        assert!(!fetcher.config.require_content_length);
    }

    #[tokio::test]
    async fn test_strict_mode_requires_content_length() {
        let config = WebIngestionConfig::builder()
            .require_content_length(true)
            .build();

        let fetcher = WebFetcher::new(config).unwrap();
        assert!(fetcher.config.require_content_length);
    }
}
