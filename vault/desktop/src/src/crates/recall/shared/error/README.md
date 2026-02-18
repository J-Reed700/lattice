# Enhanced Error Handling System

This module provides a comprehensive error handling system with full context chain preservation for the Vault Desktop application.

## Overview

The error handling system consists of two components:

1. **Legacy System** (`legacy.rs`) - Original `AppError` type for backward compatibility
2. **Enhanced System** (`enhanced.rs`) - New system with complete error chain preservation

## Key Features

### 1. Full Error Chain Preservation

The enhanced error system uses `thiserror` to preserve complete error chains through `#[source]` annotations:

```rust
use vault_desktop::error::enhanced::{EnhancedError, EnhancedResultExt};

async fn read_config(path: &str) -> Result<String, EnhancedError> {
    let content = tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("Failed to read config: {}", path))
        .add_tag("operation", "read_config")
        .add_tag("path", path)?;

    Ok(content)
}

// When an error occurs, you can see the full chain:
match read_config("config.toml").await {
    Err(err) => {
        eprintln!("Error chain:\n{}", err.chain_to_string());
        // Output:
        // Error in operation: Failed to read config: config.toml
        //   caused by: I/O operation failed: file not found
        //   caused by: No such file or directory (os error 2)
    }
}
```

### 2. Structured Context with Tags

Add key-value metadata to errors for observability:

```rust
let result = process_document(&doc)
    .add_tag("document_id", doc.id.to_string())
    .add_tag("user_id", user.id.to_string())
    .add_tag("operation", "index")
    .with_category(ErrorCategory::Retriable)?;

// Tags are preserved in the error context
if let Err(err) = result {
    let doc_id = err.context().tags.get("document_id");
    tracing::error!(
        error = %err,
        tags = ?err.context().tags,
        "Document processing failed"
    );
}
```

### 3. Error Categorization

Errors are automatically categorized for appropriate handling:

- **Retriable** - Transient failures that can be retried (network timeouts, queue full)
- **UserFixable** - Errors the user can fix (invalid input, permissions)
- **Fatal** - Critical errors requiring intervention (database corruption)
- **Transient** - Temporary issues (network problems, rate limits)

```rust
async fn process_with_retry(data: &Data) -> Result<(), EnhancedError> {
    let mut retries = 0;

    loop {
        match attempt_process(data).await {
            Ok(_) => return Ok(()),
            Err(err) if err.is_transient() && retries < 3 => {
                tracing::warn!("Transient error, retrying...");
                retries += 1;
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Err(err) if err.is_user_fixable() => {
                return Err(err.with_user_message(
                    "Please check your input and try again"
                ));
            }
            Err(err) => return Err(err),
        }
    }
}
```

### 4. User-Friendly Messages

Convert technical errors to user-friendly messages:

```rust
match operation().await {
    Err(err) => {
        // Log technical details
        tracing::error!(
            error = %err,
            chain = %err.chain_to_string(),
            "Operation failed"
        );

        // Show user-friendly message
        show_error_dialog(err.to_user_friendly_message());
    }
}

// Or set custom user messages:
let err = EnhancedError::Database { /* ... */ }
    .with_user_message("Unable to save your changes. Please try again.");
```

### 5. Helper Macros

Ergonomic macros for common error patterns:

```rust
use vault_desktop::{bail_with_context, ensure_with_context};

fn validate_input(value: i32) -> Result<(), EnhancedError> {
    // Early return with context
    if value < 0 {
        bail_with_context!("Value must be non-negative, got {}", value);
    }

    // Ensure condition with context
    ensure_with_context!(
        value <= 100,
        "Value {} exceeds maximum 100",
        value
    );

    Ok(())
}
```

## Module Structure

```
error/
├── mod.rs          # Module definition and conversions
├── legacy.rs       # Original AppError (backward compatible)
├── enhanced.rs     # Enhanced error system with context chains
├── examples.rs     # Comprehensive usage examples
├── tests.rs        # Test suite
└── README.md       # This file
```

## Migration Guide

### From Legacy to Enhanced

**Before (Legacy System):**
```rust
use vault_desktop::error::{AppError, Result, ResultExt};

fn read_file(path: &str) -> Result<String> {
    std::fs::read_to_string(path)
        .context("Failed to read file")?;
    Ok("content".to_string())
}
```

**After (Enhanced System):**
```rust
use vault_desktop::error::enhanced::{EnhancedError, Result, EnhancedResultExt};

fn read_file(path: &str) -> Result<String> {
    std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path))
        .add_tag("operation", "file_read")
        .add_tag("path", path)?;
    Ok("content".to_string())
}
```

### Backward Compatibility

The legacy `AppError` is still available and fully supported. You can convert between the two systems:

```rust
use vault_desktop::error::{AppError, enhanced::EnhancedError};

// EnhancedError -> AppError
let enhanced = EnhancedError::NotFound { /* ... */ };
let legacy: AppError = enhanced.into();

// AppError -> EnhancedError
let app_err = AppError::NotFound("not found".to_string());
let enhanced: EnhancedError = app_err.into();
```

## Error Types

### Enhanced Error Variants

- `Io` - I/O errors with source preservation
- `Database` - Database errors (SQLx) with source
- `Json` - JSON serialization errors with source
- `OnnxRuntime` - ONNX runtime errors for embeddings
- `HttpRequest` - HTTP/network request errors
- `InvalidConfig` - Configuration errors
- `NotFound` - Resource not found
- `PermissionDenied` - Permission/access errors
- `InvalidInput` - User input validation errors
- `FileTooLarge` - File size limit exceeded
- `UnsupportedFileType` - Unsupported file format
- `ContentExtraction` - Content extraction failures
- `EmbeddingFailed` - Embedding generation errors
- `QueueFull` - Processing queue at capacity
- `Network` - Network/connectivity errors
- `Timeout` - Operation timeout errors
- `CircuitBreakerOpen` - Circuit breaker activated
- `Wrapped` - Wrapped lower-level error with context
- `Other` - Generic error with custom message

## Best Practices

### 1. Always Preserve Error Chains

```rust
// ✅ GOOD - Preserves error chain
let content = std::fs::read_to_string(path)
    .with_context(|| format!("Failed to read: {}", path))?;

// ❌ BAD - Loses error chain
let content = std::fs::read_to_string(path)
    .map_err(|_| EnhancedError::Other {
        message: "File read failed".to_string(),
        context: ErrorContext::new(),
    })?;
```

### 2. Add Relevant Context Tags

```rust
// ✅ GOOD - Rich context for debugging
process_document(&doc)
    .add_tag("document_id", doc.id.to_string())
    .add_tag("user_id", user.id.to_string())
    .add_tag("file_size", doc.size.to_string())
    .add_tag("mime_type", &doc.mime_type)?;

// ❌ BAD - Missing context
process_document(&doc)?;
```

### 3. Use Appropriate Error Categories

```rust
// ✅ GOOD - Categorized for proper handling
return Err(EnhancedError::InvalidInput {
    message: "Email format invalid".to_string(),
    context: ErrorContext::new()
        .with_category(ErrorCategory::UserFixable)
        .with_user_message("Please enter a valid email address"),
});

// ❌ BAD - Uncategorized
return Err(EnhancedError::Other {
    message: "Email invalid".to_string(),
    context: ErrorContext::new(),
});
```

### 4. Set User-Friendly Messages

```rust
// ✅ GOOD - Clear user message
Err(err) => {
    return Err(err.with_user_message(
        "Unable to save your document. Please check disk space and try again."
    ));
}

// ❌ BAD - Technical message exposed to user
Err(err) => return Err(err), // Shows "sqlx::Error: connection pool exhausted"
```

### 5. Log Full Error Chains

```rust
// ✅ GOOD - Complete error information logged
match process().await {
    Err(err) => {
        tracing::error!(
            error = %err,
            error_chain = %err.chain_to_string(),
            tags = ?err.context().tags,
            category = ?err.category(),
            "Processing failed"
        );
    }
}

// ❌ BAD - Only top-level error logged
match process().await {
    Err(err) => {
        tracing::error!("Error: {}", err);
    }
}
```

## Examples

See `examples.rs` for comprehensive examples demonstrating:

- Full error chain preservation through async calls
- Context propagation across multiple layers
- Converting technical errors to user-friendly messages
- Using helper macros
- Error categorization and retry strategies
- Wrapping third-party library errors
- Complex context chaining
- Error recovery patterns with fallbacks

## Testing

Run the test suite:

```bash
cd vault/desktop/src-tauri
cargo test error::
```

The test suite includes:

- Error chain preservation tests
- Context propagation tests
- Error categorization tests
- User-friendly message tests
- Conversion tests between legacy and enhanced
- Extension trait tests
- Macro functionality tests
- Integration tests for async error propagation

## Performance Considerations

- Error context tags use `HashMap<String, String>` - minimal overhead
- Error chains are preserved via `#[source]` - zero-cost abstraction
- User-friendly messages are generated on-demand - not stored
- Tags are only collected when errors occur - happy path has no overhead

## Future Enhancements

Potential improvements:

1. Error analytics/telemetry integration
2. Structured error codes for error tracking systems
3. Error recovery suggestions based on error type
4. Integration with distributed tracing (OpenTelemetry)
5. Error serialization for cross-process error propagation

## References

- [thiserror documentation](https://docs.rs/thiserror/)
- [Error Handling Best Practices](https://doc.rust-lang.org/book/ch09-00-error-handling.html)
- [Rust Error Handling Survey](https://blog.yoshuawuyts.com/error-handling-survey/)

---

**Last Updated:** 2025-11-15
**Maintainer:** Recall Development Team
