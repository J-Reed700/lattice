# Audit System Integration Guide

This guide shows how to integrate the audit logging system into the Tauri application.

## Application Startup Integration

Add audit logger initialization to your `main.rs`:

```rust
// src-tauri/src/main.rs
use vault_desktop::audit::{init_audit_logger, get_audit_logger, AuditAction, AuditResult, AuditEvent};
use vault_desktop::audit::sinks::SqliteAuditSink;
use vault_desktop::audit_success;

#[tokio::main]
async fn main() {
    // Initialize tracing first
    tracing_subscriber::fmt::init();

    // Initialize audit logger during app startup
    let app_data_dir = tauri::api::path::app_data_dir(&tauri::Config::default())
        .expect("Failed to get app data directory");

    let audit_db_path = app_data_dir.join("audit.db");

    match SqliteAuditSink::new(&audit_db_path).await {
        Ok(sink) => {
            init_audit_logger(vec![Box::new(sink)]).await;
            println!("Audit logging initialized at: {:?}", audit_db_path);
        }
        Err(e) => {
            eprintln!("Failed to initialize audit logging: {}", e);
            // Continue without audit logging or exit based on requirements
        }
    }

    // Log system startup
    let logger = get_audit_logger();
    let _ = audit_success!(logger, AuditAction::SystemStartup, "application").await;

    // Build and run Tauri app
    tauri::Builder::default()
        .setup(|app| {
            // Additional setup
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Your command handlers
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    // Log shutdown
    let _ = audit_success!(logger, AuditAction::SystemShutdown, "application").await;
    let _ = logger.close().await;
}
```

## Integration with Existing Commands

### File Indexing Command

```rust
// src-tauri/src/commands/indexing.rs
use vault_desktop::{audit_success, audit_failure};
use vault_desktop::audit::{get_audit_logger, AuditAction};

#[tauri::command]
pub async fn index_file(path: String) -> Result<String, String> {
    let logger = get_audit_logger();

    match perform_indexing(&path).await {
        Ok(result) => {
            // Log successful indexing
            let _ = audit_success!(logger, AuditAction::FileIndexed, &path,
                "chunks" => &result.chunk_count.to_string(),
                "size_bytes" => &result.file_size.to_string(),
                "duration_ms" => &result.duration_ms.to_string()
            ).await;

            Ok(result.message)
        }
        Err(e) => {
            // Log failed indexing
            let _ = audit_failure!(logger, AuditAction::FileIndexed, &path,
                &e.to_string(),
                "error_type" => "indexing_error"
            ).await;

            Err(e.to_string())
        }
    }
}
```

### Search Command

```rust
// src-tauri/src/commands/search.rs
use vault_desktop::audit_event;
use vault_desktop::audit::{get_audit_logger, AuditAction, AuditResult};

#[tauri::command]
pub async fn semantic_search(
    query: String,
    limit: Option<usize>,
    user_id: Option<String>,
) -> Result<Vec<SearchResult>, String> {
    let logger = get_audit_logger();
    let start = std::time::Instant::now();

    match perform_search(&query, limit.unwrap_or(10)).await {
        Ok(results) => {
            let duration_ms = start.elapsed().as_millis();

            // Log successful search with full details
            let _ = audit_event!(logger,
                action: AuditAction::SearchPerformed,
                result: AuditResult::success(),
                resource: &query,
                user: user_id.unwrap_or_else(|| "anonymous".to_string()),
                metadata: {
                    "results_count" => &results.len().to_string(),
                    "limit" => &limit.unwrap_or(10).to_string(),
                    "duration_ms" => &duration_ms.to_string()
                }
            ).await;

            Ok(results)
        }
        Err(e) => {
            let _ = audit_failure!(logger, AuditAction::SearchPerformed, &query,
                &e.to_string()
            ).await;

            Err(e.to_string())
        }
    }
}
```

### Credential Access Command

```rust
// src-tauri/src/commands/credentials.rs
use vault_desktop::{audit_success, audit_denied};
use vault_desktop::audit::{get_audit_logger, AuditAction};

#[tauri::command]
pub async fn get_api_key(service: String, user_id: String) -> Result<String, String> {
    let logger = get_audit_logger();

    // Check permissions
    if !has_permission(&user_id, &service).await {
        let _ = audit_denied!(logger, AuditAction::CredentialAccessed, &service,
            "Insufficient permissions",
            "user_id" => &user_id,
            "required_permission" => "credential_read"
        ).await;

        return Err("Permission denied".to_string());
    }

    match retrieve_credential(&service).await {
        Ok(credential) => {
            // Log credential access
            let _ = audit_success!(logger, AuditAction::CredentialAccessed, &service,
                "user_id" => &user_id
                // Note: DO NOT log the actual credential value
            ).await;

            Ok(credential)
        }
        Err(e) => Err(e.to_string()),
    }
}
```

### Configuration Changes

```rust
// src-tauri/src/commands/config.rs
use vault_desktop::audit_success;
use vault_desktop::audit::{get_audit_logger, AuditAction};
use serde_json::json;

#[tauri::command]
pub async fn update_config(
    key: String,
    value: serde_json::Value,
    user_id: Option<String>,
) -> Result<(), String> {
    let logger = get_audit_logger();

    let old_value = get_config_value(&key).await?;

    match set_config_value(&key, &value).await {
        Ok(()) => {
            // Log configuration change
            let _ = audit_success!(logger, AuditAction::ConfigChanged, &key,
                "old_value" => &old_value.to_string(),
                "new_value" => &value.to_string(),
                "changed_by" => &user_id.unwrap_or_else(|| "system".to_string())
            ).await;

            Ok(())
        }
        Err(e) => Err(e.to_string()),
    }
}
```

## Integration with File Watcher

```rust
// src-tauri/src/services/watcher.rs
use vault_desktop::audit_success;
use vault_desktop::audit::{get_audit_logger, AuditAction};
use notify::{Watcher, RecursiveMode, Event};

pub struct FileWatcher {
    // ... fields
}

impl FileWatcher {
    pub async fn start(&mut self, path: impl AsRef<Path>) -> Result<(), Box<dyn std::error::Error>> {
        let logger = get_audit_logger();

        // Start watching
        self.watcher.watch(path.as_ref(), RecursiveMode::Recursive)?;

        // Log watcher start
        let _ = audit_success!(logger, AuditAction::WatcherStarted,
            path.as_ref().to_string_lossy().as_ref(),
            "recursive" => "true"
        ).await;

        Ok(())
    }

    pub async fn stop(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let logger = get_audit_logger();

        // Stop watching
        self.watcher.unwatch(&self.watched_path)?;

        // Log watcher stop
        let _ = audit_success!(logger, AuditAction::WatcherStopped,
            self.watched_path.to_string_lossy().as_ref()
        ).await;

        Ok(())
    }
}
```

## Query Audit Logs from Frontend

Create a command to query audit logs:

```rust
// src-tauri/src/commands/audit.rs
use vault_desktop::audit::{get_audit_logger, AuditEvent};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub struct AuditEventDTO {
    pub id: String,
    pub timestamp: String,
    pub user_id: Option<String>,
    pub action: String,
    pub resource_id: Option<String>,
    pub result: String,
    pub metadata: std::collections::HashMap<String, String>,
}

impl From<AuditEvent> for AuditEventDTO {
    fn from(event: AuditEvent) -> Self {
        Self {
            id: event.id.to_string(),
            timestamp: event.timestamp.to_rfc3339(),
            user_id: event.user_id,
            action: format!("{:?}", event.action),
            resource_id: event.resource_id,
            result: format!("{:?}", event.result),
            metadata: event.metadata,
        }
    }
}

#[tauri::command]
pub async fn get_audit_logs(
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<Vec<AuditEventDTO>, String> {
    let logger = get_audit_logger();

    let events = logger
        .query(limit.unwrap_or(100), offset.unwrap_or(0))
        .await
        .map_err(|e| e.to_string())?;

    Ok(events.into_iter().map(AuditEventDTO::from).collect())
}

#[tauri::command]
pub async fn get_audit_logs_count() -> Result<usize, String> {
    let logger = get_audit_logger();
    logger.count().await.map_err(|e| e.to_string())
}
```

Register the command:

```rust
// src-tauri/src/main.rs
tauri::Builder::default()
    .invoke_handler(tauri::generate_handler![
        get_audit_logs,
        get_audit_logs_count,
        // ... other commands
    ])
    // ...
```

## Frontend Integration (React/TypeScript)

```typescript
// src/hooks/useAuditLogs.ts
import { invoke } from '@tauri-apps/api/core';

interface AuditEvent {
  id: string;
  timestamp: string;
  user_id?: string;
  action: string;
  resource_id?: string;
  result: string;
  metadata: Record<string, string>;
}

export function useAuditLogs() {
  const [logs, setLogs] = useState<AuditEvent[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(false);

  const fetchLogs = async (limit = 100, offset = 0) => {
    setLoading(true);
    try {
      const [events, count] = await Promise.all([
        invoke<AuditEvent[]>('get_audit_logs', { limit, offset }),
        invoke<number>('get_audit_logs_count'),
      ]);

      setLogs(events);
      setTotal(count);
    } catch (error) {
      console.error('Failed to fetch audit logs:', error);
    } finally {
      setLoading(false);
    }
  };

  return { logs, total, loading, fetchLogs };
}
```

```typescript
// src/components/AuditLogViewer.tsx
import React, { useEffect } from 'react';
import { useAuditLogs } from '../hooks/useAuditLogs';

export const AuditLogViewer: React.FC = () => {
  const { logs, total, loading, fetchLogs } = useAuditLogs();

  useEffect(() => {
    fetchLogs();
  }, []);

  if (loading) return <div>Loading audit logs...</div>;

  return (
    <div className="audit-log-viewer">
      <h2>Audit Logs ({total} total)</h2>
      <table>
        <thead>
          <tr>
            <th>Timestamp</th>
            <th>Action</th>
            <th>Resource</th>
            <th>Result</th>
            <th>User</th>
          </tr>
        </thead>
        <tbody>
          {logs.map((log) => (
            <tr key={log.id}>
              <td>{new Date(log.timestamp).toLocaleString()}</td>
              <td>{log.action}</td>
              <td>{log.resource_id || '-'}</td>
              <td>{log.result}</td>
              <td>{log.user_id || '-'}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
};
```

## Testing Integration

```rust
// src-tauri/tests/integration_audit.rs
use vault_desktop::audit::{AuditLogger, AuditAction, AuditResult};
use vault_desktop::audit::sinks::MemoryAuditSink;

#[tokio::test]
async fn test_file_indexing_creates_audit_log() {
    // Setup audit logger with memory sink for testing
    let logger = AuditLogger::new();
    let sink = MemoryAuditSink::new(100);
    logger.add_sink(Box::new(sink.clone())).await;

    // Perform action that should create audit log
    let result = index_file("test.txt".to_string()).await;
    assert!(result.is_ok());

    // Verify audit log was created
    let events = sink.get_all().await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].action, AuditAction::FileIndexed);
    assert_eq!(events[0].resource_id.as_deref(), Some("test.txt"));
    assert!(events[0].result.is_success());
}
```

## Best Practices

### 1. Always Log Important Actions

```rust
// DO log:
- File operations (create, update, delete, index)
- Search queries
- Credential access
- Configuration changes
- Authentication attempts
- Data exports/imports
- System events (startup, shutdown)

// DON'T log:
- Read-only queries that don't access sensitive data
- UI state changes
- Cached data retrieval
- Performance metrics (use separate metrics system)
```

### 2. Include Relevant Metadata

```rust
// Good - includes context
audit_success!(logger, AuditAction::FileIndexed, path,
    "size_bytes" => file_size,
    "duration_ms" => processing_time,
    "chunks" => chunk_count
).await;

// Bad - missing context
audit_success!(logger, AuditAction::FileIndexed, path).await;
```

### 3. Handle Errors Gracefully

```rust
// Log the audit event but don't fail the operation if audit logging fails
if let Err(e) = audit_success!(logger, action, resource).await {
    tracing::warn!("Failed to log audit event: {}", e);
    // Continue with the operation
}
```

### 4. Sanitize Sensitive Data

```rust
// DON'T log sensitive data
// BAD:
audit_success!(logger, AuditAction::CredentialAccessed, service,
    "api_key" => api_key  // ❌ Never log credentials!
).await;

// GOOD:
audit_success!(logger, AuditAction::CredentialAccessed, service,
    "key_length" => api_key.len().to_string()  // ✅ Log metadata only
).await;
```

### 5. Use Appropriate Result Types

```rust
// Success
audit_success!(logger, action, resource).await;

// Failure (expected errors)
audit_failure!(logger, action, resource, "File not found").await;

// Denied (permission/authorization issues)
audit_denied!(logger, action, resource, "Insufficient permissions").await;
```

## Troubleshooting

### Audit logs not appearing

1. Check that audit logger is initialized before use
2. Verify the database path is writable
3. Check for errors in the logs
4. Ensure `await` is used on async audit calls

### Performance issues

1. Use memory sink for testing, not production
2. Set appropriate capacity on memory sinks
3. Query with pagination for large result sets
4. Consider archiving old audit logs periodically

### Database locked errors (SQLite)

1. Ensure you're not exceeding connection pool limits
2. Use a separate database for audit logs
3. Consider using WAL mode for better concurrency

## Security Checklist

- [ ] Audit database file has restricted permissions (600 or 640)
- [ ] No sensitive data (passwords, keys, tokens) in metadata
- [ ] Audit logs are write-only for application code
- [ ] Log rotation/retention policy implemented
- [ ] Regular backups of audit logs
- [ ] Monitoring for suspicious patterns in audit logs
- [ ] Audit log tampering detection (checksums, signatures)

## Conclusion

The audit logging system provides comprehensive tracking of all important events in the application. By following this integration guide and best practices, you'll have a robust audit trail for security, compliance, and debugging purposes.
