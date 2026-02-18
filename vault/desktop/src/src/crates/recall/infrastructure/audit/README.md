# Audit Logging System

A production-ready audit logging system for the Tauri desktop application, providing comprehensive tracking of security-relevant events and user actions.

## Features

- **Flexible Event Types**: Pre-defined actions covering file operations, searches, credential access, configuration changes, and more
- **Multiple Storage Backends**: SQLite for persistent storage, in-memory for testing
- **Async-First Design**: Built on `tokio` and `async-trait` for high performance
- **Structured Logging**: Integrates with `tracing` for consistent log output
- **Type-Safe API**: Leverages Rust's type system for correctness
- **Easy-to-Use Macros**: Convenient macros for common logging patterns
- **Comprehensive Testing**: Full test coverage for all components

## Architecture

```
audit/
├── event.rs          # Event types and builders
├── logger.rs         # Core logger and AuditSink trait
├── sinks/
│   ├── sqlite.rs     # SQLite persistent storage
│   └── memory.rs     # In-memory storage for testing
└── mod.rs            # Public API and macros
```

### Components

#### AuditEvent

Represents a single audit event with:
- Unique ID (UUID)
- Timestamp (UTC)
- Optional user ID
- Action type (enum)
- Optional resource ID
- Result (Success/Failure/Denied)
- Metadata (key-value pairs)

#### AuditAction

Predefined action types:
- `FileIndexed`, `FileUpdated`, `FileDeleted`
- `SearchPerformed`, `QuestionAnswered`
- `CredentialAccessed`, `CredentialStored`, `CredentialDeleted`
- `ConfigChanged`
- `BackupCreated`, `BackupRestored`
- `WatcherStarted`, `WatcherStopped`
- `ModelLoaded`, `CacheCleared`
- `DataExported`, `DataImported`
- `AuthAttempt`
- `SystemStartup`, `SystemShutdown`
- `Custom(String)` - for application-specific actions

#### AuditResult

Result types:
- `Success` - Action completed successfully
- `Failure { reason }` - Action failed
- `Denied { reason }` - Action was denied (e.g., permission denied)

#### AuditSink Trait

Async trait for implementing custom storage backends:

```rust
#[async_trait]
pub trait AuditSink: Send + Sync {
    async fn log(&self, event: &AuditEvent) -> Result<()>;
    async fn flush(&self) -> Result<()>;
    async fn query(&self, limit: usize, offset: usize) -> Result<Vec<AuditEvent>>;
    async fn count(&self) -> Result<usize>;
    async fn close(&self) -> Result<()>;
}
```

## Usage

### Basic Setup

```rust
use vault_desktop::audit::{
    init_audit_logger, get_audit_logger,
    AuditEvent, AuditAction, AuditResult,
    sinks::SqliteAuditSink,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize with SQLite sink
    let db_path = "/path/to/audit.db";
    let sqlite_sink = SqliteAuditSink::new(db_path).await?;

    init_audit_logger(vec![
        Box::new(sqlite_sink)
    ]).await;

    // Use global logger
    let logger = get_audit_logger();

    // Log an event
    let event = AuditEvent::new(
        AuditAction::FileIndexed,
        AuditResult::success()
    )
    .with_resource_id("/path/to/document.txt")
    .with_metadata("size", "1024")
    .with_metadata("type", "text/plain");

    logger.log(event).await?;

    Ok(())
}
```

### Using Macros

The audit module provides convenient macros for common patterns:

#### audit_success!

Log a successful action:

```rust
use vault_desktop::{audit_success, audit::AuditAction};

let logger = get_audit_logger();

// Simple success
audit_success!(logger, AuditAction::FileIndexed, "/path/to/file.txt").await?;

// With metadata
audit_success!(logger, AuditAction::SearchPerformed, "machine learning",
    "results" => "25",
    "duration_ms" => "180"
).await?;
```

#### audit_failure!

Log a failed action:

```rust
use vault_desktop::{audit_failure, audit::AuditAction};

let logger = get_audit_logger();

audit_failure!(logger, AuditAction::FileIndexed, "/path/to/file.txt",
    "Permission denied",
    "error_code" => "403"
).await?;
```

#### audit_denied!

Log a denied action:

```rust
use vault_desktop::{audit_denied, audit::AuditAction};

let logger = get_audit_logger();

audit_denied!(logger, AuditAction::CredentialAccessed, "api_key",
    "Insufficient permissions",
    "user_role" => "user",
    "required_role" => "admin"
).await?;
```

#### audit_event!

Log a custom event with full control:

```rust
use vault_desktop::{audit_event, audit::{AuditAction, AuditResult}};

let logger = get_audit_logger();

audit_event!(logger,
    action: AuditAction::SearchPerformed,
    result: AuditResult::success(),
    resource: "semantic search",
    user: "user123",
    metadata: {
        "query" => "machine learning",
        "results" => "25",
        "duration_ms" => "180"
    }
).await?;
```

### Event Builder Pattern

For complex events, use the builder pattern:

```rust
use vault_desktop::audit::{AuditEvent, AuditAction, AuditResult};

let event = AuditEvent::builder()
    .action(AuditAction::SearchPerformed)
    .result(AuditResult::success())
    .user_id("user123")
    .resource_id("semantic_search")
    .metadata("query", "rust programming")
    .metadata("limit", "10")
    .metadata("duration_ms", "250")
    .build();

logger.log(event).await?;
```

### Querying Audit Events

```rust
// Query recent events
let events = logger.query(100, 0).await?;

for event in events {
    println!("Event: {:?} at {}", event.action, event.timestamp);
    if let Some(resource) = event.resource_id {
        println!("  Resource: {}", resource);
    }
}

// Count total events
let total = logger.count().await?;
println!("Total audit events: {}", total);
```

### Multiple Sinks

You can configure multiple sinks to write events to different destinations:

```rust
use vault_desktop::audit::sinks::{SqliteAuditSink, MemoryAuditSink};

let logger = AuditLogger::new();

// Add SQLite sink for persistence
let sqlite_sink = SqliteAuditSink::new("/path/to/audit.db").await?;
logger.add_sink(Box::new(sqlite_sink)).await;

// Add memory sink for in-app queries
let memory_sink = MemoryAuditSink::new(1000);
logger.add_sink(Box::new(memory_sink)).await;

// Events will be written to both sinks
```

### Custom Sinks

Implement the `AuditSink` trait for custom storage backends:

```rust
use vault_desktop::audit::{AuditSink, AuditEvent};
use vault_desktop::error::Result;
use async_trait::async_trait;

struct CustomSink {
    // Your implementation
}

#[async_trait]
impl AuditSink for CustomSink {
    async fn log(&self, event: &AuditEvent) -> Result<()> {
        // Write event to custom storage
        Ok(())
    }

    async fn flush(&self) -> Result<()> {
        // Flush any buffers
        Ok(())
    }

    async fn query(&self, limit: usize, offset: usize) -> Result<Vec<AuditEvent>> {
        // Query events
        Ok(Vec::new())
    }

    async fn count(&self) -> Result<usize> {
        // Count events
        Ok(0)
    }

    async fn close(&self) -> Result<()> {
        // Cleanup
        Ok(())
    }
}
```

## Storage Backends

### SQLite Sink

**Features:**
- Persistent storage with SQLite database
- Automatic schema initialization
- Indexed queries for fast retrieval
- Supports connection pooling via `SqlitePool`

**Schema:**
```sql
CREATE TABLE audit_events (
    id TEXT PRIMARY KEY,
    timestamp TEXT NOT NULL,
    user_id TEXT,
    action TEXT NOT NULL,
    resource_id TEXT,
    result TEXT NOT NULL,
    metadata TEXT NOT NULL
);

-- Indexes for common queries
CREATE INDEX idx_audit_timestamp ON audit_events(timestamp);
CREATE INDEX idx_audit_action ON audit_events(action);
CREATE INDEX idx_audit_user_id ON audit_events(user_id);
CREATE INDEX idx_audit_resource_id ON audit_events(resource_id);
```

**Usage:**
```rust
use vault_desktop::audit::sinks::SqliteAuditSink;

// Create new sink with path
let sink = SqliteAuditSink::new("/path/to/audit.db").await?;

// Or use existing pool
let pool = SqlitePool::connect("sqlite:audit.db").await?;
let sink = SqliteAuditSink::from_pool(pool).await?;
```

### Memory Sink

**Features:**
- Fast in-memory storage
- Configurable capacity with automatic eviction (FIFO)
- Ideal for testing and temporary storage
- Thread-safe with `Arc<RwLock<Vec<AuditEvent>>>`

**Usage:**
```rust
use vault_desktop::audit::sinks::MemoryAuditSink;

// Create with capacity
let sink = MemoryAuditSink::new(1000);

// Create with unlimited capacity (use carefully!)
let sink = MemoryAuditSink::unlimited();

// Get all events
let events = sink.get_all().await;

// Clear all events
sink.clear().await;
```

## Integration with Tracing

The audit logger integrates with the `tracing` framework for structured logging:

```rust
use tracing::info;

// Audit events are automatically logged to tracing
logger.log(event).await?;

// This creates a tracing log entry like:
// INFO audit::logger: Audit event logged
//   event_id=123e4567-e89b-12d3-a456-426614174000
//   action=FileIndexed
//   result=Success
//   user_id=Some("user123")
//   resource_id=Some("/path/to/file.txt")
```

## Error Handling

The audit system uses the application's `AppError` type for error handling:

```rust
use vault_desktop::error::{AppError, Result};

// All audit operations return Result<T>
match logger.log(event).await {
    Ok(()) => println!("Event logged successfully"),
    Err(AppError::Database(msg)) => eprintln!("Database error: {}", msg),
    Err(e) => eprintln!("Error: {}", e),
}
```

### Resilience

The logger is designed to be resilient:
- If one sink fails, others continue to work
- Individual sink failures are logged as warnings
- Only if all sinks fail does the log operation return an error

## Performance Considerations

### SQLite Sink
- Uses connection pooling for efficiency
- Prepared statements for all queries
- Indexes on commonly queried columns
- Async I/O to avoid blocking

### Memory Sink
- Lock-free reads when possible
- Configurable capacity to limit memory usage
- Automatic eviction of old events (FIFO)

### Best Practices
1. Use SQLite for production persistence
2. Use memory sink for testing and development
3. Set appropriate capacity limits on memory sinks
4. Flush periodically if using buffered custom sinks
5. Query with pagination for large result sets

## Testing

The audit system includes comprehensive tests:

```bash
# Run all audit tests
cargo test --lib audit

# Run specific test module
cargo test --lib audit::event
cargo test --lib audit::logger
cargo test --lib audit::sinks::sqlite
cargo test --lib audit::sinks::memory
```

### Testing with Memory Sink

```rust
#[tokio::test]
async fn test_my_feature() {
    use vault_desktop::audit::sinks::MemoryAuditSink;

    let logger = AuditLogger::new();
    let sink = MemoryAuditSink::new(100);
    logger.add_sink(Box::new(sink.clone())).await;

    // Perform actions that generate audit events
    my_feature().await;

    // Verify events were logged
    let events = sink.get_all().await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].action, AuditAction::FileIndexed);
}
```

## Security Considerations

1. **Access Control**: Audit logs should be write-only for most code
2. **Data Sanitization**: Be careful about logging sensitive data in metadata
3. **Tamper Protection**: SQLite database should have appropriate file permissions
4. **Retention Policy**: Implement log rotation and retention policies
5. **Encryption**: Consider encrypting the SQLite database for sensitive environments

## Examples

See the test modules in each file for comprehensive examples:
- `event.rs` - Event creation and manipulation
- `logger.rs` - Logger usage and sink management
- `sinks/sqlite.rs` - SQLite sink operations
- `sinks/memory.rs` - Memory sink operations
- `mod.rs` - Macro usage examples

## Future Enhancements

Potential future improvements:
- Remote sink for centralized logging
- File-based sink with rotation
- Filtering and querying by action type, user, date range
- Export to various formats (JSON, CSV)
- Real-time event streaming via channels
- Encryption for sensitive events
- Compression for long-term storage

## License

Part of the Recall project - see project LICENSE file.

## Contributing

When adding new audit actions:
1. Add the variant to `AuditAction` enum
2. Update the `description()` method
3. Add tests for the new action
4. Document in this README
5. Update any relevant integration points

## Support

For issues or questions about the audit system, please refer to the main project documentation or open an issue in the project repository.
