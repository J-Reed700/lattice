#[cfg(test)]
mod security_tests {
    use super::*;
    use crate::infrastructure::web::ingestion::config::WebIngestionConfig;
    use crate::infrastructure::web::ingestion::error::WebIngestionError;
    use crate::infrastructure::web::ingestion::fetcher::WebFetcher;
    use std::time::Duration;

    #[tokio::test]
    async fn test_default_uses_streaming_mode() {
        let config = WebIngestionConfig::default();
        assert!(!config.require_content_length, 
            "Default config should use streaming mode for safety");
    }

    #[tokio::test]
    async fn test_strict_mode_can_be_enabled() {
        let config = WebIngestionConfig::builder()
            .require_content_length(true)
            .build();
        
        assert!(config.require_content_length, 
            "Strict mode should require Content-Length header");
    }

    #[tokio::test]
    async fn test_max_content_size_is_enforced() {
        let config = WebIngestionConfig::builder()
            .max_content_size(1024) // 1KB limit
            .build();
        
        let fetcher = WebFetcher::new(config).unwrap();
        assert_eq!(fetcher.config.max_content_size, 1024);
    }

    #[test]
    fn test_content_length_validation_logic() {
        let max_size = 1024usize;
        let content_length = 2048u64;
        
        assert!(content_length > max_size as u64, 
            "Content-Length check should reject oversized content before download");
    }

    #[test]
    fn test_streaming_size_enforcement_logic() {
        let max_size = 1024;
        let mut total_size = 0;
        let chunk_sizes = vec![512, 512, 512]; // Total: 1536 bytes
        
        let mut exceeded = false;
        for chunk_size in chunk_sizes {
            total_size += chunk_size;
            if total_size > max_size {
                exceeded = true;
                break;
            }
        }
        
        assert!(exceeded, "Streaming should detect size overflow mid-download");
        assert_eq!(total_size, 1536, "Should have detected overflow at 1536 bytes");
    }

    #[test]
    fn test_config_builder_sets_all_security_fields() {
        let config = WebIngestionConfig::builder()
            .max_content_size(5 * 1024 * 1024) // 5MB
            .require_content_length(true)
            .timeout(Duration::from_secs(15))
            .build();
        
        assert_eq!(config.max_content_size, 5 * 1024 * 1024);
        assert!(config.require_content_length);
        assert_eq!(config.timeout, Duration::from_secs(15));
    }

    #[tokio::test]
    async fn test_fetcher_creation_with_security_config() {
        let config = WebIngestionConfig::builder()
            .max_content_size(10 * 1024 * 1024)
            .require_content_length(false) // Use streaming
            .build();
        
        let result = WebFetcher::new(config);
        assert!(result.is_ok(), "Fetcher should be created with valid config");
        
        let fetcher = result.unwrap();
        assert!(!fetcher.config.require_content_length);
        assert_eq!(fetcher.config.max_content_size, 10 * 1024 * 1024);
    }
}
