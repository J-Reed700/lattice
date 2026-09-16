use crate::shared::error::{AppError, Result};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// SECURITY FIX: Rate limiting for expensive operations (CWE-770)
/// Prevents resource exhaustion attacks
#[derive(Clone)]
pub struct RateLimiter {
    limits: Arc<Mutex<HashMap<String, Vec<Instant>>>>,
    max_requests: usize,
    window: Duration,
    cleanup_counter: Arc<Mutex<usize>>,
}

impl std::fmt::Debug for RateLimiter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RateLimiter")
            .field("max_requests", &self.max_requests)
            .field("window", &self.window)
            .finish()
    }
}

impl RateLimiter {
    pub fn new(max_requests: usize, window_seconds: u64) -> Self {
        Self {
            limits: Arc::new(Mutex::new(HashMap::new())),
            max_requests,
            window: Duration::from_secs(window_seconds),
            cleanup_counter: Arc::new(Mutex::new(0)),
        }
    }

    /// Check if a request is allowed for a given key (e.g., user ID, IP address)
    pub async fn check_rate_limit(&self, key: &str) -> Result<()> {
        let now = Instant::now();

        // Auto-cleanup every 100 checks to prevent memory leak
        {
            let mut counter = self.cleanup_counter.lock().await;
            *counter += 1;
            if *counter >= 100 {
                *counter = 0;
                // Perform cleanup without holding the counter lock
                drop(counter);
                let mut limits = self.limits.lock().await;
                limits.retain(|_, timestamps| {
                    timestamps.retain(|&t| now.duration_since(t) < self.window);
                    !timestamps.is_empty()
                });
            }
        }

        let mut limits = self.limits.lock().await;

        let requests = limits.entry(key.to_string()).or_insert_with(Vec::new);

        requests.retain(|&timestamp| now.duration_since(timestamp) < self.window);

        if requests.len() >= self.max_requests {
            let oldest = requests.first().copied().unwrap_or(now);
            let reset_in = self.window.saturating_sub(now.duration_since(oldest));
            return Err(AppError::Other(format!(
                "Rate limit exceeded. Try again in {} seconds",
                reset_in.as_secs()
            )));
        }

        requests.push(now);
        Ok(())
    }

    /// Clean up old entries to prevent memory leaks
    pub async fn cleanup(&self) {
        let mut limits = self.limits.lock().await;
        let now = Instant::now();

        limits.retain(|_, requests| {
            // Keep entries that have recent requests
            requests
                .iter()
                .any(|&timestamp| now.duration_since(timestamp) < self.window)
        });
    }
}

/// Different rate limiters for different operations
#[derive(Clone, Debug)]
pub struct RateLimiters {
    pub search: RateLimiter,
    pub indexing: RateLimiter,
    pub file_operations: RateLimiter,
    pub llm_tags: RateLimiter,
    pub llm_embed: RateLimiter,
    pub llm_search: RateLimiter,
    pub llm_question: RateLimiter,
    pub llm_stream: RateLimiter,
    pub embedding: RateLimiter,        // NEW: General embedding operations
    pub web_ingest: RateLimiter,       // NEW: Web ingestion operations
    pub backup: RateLimiter,           // NEW: Backup/export operations
    pub credentials: RateLimiter,      // NEW: Credential operations (CWE-307 mitigation)
    pub health_check: RateLimiter,     // Health check operations (CWE-770 mitigation)
    pub web_search: RateLimiter,       // SECURITY: Web search operations (reduced limit)
    pub fetch_url: RateLimiter,        // SECURITY: URL fetching operations (reduced limit)
    pub list: RateLimiter,             // List operations
    pub general: RateLimiter,          // General operations (tag commands)
    pub qa: RateLimiter,               // Q&A operations (question answering with RAG)
    pub model_management: RateLimiter, // Model management operations (download, delete, set active)
}

impl Default for RateLimiters {
    fn default() -> Self {
        Self {
            search: RateLimiter::new(100, 60),  // 100 searches per minute
            indexing: RateLimiter::new(10, 60), // 10 indexing operations per minute
            file_operations: RateLimiter::new(50, 60), // 50 file operations per minute
            llm_tags: RateLimiter::new(10, 60), // 10 LLM tag generations per minute
            llm_embed: RateLimiter::new(20, 60), // 20 embedding generations per minute
            llm_search: RateLimiter::new(20, 60), // 20 semantic searches per minute
            llm_question: RateLimiter::new(10, 60), // 10 questions per minute
            llm_stream: RateLimiter::new(10, 60), // 10 streaming questions per minute
            embedding: RateLimiter::new(100, 60), // 100 embedding requests per minute
            web_ingest: RateLimiter::new(5, 60), // 5 web ingestions per minute
            backup: RateLimiter::new(1, 300),   // 1 backup every 5 minutes
            credentials: RateLimiter::new(20, 60), // 20 credential operations per minute (CWE-307)
            health_check: RateLimiter::new(60, 60), // 60 health checks per minute
            web_search: RateLimiter::new(10, 60), // SECURITY: 10 web searches per minute (reduced for security)
            fetch_url: RateLimiter::new(5, 60), // SECURITY: 5 URL fetches per minute (reduced for security)
            list: RateLimiter::new(100, 60),    // 100 list operations per minute
            general: RateLimiter::new(100, 60), // 100 general operations per minute
            qa: RateLimiter::new(10, 60), // 10 Q&A operations per minute (expensive - LLM calls)
            model_management: RateLimiter::new(50, 60), // 50 model management operations per minute
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiting() {
        let limiter = RateLimiter::new(3, 1); // 3 requests per second
        let key = "test_user";

        // First 3 requests should succeed
        assert!(limiter.check_rate_limit(key).await.is_ok());
        assert!(limiter.check_rate_limit(key).await.is_ok());
        assert!(limiter.check_rate_limit(key).await.is_ok());

        // 4th request should fail
        assert!(limiter.check_rate_limit(key).await.is_err());

        // Wait for window to pass
        tokio::time::sleep(Duration::from_secs(2)).await;

        assert!(limiter.check_rate_limit(key).await.is_ok());
    }

    #[tokio::test]
    async fn test_cleanup() {
        let limiter = RateLimiter::new(10, 1);

        limiter.check_rate_limit("user1").await.unwrap();
        limiter.check_rate_limit("user2").await.unwrap();

        // Wait for window to pass
        tokio::time::sleep(Duration::from_secs(2)).await;

        limiter.cleanup().await;

        let limits = limiter.limits.lock().await;
        assert_eq!(limits.len(), 0);
    }

    #[tokio::test]
    async fn test_automatic_cleanup() {
        let limiter = RateLimiter::new(200, 1); // High limit to avoid rate limiting

        for i in 0..10 {
            limiter
                .check_rate_limit(&format!("user{}", i))
                .await
                .unwrap();
        }

        {
            let limits = limiter.limits.lock().await;
            assert_eq!(limits.len(), 10);
        }

        // Wait for window to pass
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Trigger automatic cleanup by making 100 requests with new users
        for i in 10..110 {
            limiter
                .check_rate_limit(&format!("user{}", i))
                .await
                .unwrap();
        }

        // Old entries should be cleaned up automatically
        let limits = limiter.limits.lock().await;
        // Only recent users (user10-user109) should remain
        assert!(
            limits.len() <= 100,
            "Expected <= 100 entries, got {}",
            limits.len()
        );

        // Old users should be gone
        for i in 0..10 {
            assert!(
                !limits.contains_key(&format!("user{}", i)),
                "Old user{} should have been cleaned up",
                i
            );
        }
    }
}
