# Error Handling Guidelines

## Prohibited Patterns

❌ **Never use `.unwrap()` in production code**
```rust
let value = option.unwrap();  // Will panic on None!
```

✅ **Use proper error propagation**
```rust
let value = option.ok_or_else(|| AppError::NotFound("Value not found".to_string()))?;
```

## When to Use Each Pattern

### Option Handling

**Convert to Result**
```rust
// Static error
let value = option.ok_or(AppError::NotFound("Value not found".to_string()))?;

// Dynamic error with context
let value = option.ok_or_else(|| {
    AppError::NotFound(format!("User {} not found", user_id))
})?;
```

**Provide Fallback**
```rust
// Static default
let value = option.unwrap_or(0);

// Dynamic default
let value = option.unwrap_or_else(|| compute_default());

// Type default
let value = option.unwrap_or_default();
```

**Conditional Logic**
```rust
if let Some(x) = option {
    // Use x
} else {
    // Handle None case
}
```

**Explicit Handling**
```rust
match option {
    Some(x) => {
        // Success path
    }
    None => {
        // Failure path
    }
}
```

### Result Handling

**Propagate Errors**
```rust
// Simple propagation
let value = function_that_returns_result()?;

// Add context while propagating
let value = function_that_returns_result()
    .map_err(|e| AppError::ProcessingFailed(format!("Failed to process: {}", e)))?;
```

**Explicit Handling**
```rust
match result {
    Ok(x) => {
        // Success path
    }
    Err(e) => {
        // Error handling
        tracing::error!("Operation failed: {}", e);
        return Err(AppError::from(e));
    }
}
```

### Lock Handling

**Mutex/RwLock**
```rust
// Convert poisoned lock to error
let guard = mutex.lock()
    .map_err(|e| AppError::LockPoisoned(e.to_string()))?;

// Or handle explicitly
let guard = match mutex.lock() {
    Ok(guard) => guard,
    Err(poisoned) => {
        tracing::warn!("Lock poisoned, recovering data");
        poisoned.into_inner()
    }
};
```

### Collection Indexing

❌ **Avoid unchecked indexing**
```rust
let item = vec[index];  // Panics if out of bounds!
```

✅ **Use safe accessors**
```rust
// Option-based access
let item = vec.get(index)
    .ok_or(AppError::InvalidIndex)?;

// Iterator-based access
for item in vec.iter() {
    // Safe iteration
}

// Pattern matching
match vec.get(index) {
    Some(item) => { /* use item */ }
    None => { /* handle missing */ }
}
```

### File I/O

**Reading Files**
```rust
use std::fs;
use crate::domain::ValidatedFilePath;

// Validate path first
let path = ValidatedFilePath::new(path_buf)?;

// Read with proper error handling
let content = fs::read_to_string(path.as_path())
    .map_err(|e| AppError::FileReadError {
        path: path.to_string_lossy().to_string(),
        source: e,
    })?;
```

**Writing Files**
```rust
use std::fs;

fs::write(&path, content)
    .map_err(|e| AppError::FileWriteError {
        path: path.to_string_lossy().to_string(),
        source: e,
    })?;
```

### Database Operations

**Query Execution**
```rust
use sqlx::{Row, SqlitePool};

// Single row
let row = sqlx::query("SELECT * FROM documents WHERE id = ?")
    .bind(doc_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| match e {
        sqlx::Error::RowNotFound => AppError::NotFound("Document not found".to_string()),
        _ => AppError::DatabaseError(e.to_string()),
    })?;

// Optional row
let row = sqlx::query("SELECT * FROM documents WHERE id = ?")
    .bind(doc_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| AppError::DatabaseError(e.to_string()))?;

let doc = row
    .ok_or_else(|| AppError::NotFound("Document not found".to_string()))?;

// Multiple rows
let rows = sqlx::query("SELECT * FROM documents")
    .fetch_all(&pool)
    .await
    .map_err(|e| AppError::DatabaseError(e.to_string()))?;
```

### Async Operations

**Timeout Handling**
```rust
use tokio::time::{timeout, Duration};

let result = timeout(Duration::from_secs(30), async_operation())
    .await
    .map_err(|_| AppError::Timeout("Operation timed out".to_string()))?
    .map_err(|e| AppError::from(e))?;
```

**Concurrent Operations**
```rust
use futures::future::try_join_all;

let results = try_join_all(futures)
    .await
    .map_err(|e| AppError::ConcurrentOperationFailed(e.to_string()))?;
```

## Exceptions (Tests Only)

✅ **Unwraps are OK in tests**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() {
        let value = setup().unwrap();  // OK in tests
        assert_eq!(value, expected);
    }

    #[tokio::test]
    async fn test_async_operation() {
        let result = async_operation().await.unwrap();  // OK in tests
        assert!(result.is_valid());
    }
}
```

**Why tests can use unwrap:**
- Test failures are expected and intentional
- Stack traces aid debugging
- No production impact
- Panics fail the test (desired behavior)

## Error Context Best Practices

**Add Meaningful Context**
```rust
// ❌ Generic error
let file = fs::read_to_string(path)?;

// ✅ Contextual error
let file = fs::read_to_string(&path)
    .map_err(|e| AppError::FileReadError {
        path: path.to_string_lossy().to_string(),
        source: e,
    })?;
```

**Include Relevant Details**
```rust
// ❌ Missing context
return Err(AppError::InvalidInput("Bad format".to_string()));

// ✅ Helpful context
return Err(AppError::InvalidInput(
    format!("Invalid document format '{}': expected JSON or YAML", extension)
));
```

## Common Patterns by Use Case

### API/Command Handlers
```rust
#[tauri::command]
async fn my_command(
    container: State<'_, ServiceContainer>,
    input: String,
) -> Result<Response, AppError> {
    // Validate input
    let validated = container.security_context()
        .input_validator
        .validate_input(&input)?;

    // Get service
    let service = container.my_service();

    // Execute with error context
    service.do_something(validated)
        .await
        .map_err(|e| AppError::OperationFailed {
            operation: "my_command",
            details: e.to_string(),
        })
}
```

### Service Layer
```rust
impl MyService {
    pub async fn process(&self, input: &str) -> Result<Output, AppError> {
        // Validate business rules
        if input.is_empty() {
            return Err(AppError::InvalidInput("Input cannot be empty".to_string()));
        }

        // Database operations with context
        let data = self.repository.fetch(input)
            .await
            .map_err(|e| AppError::DataAccessError {
                entity: "document",
                operation: "fetch",
                source: e,
            })?;

        // Transform data
        let output = self.transform(data)
            .map_err(|e| AppError::ProcessingFailed(e.to_string()))?;

        Ok(output)
    }
}
```

### Domain Models
```rust
impl DocumentAggregate {
    pub fn new(path: ValidatedFilePath, content: String) -> Result<Self, AppError> {
        // Invariant: content must not be empty
        if content.is_empty() {
            return Err(AppError::InvalidDocument(
                "Document content cannot be empty".to_string()
            ));
        }

        // Invariant: must have at least one chunk
        let chunks = Self::chunk_content(&content)?;
        if chunks.is_empty() {
            return Err(AppError::InvalidDocument(
                "Document must produce at least one chunk".to_string()
            ));
        }

        Ok(Self {
            path,
            content,
            chunks,
        })
    }
}
```

## Lint Configuration

The project uses Clippy lints to enforce these patterns:

```toml
[lints.clippy]
unwrap_used = "warn"           # Warn on .unwrap()
expect_used = "warn"           # Warn on .expect()
panic = "deny"                 # Deny explicit panic!()
unwrap_in_result = "warn"      # Unwrap inside Result-returning functions
indexing_slicing = "warn"      # Array indexing without bounds check
missing_errors_doc = "warn"    # Document error cases
```

Tests are exempt from these lints via `.cargo/config.toml`:
```toml
[target.'cfg(test)']
rustflags = [
    "-A", "clippy::unwrap_used",
    "-A", "clippy::expect_used",
]
```

## Checking for Violations

```bash
# Check for unwraps in production code
cargo clippy -- -D clippy::unwrap_used -D clippy::expect_used

# Search for unwraps in source files (excluding tests)
rg "\.unwrap\(\)" src/ -g '!*test*.rs'

# Count unwraps by file
rg "\.unwrap\(\)" src/ -g '!*test*.rs' --count-matches | sort -t: -k2 -rn
```

## Migration Strategy

When replacing existing unwraps:

1. **Identify the context** - What could fail?
2. **Choose the right pattern** - See patterns above
3. **Add meaningful errors** - Include context
4. **Test both paths** - Success and error cases
5. **Update documentation** - Document error cases

**Example Migration:**
```rust
// Before
let user = users.get(&id).unwrap();

// After
let user = users.get(&id)
    .ok_or_else(|| AppError::NotFound(format!("User {} not found", id)))?;
```

## References

- [Rust Error Handling](https://doc.rust-lang.org/book/ch09-00-error-handling.html)
- [Clippy Lints](https://rust-lang.github.io/rust-clippy/master/)
- [thiserror Documentation](https://docs.rs/thiserror/)
- [anyhow Documentation](https://docs.rs/anyhow/)
