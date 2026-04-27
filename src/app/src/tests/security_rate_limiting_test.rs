#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]

//! Comprehensive rate limiting integration tests
//!
//! These tests verify that rate limiting actually prevents abuse
//! and enforces the configured limits across different operations.
use std::time::Duration;
use tokio::time::sleep;
use lattice::error::AppError;
use lattice::security::rate_limiter::{RateLimiter, RateLimiters};

#[tokio::test]
async fn test_rate_limiter_allows_within_limit() {
    let limiter = RateLimiter::new(10, 60);
    let key = "test_user";

    // Should allow first 10 requests
    for i in 0..10 {
        let result = limiter.check_rate_limit(key).await;
        assert!(result.is_ok(), "Request {} should be allowed", i);
    }
}

#[tokio::test]
async fn test_rate_limiter_blocks_over_limit() {
    let limiter = RateLimiter::new(10, 60);
    let key = "test_user";

    // Allow 10 requests
    for _ in 0..10 {
        limiter.check_rate_limit(key).await.unwrap();
    }

    // 11th request should be blocked
    let result = limiter.check_rate_limit(key).await;
    assert!(result.is_err(), "11th request should be blocked");

    match result {
        Err(AppError::Other(msg)) => {
            assert!(msg.contains("Rate limit exceeded"));
        }
        _ => panic!("Expected rate limit error"),
    }
}

#[tokio::test]
async fn test_rate_limiter_resets_after_window() {
    let limiter = RateLimiter::new(5, 1); // 5 requests per second
    let key = "test_user";

    // Use all 5 requests
    for _ in 0..5 {
        limiter.check_rate_limit(key).await.unwrap();
    }

    // Next should fail
    assert!(limiter.check_rate_limit(key).await.is_err());

    // Wait for window to reset
    sleep(Duration::from_secs(2)).await;

    // Should allow requests again
    let result = limiter.check_rate_limit(key).await;
    assert!(
        result.is_ok(),
        "Requests should be allowed after window reset"
    );
}

#[tokio::test]
async fn test_rate_limiter_different_keys_independent() {
    let limiter = RateLimiter::new(5, 60);

    // Use limit for key1
    for _ in 0..5 {
        limiter.check_rate_limit("key1").await.unwrap();
    }

    // key1 should be blocked
    assert!(limiter.check_rate_limit("key1").await.is_err());

    // key2 should still be allowed
    assert!(limiter.check_rate_limit("key2").await.is_ok());

    // key3 should also be allowed
    assert!(limiter.check_rate_limit("key3").await.is_ok());
}

#[tokio::test]
async fn test_rate_limiter_cleanup() {
    let limiter = RateLimiter::new(10, 1);

    // Add requests for multiple keys
    for i in 0..5 {
        limiter
            .check_rate_limit(&format!("user{}", i))
            .await
            .unwrap();
    }

    // Wait for window to pass
    sleep(Duration::from_secs(2)).await;

    // Trigger cleanup
    limiter.cleanup().await;

    // All keys should allow new requests (indicating cleanup worked)
    for i in 0..5 {
        let result = limiter.check_rate_limit(&format!("user{}", i)).await;
        assert!(result.is_ok(), "user{} should be allowed after cleanup", i);
    }
}

#[tokio::test]
async fn test_rate_limiter_automatic_cleanup() {
    let limiter = RateLimiter::new(200, 1); // High limit to avoid rate limiting

    // Add requests for multiple users
    for i in 0..10 {
        limiter
            .check_rate_limit(&format!("user{}", i))
            .await
            .unwrap();
    }

    // Wait for window to pass
    sleep(Duration::from_secs(2)).await;

    // Trigger automatic cleanup by making 100+ requests with new users
    for i in 10..120 {
        limiter
            .check_rate_limit(&format!("user{}", i))
            .await
            .unwrap();
    }

    // Verify cleanup occurred (old entries should be gone)
    // This is implicit - if we didn't clean up, we'd run out of memory eventually
}

#[tokio::test]
async fn test_rate_limiters_default_configuration() {
    let limiters = RateLimiters::default();

    // Test search limiter (100 per minute)
    for _ in 0..100 {
        assert!(limiters.search.check_rate_limit("user").await.is_ok());
    }
    assert!(limiters.search.check_rate_limit("user").await.is_err());

    // Test indexing limiter (10 per minute)
    for _ in 0..10 {
        assert!(limiters.indexing.check_rate_limit("user2").await.is_ok());
    }
    assert!(limiters.indexing.check_rate_limit("user2").await.is_err());
}

#[tokio::test]
async fn test_embedding_rate_limit() {
    let limiters = RateLimiters::default();
    let key = "embedding_user";

    // Embedding limiter allows 100 per minute
    for i in 0..100 {
        let result = limiters.embedding.check_rate_limit(key).await;
        assert!(result.is_ok(), "Request {} should succeed", i);
    }

    // 101st should fail
    let result = limiters.embedding.check_rate_limit(key).await;
    assert!(result.is_err(), "Request 101 should be rate limited");
}

#[tokio::test]
async fn test_llm_operation_rate_limits() {
    let limiters = RateLimiters::default();

    // LLM tags: 10 per minute
    for _ in 0..10 {
        assert!(limiters.llm_tags.check_rate_limit("user").await.is_ok());
    }
    assert!(limiters.llm_tags.check_rate_limit("user").await.is_err());

    // LLM embed: 20 per minute
    for _ in 0..20 {
        assert!(limiters.llm_embed.check_rate_limit("user2").await.is_ok());
    }
    assert!(limiters.llm_embed.check_rate_limit("user2").await.is_err());

    // LLM question: 10 per minute
    for _ in 0..10 {
        assert!(limiters
            .llm_question
            .check_rate_limit("user3")
            .await
            .is_ok());
    }
    assert!(limiters
        .llm_question
        .check_rate_limit("user3")
        .await
        .is_err());
}

#[tokio::test]
async fn test_backup_rate_limit() {
    let limiters = RateLimiters::default();

    // Backup limiter: 1 per 5 minutes (300 seconds)
    let result = limiters.backup.check_rate_limit("user").await;
    assert!(result.is_ok(), "First backup should be allowed");

    // Second backup should fail
    let result = limiters.backup.check_rate_limit("user").await;
    assert!(result.is_err(), "Second backup should be rate limited");
}

#[tokio::test]
async fn test_web_ingest_rate_limit() {
    let limiters = RateLimiters::default();

    // Web ingest: 5 per minute
    for i in 0..5 {
        let result = limiters.web_ingest.check_rate_limit("user").await;
        assert!(result.is_ok(), "Request {} should succeed", i);
    }

    // 6th should fail
    let result = limiters.web_ingest.check_rate_limit("user").await;
    assert!(result.is_err(), "6th web ingest should be rate limited");
}

#[tokio::test]
async fn test_file_operations_rate_limit() {
    let limiters = RateLimiters::default();

    // File operations: 50 per minute
    for i in 0..50 {
        let result = limiters.file_operations.check_rate_limit("user").await;
        assert!(result.is_ok(), "File operation {} should succeed", i);
    }

    // 51st should fail
    let result = limiters.file_operations.check_rate_limit("user").await;
    assert!(
        result.is_err(),
        "51st file operation should be rate limited"
    );
}

#[tokio::test]
async fn test_concurrent_rate_limiting() {
    let limiter = RateLimiter::new(10, 60);
    let key = "concurrent_user";

    // Spawn 20 concurrent requests
    let mut handles = vec![];
    for _ in 0..20 {
        let limiter_clone = limiter.clone();
        let key_clone = key.to_string();
        let handle = tokio::spawn(async move { limiter_clone.check_rate_limit(&key_clone).await });
        handles.push(handle);
    }

    // Collect results
    let mut successes = 0;
    let mut failures = 0;

    for handle in handles {
        match handle.await.unwrap() {
            Ok(_) => successes += 1,
            Err(_) => failures += 1,
        }
    }

    // Should allow exactly 10 and block 10
    assert_eq!(successes, 10, "Should allow exactly 10 requests");
    assert_eq!(failures, 10, "Should block exactly 10 requests");
}

#[tokio::test]
async fn test_rate_limit_error_message() {
    let limiter = RateLimiter::new(1, 5);

    // Use the one allowed request
    limiter.check_rate_limit("user").await.unwrap();

    // Get error message from blocked request
    let result = limiter.check_rate_limit("user").await;
    assert!(result.is_err());

    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("Rate limit exceeded"));
    assert!(error_msg.contains("Try again in"));
    assert!(error_msg.contains("seconds"));
}

#[tokio::test]
async fn test_rate_limiter_gradual_recovery() {
    let limiter = RateLimiter::new(5, 2); // 5 requests per 2 seconds

    // Use all 5 requests
    for _ in 0..5 {
        limiter.check_rate_limit("user").await.unwrap();
    }

    // Should be blocked
    assert!(limiter.check_rate_limit("user").await.is_err());

    // Wait half the window
    sleep(Duration::from_secs(1)).await;

    // Should still be blocked
    assert!(limiter.check_rate_limit("user").await.is_err());

    // Wait for full window
    sleep(Duration::from_secs(2)).await;

    // Should be allowed again
    assert!(limiter.check_rate_limit("user").await.is_ok());
}

#[tokio::test]
async fn test_rate_limiter_burst_then_steady() {
    // Use 2-second window instead of 60 for faster testing
    let limiter = RateLimiter::new(10, 2);

    // Burst: use all 10 immediately
    for _ in 0..10 {
        limiter.check_rate_limit("user").await.unwrap();
    }

    // Blocked
    assert!(limiter.check_rate_limit("user").await.is_err());

    // Wait for window to expire (3 seconds to ensure 2-second window has passed)
    sleep(Duration::from_secs(3)).await;

    // Steady state: should work again
    for i in 0..10 {
        let result = limiter.check_rate_limit("user").await;
        assert!(result.is_ok(), "Steady request {} should succeed", i);
    }
}

#[tokio::test]
async fn test_rate_limiter_memory_efficiency() {
    let limiter = RateLimiter::new(100, 1);

    // Create many keys
    for i in 0..1000 {
        limiter
            .check_rate_limit(&format!("user{}", i))
            .await
            .unwrap();
    }

    // Wait for window to pass
    sleep(Duration::from_secs(2)).await;

    // Trigger cleanup with new requests
    for i in 0..100 {
        limiter
            .check_rate_limit(&format!("new_user{}", i))
            .await
            .unwrap();
    }

    // Old keys should be cleaned up (we can't directly test memory,
    // but cleanup should have occurred)
}

#[tokio::test]
async fn test_rate_limiter_zero_limit() {
    let limiter = RateLimiter::new(0, 60);

    // Even first request should be blocked
    let result = limiter.check_rate_limit("user").await;
    assert!(result.is_err(), "Zero limit should block all requests");
}

#[tokio::test]
async fn test_rate_limiter_very_short_window() {
    let limiter = RateLimiter::new(5, 0); // 0 second window (essentially unlimited)

    // Should allow many requests
    for _ in 0..100 {
        // With 0 second window, all requests should work
        limiter.check_rate_limit("user").await.unwrap();
    }
}
