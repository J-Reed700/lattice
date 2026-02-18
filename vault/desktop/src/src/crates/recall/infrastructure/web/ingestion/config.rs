use std::time::Duration;

#[derive(Debug, Clone)]
pub struct WebIngestionConfig {
    pub timeout: Duration,
    pub max_redirects: usize,
    pub max_content_size: usize,
    pub user_agent: String,
    pub follow_redirects: bool,
    pub verify_ssl: bool,
    pub min_text_length: usize,
    pub max_node_score: f64,
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Initial backoff duration in milliseconds
    pub initial_backoff_ms: u64,
    /// Maximum backoff duration in milliseconds
    pub max_backoff_ms: u64,
    /// Require Content-Length header for safety (default: false, uses streaming enforcement)
    pub require_content_length: bool,
}

impl Default for WebIngestionConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            max_redirects: 10,
            max_content_size: 10 * 1024 * 1024,
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
            follow_redirects: true,
            verify_ssl: true,
            min_text_length: 25,
            max_node_score: 1000.0,
            max_retries: 3,
            initial_backoff_ms: 1000,  // 1 second
            max_backoff_ms: 10000,     // 10 seconds
            require_content_length: false,
        }
    }
}

impl WebIngestionConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn builder() -> WebIngestionConfigBuilder {
        WebIngestionConfigBuilder::new()
    }
}

#[derive(Debug, Default)]
pub struct WebIngestionConfigBuilder {
    timeout: Option<Duration>,
    max_redirects: Option<usize>,
    max_content_size: Option<usize>,
    user_agent: Option<String>,
    follow_redirects: Option<bool>,
    verify_ssl: Option<bool>,
    min_text_length: Option<usize>,
    max_node_score: Option<f64>,
    max_retries: Option<u32>,
    initial_backoff_ms: Option<u64>,
    max_backoff_ms: Option<u64>,
    require_content_length: Option<bool>,
}

impl WebIngestionConfigBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn max_redirects(mut self, max_redirects: usize) -> Self {
        self.max_redirects = Some(max_redirects);
        self
    }

    pub fn max_content_size(mut self, max_content_size: usize) -> Self {
        self.max_content_size = Some(max_content_size);
        self
    }

    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = Some(user_agent.into());
        self
    }

    pub fn follow_redirects(mut self, follow_redirects: bool) -> Self {
        self.follow_redirects = Some(follow_redirects);
        self
    }

    pub fn verify_ssl(mut self, verify_ssl: bool) -> Self {
        self.verify_ssl = Some(verify_ssl);
        self
    }

    pub fn min_text_length(mut self, min_text_length: usize) -> Self {
        self.min_text_length = Some(min_text_length);
        self
    }

    pub fn max_node_score(mut self, max_node_score: f64) -> Self {
        self.max_node_score = Some(max_node_score);
        self
    }

    pub fn max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = Some(max_retries);
        self
    }

    pub fn retry_backoff(mut self, initial_ms: u64, max_ms: u64) -> Self {
        self.initial_backoff_ms = Some(initial_ms);
        self.max_backoff_ms = Some(max_ms);
        self
    }

    pub fn require_content_length(mut self, require_content_length: bool) -> Self {
        self.require_content_length = Some(require_content_length);
        self
    }

    pub fn build(self) -> WebIngestionConfig {
        let defaults = WebIngestionConfig::default();

        WebIngestionConfig {
            timeout: self.timeout.unwrap_or(defaults.timeout),
            max_redirects: self.max_redirects.unwrap_or(defaults.max_redirects),
            max_content_size: self.max_content_size.unwrap_or(defaults.max_content_size),
            user_agent: self.user_agent.unwrap_or(defaults.user_agent),
            follow_redirects: self.follow_redirects.unwrap_or(defaults.follow_redirects),
            verify_ssl: self.verify_ssl.unwrap_or(defaults.verify_ssl),
            min_text_length: self.min_text_length.unwrap_or(defaults.min_text_length),
            max_node_score: self.max_node_score.unwrap_or(defaults.max_node_score),
            max_retries: self.max_retries.unwrap_or(defaults.max_retries),
            initial_backoff_ms: self.initial_backoff_ms.unwrap_or(defaults.initial_backoff_ms),
            max_backoff_ms: self.max_backoff_ms.unwrap_or(defaults.max_backoff_ms),
            require_content_length: self.require_content_length.unwrap_or(defaults.require_content_length),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = WebIngestionConfig::default();
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert_eq!(config.max_redirects, 10);
        assert!(config.follow_redirects);
        assert!(config.verify_ssl);
    }

    #[test]
    fn test_builder() {
        let config = WebIngestionConfig::builder()
            .timeout(Duration::from_secs(60))
            .max_redirects(5)
            .user_agent("Custom Agent")
            .follow_redirects(false)
            .build();

        assert_eq!(config.timeout, Duration::from_secs(60));
        assert_eq!(config.max_redirects, 5);
        assert_eq!(config.user_agent, "Custom Agent");
        assert!(!config.follow_redirects);
        assert!(config.verify_ssl);
    }

    #[test]
    fn test_builder_partial() {
        let config = WebIngestionConfig::builder()
            .timeout(Duration::from_secs(45))
            .build();

        assert_eq!(config.timeout, Duration::from_secs(45));
        assert_eq!(config.max_redirects, 10);
    }
}
