//! Smoke tests for Cache plugin DTOs

use lattice::features::cache::commands::{
    CacheMetrics, CacheOperation, CacheResponse, SearchCacheStats,
};

#[test]
fn test_cache_stats_creation() {
    let stats = SearchCacheStats {
        size: 42,
        capacity: 100,
        hits: 150,
        misses: 50,
        total_time_saved_ms: 5000,
        hit_rate: 0.75,
    };

    assert_eq!(stats.size, 42);
    assert_eq!(stats.capacity, 100);
    assert_eq!(stats.hits, 150);
    assert_eq!(stats.misses, 50);
    assert_eq!(stats.hit_rate, 0.75);
}

#[test]
fn test_cache_stats_empty() {
    let stats = SearchCacheStats {
        size: 0,
        capacity: 100,
        hits: 0,
        misses: 0,
        total_time_saved_ms: 0,
        hit_rate: 0.0,
    };

    assert_eq!(stats.size, 0);
    assert_eq!(stats.hit_rate, 0.0);
}

#[test]
fn test_cache_metrics_creation() {
    let metrics = CacheMetrics {
        hits: 200,
        misses: 100,
        total_time_saved_ms: 10000,
        hit_rate: 0.667,
    };

    assert_eq!(metrics.hits, 200);
    assert_eq!(metrics.misses, 100);
    assert_eq!(metrics.hit_rate, 0.667);
}

#[test]
fn test_cache_operation_clear() {
    let json = r#"{"action":"clear"}"#;
    let op: CacheOperation = serde_json::from_str(json).expect("Failed to parse");
    matches!(op, CacheOperation::Clear);
}

#[test]
fn test_cache_operation_get_stats() {
    let json = r#"{"action":"getStats"}"#;
    let op: CacheOperation = serde_json::from_str(json).expect("Failed to parse");
    matches!(op, CacheOperation::GetStats);
}

#[test]
fn test_cache_operation_get_metrics() {
    let json = r#"{"action":"getMetrics"}"#;
    let op: CacheOperation = serde_json::from_str(json).expect("Failed to parse");
    matches!(op, CacheOperation::GetMetrics);
}

#[test]
fn test_cache_response_cleared() {
    let response = CacheResponse::Cleared;
    let json = serde_json::to_string(&response).expect("Failed to serialize");
    assert!(json.contains("cleared"));
}

#[test]
fn test_cache_stats_serialization() {
    let stats = SearchCacheStats {
        size: 10,
        capacity: 50,
        hits: 100,
        misses: 25,
        total_time_saved_ms: 2500,
        hit_rate: 0.8,
    };

    let json = serde_json::to_string(&stats).expect("Failed to serialize");
    assert!(json.contains("\"size\":10"));
    assert!(json.contains("\"hitRate\":0.8"));
}
