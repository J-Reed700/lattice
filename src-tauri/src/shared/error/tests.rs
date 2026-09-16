//! Comprehensive tests for enhanced error handling system.
//!
//! These tests verify:
//! - Complete error chain preservation
//! - Context propagation through multiple layers
//! - Error categorization
//! - Conversion between error types
//! - Helper macros functionality
//! - User-friendly message generation

use super::enhanced::{EnhancedError, EnhancedResultExt, ErrorCategory, ErrorContext, Result};
use std::error::Error;

#[test]
fn test_io_error_chain_preserved() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file.txt not found");
    let enhanced: EnhancedError = io_err.into();

    let error_string = enhanced.to_string();
    assert!(error_string.contains("I/O operation failed"));

    assert!(enhanced.source().is_some());
    let source = enhanced.source().unwrap();
    assert!(source.to_string().contains("file.txt not found"));

    assert_eq!(source.source(), None); // This is the root
}

#[test]
fn test_database_error_chain_preserved() {
    let db_err = sqlx::Error::RowNotFound;
    let enhanced: EnhancedError = db_err.into();

    assert!(matches!(enhanced, EnhancedError::Database { .. }));

    assert!(enhanced.source().is_some());
    let source = enhanced.source().unwrap();
    assert!(source.to_string().contains("no rows returned"));
}

#[test]
fn test_json_error_chain_preserved() {
    let json_str = "{ invalid json }";
    let json_err: serde_json::Error =
        serde_json::from_str::<serde_json::Value>(json_str).unwrap_err();
    let enhanced: EnhancedError = json_err.into();

    assert!(matches!(enhanced, EnhancedError::Json { .. }));

    assert!(enhanced.source().is_some());
    assert!(enhanced.to_string().contains("JSON"));
}

#[test]
fn test_wrapped_error_chain() {
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
    let result: std::result::Result<(), _> = Err(io_err);

    let wrapped = result
        .with_context("Failed to read config")
        .wrap("initialize_app");

    assert!(wrapped.is_err());
    let err = wrapped.unwrap_err();

    assert!(matches!(err, EnhancedError::Wrapped { .. }));

    assert!(err.to_string().contains("initialize_app"));
    assert!(err.source().is_some());
}

#[test]
fn test_multi_layer_error_chain() {
    fn layer3() -> std::io::Result<String> {
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "config.toml not found",
        ))
    }

    fn layer2() -> Result<String> {
        layer3()
            .with_context("Failed to load configuration")
            .add_tag("config_type", "app_settings")
    }

    fn layer1() -> Result<String> {
        layer2()
            .wrap("startup_initialization")
            .add_tag("phase", "bootstrap")
    }

    let result = layer1();
    assert!(result.is_err());

    let err = result.unwrap_err();

    let chain_str = err.chain_to_string();
    assert!(chain_str.contains("startup_initialization"));
    assert!(chain_str.contains("Failed to load configuration"));
    assert!(chain_str.contains("config.toml not found"));
    assert!(chain_str.contains("caused by"));

    assert_eq!(
        err.context().tags.get("phase"),
        Some(&"bootstrap".to_string())
    );
}

#[test]
fn test_error_chain_to_string_format() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "original error");
    let enhanced = EnhancedError::Io {
        message: "wrapped error".to_string(),
        source: io_err,
        context: ErrorContext::new(),
    };

    let chain = enhanced.chain_to_string();

    assert!(chain.contains("I/O operation failed: wrapped error"));
    assert!(chain.contains("caused by:"));
    assert!(chain.contains("original error"));

    let lines: Vec<&str> = chain.split('\n').collect();
    assert!(lines.len() >= 2);
}

#[test]
fn test_context_tags_preserved() {
    let result: Result<i32> = Err(EnhancedError::NotFound {
        message: "item not found".to_string(),
        context: ErrorContext::new(),
    });

    let with_context = result
        .add_tag("item_id", "42")
        .add_tag("user_id", "user123")
        .add_tag("operation", "fetch");

    assert!(with_context.is_err());
    let err = with_context.unwrap_err();

    let tags = &err.context().tags;
    assert_eq!(tags.get("item_id"), Some(&"42".to_string()));
    assert_eq!(tags.get("user_id"), Some(&"user123".to_string()));
    assert_eq!(tags.get("operation"), Some(&"fetch".to_string()));
}

#[test]
fn test_context_category_preserved() {
    let result: Result<i32> = Err(EnhancedError::Other {
        message: "error".to_string(),
        context: ErrorContext::new(),
    });

    let with_category = result.with_category(ErrorCategory::Retriable);
    assert!(with_category.is_err());

    let err = with_category.unwrap_err();
    assert_eq!(err.category(), ErrorCategory::Retriable);
}

#[test]
fn test_context_user_message() {
    let mut err = EnhancedError::Network {
        message: "TCP connection refused".to_string(),
        context: ErrorContext::new(),
    };

    err = err.with_user_message("Please check your internet connection");

    assert_eq!(
        err.to_user_friendly_message(),
        "Please check your internet connection"
    );
}

#[test]
fn test_multiple_context_additions() {
    let result: Result<()> = Err(EnhancedError::Other {
        message: "base error".to_string(),
        context: ErrorContext::new(),
    });

    let enhanced = result
        .add_tag("step", "validation")
        .add_tag("attempt", "3")
        .with_category(ErrorCategory::UserFixable)
        .with_context("Operation failed");

    assert!(enhanced.is_err());
    let err = enhanced.unwrap_err();

    // All context should be preserved
    assert_eq!(
        err.context().tags.get("step"),
        Some(&"validation".to_string())
    );
    assert_eq!(err.context().tags.get("attempt"), Some(&"3".to_string()));
    assert_eq!(err.category(), ErrorCategory::UserFixable);
    assert!(err.context().tags.contains_key("context"));
}

#[test]
fn test_retriable_errors() {
    let errors = vec![
        EnhancedError::QueueFull {
            capacity: 100,
            context: ErrorContext::new(),
        },
        EnhancedError::Network {
            message: "timeout".to_string(),
            context: ErrorContext::new(),
        },
        EnhancedError::Timeout {
            operation: "fetch".to_string(),
            duration_ms: 5000,
            context: ErrorContext::new(),
        },
        EnhancedError::CircuitBreakerOpen {
            service: "api".to_string(),
            context: ErrorContext::new(),
        },
    ];

    for err in errors {
        assert!(err.is_retriable(), "Error should be retriable: {}", err);
    }
}

#[test]
fn test_user_fixable_errors() {
    let errors = vec![
        EnhancedError::InvalidInput {
            message: "invalid format".to_string(),
            context: ErrorContext::new(),
        },
        EnhancedError::InvalidConfig {
            message: "missing field".to_string(),
            context: ErrorContext::new(),
        },
        EnhancedError::PermissionDenied {
            message: "no access".to_string(),
            context: ErrorContext::new(),
        },
        EnhancedError::FileTooLarge {
            path: "big.pdf".to_string(),
            size_mb: 100,
            max_size_mb: 10,
            context: ErrorContext::new(),
        },
        EnhancedError::UnsupportedFileType {
            path: "file.xyz".to_string(),
            detected_type: "xyz".to_string(),
            context: ErrorContext::new(),
        },
    ];

    for err in errors {
        assert!(
            err.is_user_fixable(),
            "Error should be user-fixable: {}",
            err
        );
    }
}

#[test]
fn test_fatal_errors() {
    let err = EnhancedError::Database {
        message: "connection lost".to_string(),
        source: sqlx::Error::RowNotFound,
        context: ErrorContext::new(),
    };

    assert!(err.is_fatal());
    assert!(!err.is_retriable());
}

#[test]
fn test_transient_errors() {
    let errors = vec![
        EnhancedError::Network {
            message: "timeout".to_string(),
            context: ErrorContext::new(),
        },
        EnhancedError::Timeout {
            operation: "query".to_string(),
            duration_ms: 1000,
            context: ErrorContext::new(),
        },
        EnhancedError::CircuitBreakerOpen {
            service: "search".to_string(),
            context: ErrorContext::new(),
        },
    ];

    for err in errors {
        assert!(err.is_transient(), "Error should be transient: {}", err);
        assert!(
            err.is_retriable(),
            "Transient errors should be retriable: {}",
            err
        );
    }
}

#[test]
fn test_custom_category() {
    let err = EnhancedError::Other {
        message: "custom error".to_string(),
        context: ErrorContext::new().with_category(ErrorCategory::Fatal),
    };

    assert!(err.is_fatal());
    assert_eq!(err.category(), ErrorCategory::Fatal);
}

#[test]
fn test_default_user_friendly_messages() {
    let test_cases = vec![
        (
            EnhancedError::QueueFull {
                capacity: 100,
                context: ErrorContext::new(),
            },
            "The system is currently busy. Please try again in a moment.",
        ),
        (
            EnhancedError::FileTooLarge {
                path: "large.pdf".to_string(),
                size_mb: 50,
                max_size_mb: 10,
                context: ErrorContext::new(),
            },
            "This file is too large to process. Try a smaller file.",
        ),
        (
            EnhancedError::UnsupportedFileType {
                path: "file.xyz".to_string(),
                detected_type: "xyz".to_string(),
                context: ErrorContext::new(),
            },
            "This file type is not supported.",
        ),
        (
            EnhancedError::PermissionDenied {
                message: "access denied".to_string(),
                context: ErrorContext::new(),
            },
            "Access denied. Please check file permissions.",
        ),
        (
            EnhancedError::Network {
                message: "connection failed".to_string(),
                context: ErrorContext::new(),
            },
            "Network connection issue. Please check your internet connection.",
        ),
    ];

    for (err, expected) in test_cases {
        assert_eq!(err.to_user_friendly_message(), expected);
    }
}

#[test]
fn test_custom_user_friendly_message() {
    let err = EnhancedError::Other {
        message: "internal error X1234".to_string(),
        context: ErrorContext::new()
            .with_user_message("Something went wrong. Please contact support (Error X1234)"),
    };

    assert_eq!(
        err.to_user_friendly_message(),
        "Something went wrong. Please contact support (Error X1234)"
    );
}

#[test]
fn test_io_error_conversion() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
    let enhanced: EnhancedError = io_err.into();

    assert!(matches!(enhanced, EnhancedError::Io { .. }));
    assert!(enhanced.source().is_some());
}

#[test]
fn test_sqlx_error_conversion() {
    let db_err = sqlx::Error::RowNotFound;
    let enhanced: EnhancedError = db_err.into();

    assert!(matches!(enhanced, EnhancedError::Database { .. }));
    assert!(enhanced.source().is_some());
    assert!(enhanced.is_fatal());
}

#[test]
fn test_serde_json_error_conversion() {
    let json_err: serde_json::Error = serde_json::from_str::<i32>("invalid").unwrap_err();
    let enhanced: EnhancedError = json_err.into();

    assert!(matches!(enhanced, EnhancedError::Json { .. }));
    assert!(enhanced.source().is_some());
    assert!(enhanced.is_user_fixable());
}

#[test]
fn test_to_string_conversion() {
    let err = EnhancedError::NotFound {
        message: "item not found".to_string(),
        context: ErrorContext::new(),
    };

    let error_string: String = err.into();
    assert!(!error_string.is_empty());
}

#[test]
fn test_with_context_on_result() {
    let result: std::io::Result<i32> = Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "file not found",
    ));

    let enhanced = result.with_context("Failed to read configuration");
    assert!(enhanced.is_err());

    let err = enhanced.unwrap_err();
    assert!(err.to_string().contains("Failed to read configuration"));
}

#[test]
fn test_with_context_fn_on_result() {
    let path = "/tmp/config.toml";
    let result: std::io::Result<i32> = Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "not found",
    ));

    let enhanced = result.with_context_fn(|| format!("Failed to read file: {}", path));
    assert!(enhanced.is_err());

    let err = enhanced.unwrap_err();
    assert!(err
        .to_string()
        .contains("Failed to read file: /tmp/config.toml"));
}

#[test]
fn test_add_tag_on_result() {
    let result: std::io::Result<i32> = Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "not found",
    ));

    let enhanced = result.add_tag("file_path", "/tmp/test.txt");
    assert!(enhanced.is_err());

    let err = enhanced.unwrap_err();
    assert_eq!(
        err.context().tags.get("file_path"),
        Some(&"/tmp/test.txt".to_string())
    );
}

#[test]
fn test_wrap_on_result() {
    let result: std::io::Result<i32> = Err(std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "access denied",
    ));

    let wrapped = result.wrap("read_config_file");
    assert!(wrapped.is_err());

    let err = wrapped.unwrap_err();
    assert!(matches!(err, EnhancedError::Wrapped { .. }));
    assert!(err.to_string().contains("read_config_file"));
    assert!(err.source().is_some());
}

#[test]
fn test_option_with_context() {
    let opt: Option<String> = None;
    let result = opt.with_context("Value not found");

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, EnhancedError::NotFound { .. }));
    assert!(err.to_string().contains("Value not found"));
}

#[test]
fn test_option_with_context_fn() {
    let key = "username";
    let opt: Option<String> = None;
    let result = opt.with_context_fn(|| format!("Missing required field: {}", key));

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Missing required field: username"));
}

#[test]
fn test_bail_with_context_macro() {
    fn test_function(value: i32) -> Result<()> {
        if value < 0 {
            bail_with_context!("Negative values not allowed: {}", value);
        }
        Ok(())
    }

    let result = test_function(-5);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Negative values not allowed: -5"));
}

#[test]
fn test_ensure_with_context_macro() {
    fn test_function(value: i32, max: i32) -> Result<()> {
        ensure_with_context!(value <= max, "Value {} exceeds maximum {}", value, max);
        Ok(())
    }

    // Success case
    assert!(test_function(5, 10).is_ok());

    // Failure case
    let result = test_function(15, 10);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Value 15 exceeds maximum 10"));
}

#[tokio::test]
async fn test_async_error_propagation() {
    async fn level3() -> std::io::Result<String> {
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "file.txt not found",
        ))
    }

    async fn level2() -> Result<String> {
        level3()
            .await
            .with_context("Failed to load data")
            .add_tag("level", "2")
    }

    async fn level1() -> Result<String> {
        level2().await.wrap("initialize").add_tag("level", "1")
    }

    let result = level1().await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    let chain = err.chain_to_string();

    assert!(chain.contains("initialize"));
    assert!(chain.contains("Failed to load data"));
    assert!(chain.contains("file.txt not found"));

    assert_eq!(err.context().tags.get("level"), Some(&"1".to_string()));
}

#[test]
fn test_error_context_builder() {
    let context = ErrorContext::new()
        .add_tag("operation", "index")
        .add_tag("user_id", "123")
        .add_tag("file_count", "50")
        .with_user_message("Processing failed")
        .with_category(ErrorCategory::Transient);

    assert_eq!(context.tags.get("operation"), Some(&"index".to_string()));
    assert_eq!(context.tags.get("user_id"), Some(&"123".to_string()));
    assert_eq!(context.tags.get("file_count"), Some(&"50".to_string()));
    assert_eq!(context.user_message, Some("Processing failed".to_string()));
    assert_eq!(context.category, ErrorCategory::Transient);
}

#[test]
fn test_error_display_formatting() {
    let err = EnhancedError::FileTooLarge {
        path: "/path/to/file.pdf".to_string(),
        size_mb: 25,
        max_size_mb: 10,
        context: ErrorContext::new(),
    };

    let display = format!("{}", err);
    assert!(display.contains("/path/to/file.pdf"));
    assert!(display.contains("25"));
    assert!(display.contains("10"));
}

#[test]
fn test_backward_compatibility_with_legacy() {
    use super::legacy::AppError;

    // EnhancedError -> AppError
    let enhanced = EnhancedError::NotFound {
        message: "test not found".to_string(),
        context: ErrorContext::new(),
    };

    let legacy: AppError = enhanced.into();
    assert!(matches!(legacy, AppError::NotFound(_)));

    // AppError -> EnhancedError
    let legacy = AppError::InvalidInput("bad input".to_string());
    let enhanced: EnhancedError = legacy.into();
    assert!(matches!(enhanced, EnhancedError::InvalidInput { .. }));
}

#[test]
fn test_error_source_method_works() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "root cause: file missing");
    let enhanced = EnhancedError::Io {
        message: "first level".to_string(),
        source: io_err,
        context: ErrorContext::new(),
    };

    let source = enhanced.source();
    assert!(
        source.is_some(),
        "source() should return Some for IO errors"
    );

    let source_str = source.unwrap().to_string();
    assert!(
        source_str.contains("root cause: file missing"),
        "Source should contain root cause message, got: {}",
        source_str
    );
}

#[test]
fn test_multi_level_error_source_chain() {
    fn layer_4() -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "layer 4: permission denied",
        ))
    }

    fn layer_3() -> Result<()> {
        layer_4()
            .with_context("layer 3: failed to access resource")
            .add_tag("layer", "3")
    }

    fn layer_2() -> Result<()> {
        layer_3()
            .wrap("layer 2: resource initialization failed")
            .add_tag("layer", "2")
    }

    fn layer_1() -> Result<()> {
        layer_2()
            .wrap("layer 1: system startup failed")
            .add_tag("layer", "1")
    }

    let result = layer_1();
    assert!(result.is_err());

    let err = result.unwrap_err();

    // Walk the error chain using source()
    let mut current: &dyn Error = &err;
    let mut chain_depth = 0;

    while let Some(source) = current.source() {
        chain_depth += 1;
        current = source;
        if chain_depth > 10 {
            panic!("Error chain too deep, possible cycle");
        }
    }

    assert!(
        chain_depth >= 2,
        "Error chain should have at least 2 levels, got {}",
        chain_depth
    );

    let chain_str = err.chain_to_string();
    assert!(
        chain_str.contains("layer 4: permission denied"),
        "Root cause should be in chain"
    );
    assert!(chain_str.contains("layer 3:"), "Layer 3 should be in chain");
    assert!(chain_str.contains("layer 2:"), "Layer 2 should be in chain");
    assert!(chain_str.contains("layer 1:"), "Layer 1 should be in chain");
}

#[test]
fn test_context_preserved_through_source_chain() {
    let result: Result<()> = Err(EnhancedError::NotFound {
        message: "base error".to_string(),
        context: ErrorContext::new(),
    });

    let enhanced = result
        .add_tag("request_id", "req-12345")
        .add_tag("user_id", "user-789")
        .wrap("operation_failed");

    assert!(enhanced.is_err());
    let err = enhanced.unwrap_err();

    assert_eq!(
        err.context().tags.get("request_id"),
        Some(&"req-12345".to_string())
    );
    assert_eq!(
        err.context().tags.get("user_id"),
        Some(&"user-789".to_string())
    );

    assert!(err.source().is_some());
}

#[test]
fn test_database_error_source_preservation() {
    let db_err = sqlx::Error::RowNotFound;
    let enhanced = EnhancedError::Database {
        message: "User not found in database".to_string(),
        source: db_err,
        context: ErrorContext::new(),
    };

    assert!(enhanced.source().is_some());

    let source = enhanced.source().unwrap();
    assert!(
        source.to_string().contains("no rows returned"),
        "Source should be the original sqlx error"
    );
}

#[test]
fn test_wrapped_error_preserves_complete_chain() {
    // Original error
    let json_err: serde_json::Error = serde_json::from_str::<i32>("not a number").unwrap_err();

    let result: std::result::Result<(), _> = Err(json_err);
    let enhanced = result
        .with_context("Failed to parse configuration")
        .add_tag("config_file", "settings.json")
        .wrap("load_settings");

    assert!(enhanced.is_err());
    let err = enhanced.unwrap_err();

    assert!(matches!(err, EnhancedError::Wrapped { .. }));

    let chain = err.chain_to_string();
    assert!(chain.contains("load_settings"));
    assert!(chain.contains("Failed to parse configuration"));
    assert!(chain.contains("expected"));

    assert_eq!(
        err.context().tags.get("config_file"),
        Some(&"settings.json".to_string())
    );
}

#[test]
fn test_error_source_none_for_leaf_errors() {
    // These error types have no underlying source
    let errors = vec![
        EnhancedError::NotFound {
            message: "item not found".to_string(),
            context: ErrorContext::new(),
        },
        EnhancedError::InvalidInput {
            message: "bad input".to_string(),
            context: ErrorContext::new(),
        },
        EnhancedError::QueueFull {
            capacity: 100,
            context: ErrorContext::new(),
        },
    ];

    for err in errors {
        assert!(
            err.source().is_none(),
            "Leaf errors should have no source: {}",
            err
        );
    }
}

#[test]
fn test_error_chain_with_multiple_wrapping_layers() {
    fn deepest_layer() -> std::io::Result<String> {
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "config.toml not found at /etc/app/config.toml",
        ))
    }

    fn middle_layer() -> Result<String> {
        deepest_layer()
            .with_context("Configuration file could not be loaded")
            .add_tag("config_path", "/etc/app/config.toml")
            .add_tag("attempt", "1")
    }

    fn top_layer() -> Result<String> {
        middle_layer()
            .wrap("application_initialization")
            .with_category(ErrorCategory::UserFixable)
            .add_tag("component", "config_loader")
    }

    let result = top_layer();
    assert!(result.is_err());

    let err = result.unwrap_err();

    let mut depth = 0;
    let mut current: Option<&dyn Error> = Some(&err);

    while let Some(e) = current {
        depth += 1;
        current = e.source();
    }

    assert!(depth >= 2, "Should have multiple levels in chain");

    assert_eq!(err.category(), ErrorCategory::UserFixable);
    assert_eq!(
        err.context().tags.get("component"),
        Some(&"config_loader".to_string())
    );

    let chain_str = err.chain_to_string();
    assert!(chain_str.contains("application_initialization"));
    assert!(chain_str.contains("Configuration file could not be loaded"));
    assert!(chain_str.contains("config.toml not found"));
}

#[test]
fn test_source_chain_iteration() {
    let io_err = std::io::Error::new(std::io::ErrorKind::Other, "disk full");
    let enhanced = EnhancedError::Io {
        message: "write failed".to_string(),
        source: io_err,
        context: ErrorContext::new(),
    };

    // Collect all errors in the chain
    let mut chain = Vec::new();
    let mut current: Option<&dyn Error> = Some(&enhanced);

    while let Some(e) = current {
        chain.push(e.to_string());
        current = e.source();
    }

    assert!(chain.len() >= 2, "Chain should have multiple entries");
    assert!(chain[0].contains("I/O operation failed"));
    assert!(chain.last().unwrap().contains("disk full"));
}

#[test]
fn test_parking_lot_compatible_error_sharing() {
    use std::sync::Arc;

    let err = Arc::new(EnhancedError::Database {
        message: "connection pool exhausted".to_string(),
        source: sqlx::Error::PoolTimedOut,
        context: ErrorContext::new()
            .add_tag("pool_size", "10")
            .add_tag("wait_time_ms", "5000"),
    });

    // Clone and use in multiple "threads" (tasks)
    let err1 = Arc::clone(&err);
    let err2 = Arc::clone(&err);
    let err3 = Arc::clone(&err);

    assert_eq!(err1.to_string(), err2.to_string());
    assert_eq!(err2.to_string(), err3.to_string());

    assert!(err1.source().is_some());
    assert!(err2.source().is_some());
    assert!(err3.source().is_some());

    assert_eq!(
        err1.context().tags.get("pool_size"),
        Some(&"10".to_string())
    );
}

#[test]
fn test_error_downcast_chain() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
    let enhanced = EnhancedError::Io {
        message: "failed to read".to_string(),
        source: io_err,
        context: ErrorContext::new(),
    };

    // Access source and verify its type
    if let Some(source) = enhanced.source() {
        // Source should be an IO error
        let source_str = source.to_string();
        assert!(source_str.contains("not found"));

        if let Some(io_source) = source.downcast_ref::<std::io::Error>() {
            assert_eq!(io_source.kind(), std::io::ErrorKind::NotFound);
        } else {
            panic!("Source should be downcastable to std::io::Error");
        }
    } else {
        panic!("Enhanced IO error should have a source");
    }
}
