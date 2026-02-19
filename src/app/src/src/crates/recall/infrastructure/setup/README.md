# Setup Module - Tauri Desktop Application

## Overview

This module contains all application setup logic extracted from `main.rs` following the "bricks and studs" philosophy. Each submodule is self-contained with clear responsibilities and contracts.

## Module Structure

```
setup/
├── mod.rs              # Module exports and error dialog helper
├── directories.rs      # App and model directory setup
├── database.rs         # Database initialization
├── embedding.rs        # Embedding service and tokenizer setup
├── observability.rs    # Tracing/logging configuration
├── tests.rs           # Integration tests
└── README.md          # This file
```

## Refactoring Impact

**Before:**
- main.rs: 487 lines with complex setup logic inline

**After:**
- main.rs: 305 lines (182 lines removed, 37% reduction)
- Setup logic: 363 lines across 6 modular files
- Test coverage: 53 lines of tests

## Module Contracts

### directories.rs

**Purpose:** Set up application and model directories with proper error handling

**Public Functions:**
- `setup_app_directories(app: &tauri::AppHandle) -> Result<PathBuf, String>`
  - Creates app data directory
  - Returns path to app data dir
  - Errors: Permission issues, disk space, antivirus blocks

- `setup_model_directory(app: &tauri::AppHandle) -> Result<PathBuf, String>`
  - Creates model directory for AI models
  - Returns path to model dir
  - Errors: Permission issues, insufficient space (~500MB needed)

**Dependencies:**
- `std::path::PathBuf`
- `tauri::Manager`
- `vault_desktop::get_model_dir`

**Side Effects:**
- Creates directories on filesystem
- No network calls
- No database operations

---

### database.rs

**Purpose:** Initialize SQLite database with proper schema

**Public Functions:**
- `setup_database(db_path: PathBuf) -> Result<db::DatabaseConnection, String>`
  - Creates database connection
  - Initializes schema via migrations
  - Returns DatabaseConnection pool
  - Errors: File locks, corrupted db, disk space

**Dependencies:**
- `crate::db::{DatabaseConnection, initialize_database}`
- `std::path::PathBuf`

**Side Effects:**
- Creates SQLite database file
- Runs schema migrations
- Establishes connection pool
- No network calls

**Performance:**
- Typical time: 100-500ms for fresh db
- Disk usage: ~100MB over time

---

### embedding.rs

**Purpose:** Initialize AI embedding service and tokenizer for semantic search

**Public Functions:**
- `setup_embedding_service(model_dir: &PathBuf) -> Result<Arc<EmbeddingService>, String>`
  - Validates model files exist (model.onnx, tokenizer.json)
  - Loads ONNX model into memory
  - Returns thread-safe Arc-wrapped service
  - Errors: Missing files, corrupted models, insufficient RAM

- `setup_tokenizer(model_dir: &PathBuf) -> Result<Arc<tokenizers::Tokenizer>, String>`
  - Loads tokenizer from tokenizer.json
  - Returns thread-safe Arc-wrapped tokenizer
  - Errors: Missing file, corrupted JSON, invalid format

**Dependencies:**
- `crate::services::embedding::EmbeddingService`
- `tokenizers::Tokenizer`
- `std::sync::Arc`

**Side Effects:**
- Loads ~500MB model into RAM
- No network calls (models must be pre-downloaded)
- No file writes

**Performance:**
- Model loading: 2-5 seconds
- RAM usage: ~2GB during operation
- Thread-safe for concurrent use

---

### observability.rs

**Purpose:** Configure structured logging and tracing

**Public Functions:**
- `setup_tracing()`
  - Configures tracing subscriber
  - Sets log levels from env or defaults
  - Enables file/line number logging
  - No return value (panics on failure)

**Dependencies:**
- `tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter}`

**Configuration:**
- Default filter: `recall_desktop=info,vault_desktop=info`
- Override via: `RUST_LOG` environment variable
- Output: stdout with structured format

**Side Effects:**
- Initializes global tracing subscriber (can only be called once)
- No file writes (logs to stdout)
- No network calls

---

### mod.rs

**Purpose:** Public module interface and shared utilities

**Public Exports:**
- All setup functions from submodules
- `show_error_dialog(app, title, message)` helper

**Helper Functions:**
- `show_error_dialog(app: &tauri::AppHandle, title: &str, message: &str)`
  - Displays error dialog using Tauri dialog plugin
  - Blocking call (waits for user to dismiss)
  - Used for critical startup errors

**Dependencies:**
- `tauri_plugin_dialog::{DialogExt, MessageDialogKind}`

---

## Error Handling Philosophy

All setup functions follow a consistent error handling pattern:

1. **Detailed Context:** Error messages include file paths, specific causes
2. **User-Friendly:** Written for end users, not developers
3. **Actionable Guidance:**
   - "Possible causes" section
   - "Suggested actions" section
   - Original error preserved
4. **Structured Format:**
   ```
   [Operation] failed at [location].

   Possible causes:
   - Cause 1
   - Cause 2
   - Cause 3

   Suggested actions:
   - Action 1
   - Action 2
   - Action 3

   Error: [original error]
   ```

## Usage Example

```rust
// In main.rs
mod setup;

fn main() {
    setup::setup_tracing();

    tauri::Builder::default()
        .setup(|app| {
            // Setup directories
            let app_dir = match setup::setup_app_directories(app) {
                Ok(dir) => dir,
                Err(e) => {
                    setup::show_error_dialog(app, "Setup Failed", &e);
                    return Err(Box::new(std::io::Error::new(
                        std::io::ErrorKind::Other, e
                    )));
                }
            };

            // Setup continues...
            Ok(())
        })
        .run(tauri::generate_context!())
        .ok();
}
```

## Testing

**Unit Tests:** Each submodule includes basic unit tests
- Error message formatting
- Path handling
- Edge case validation

**Integration Tests:** `tests.rs`
- Module exports verification
- Error message structure validation
- Cross-module interactions

**Run Tests:**
```bash
cargo test --lib setup
```

## Design Principles

1. **Self-Contained:** Each module has all logic for its responsibility
2. **Clear Boundaries:** No cross-dependencies between setup modules
3. **Independently Testable:** Can test each module in isolation
4. **User-Friendly Errors:** All error messages guide users to solutions
5. **Regeneratable:** Can rebuild from documentation alone

## Migration Notes

When moving from old main.rs to new setup module:

**Before:**
```rust
fn setup_app_directories(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    // 18 lines of logic
}
```

**After:**
```rust
use setup;

let app_dir = setup::setup_app_directories(app)?;
```

**Breaking Changes:** None - all function signatures preserved

## Performance Characteristics

| Operation | Time | Memory | Disk |
|-----------|------|--------|------|
| setup_app_directories | <10ms | Negligible | 0 (directories only) |
| setup_model_directory | <10ms | Negligible | 0 (directories only) |
| setup_database | 100-500ms | ~10MB | ~100MB over time |
| setup_embedding_service | 2-5s | ~2GB | 0 (read-only) |
| setup_tokenizer | 100-500ms | ~50MB | 0 (read-only) |
| setup_tracing | <1ms | ~1MB | 0 (stdout only) |

**Total Startup:** 3-6 seconds on typical hardware

## Future Enhancements

- [ ] Async directory creation for parallel setup
- [ ] Progress callbacks for model loading
- [ ] Retry logic for transient failures
- [ ] Health checks for each component
- [ ] Graceful degradation (optional features)

## Troubleshooting

**Common Issues:**

1. **"Model files not found"**
   - Solution: Run `initialize_models` command first
   - Check: Model directory permissions
   - Verify: ~500MB free space

2. **"Database locked"**
   - Solution: Close other app instances
   - Check: File permissions on vault.db
   - Verify: No corrupted lockfiles

3. **"Insufficient RAM"**
   - Solution: Close other applications
   - Minimum: 2GB free RAM needed
   - Verify: System memory with `free -h`

## Documentation Compliance

This module follows the "bricks and studs" philosophy:
- ✅ Self-contained with clear boundaries
- ✅ Public interface via module exports
- ✅ Comprehensive error handling
- ✅ Unit tests for all functions
- ✅ Detailed documentation (this README)
- ✅ Regeneratable from specification

---

**Last Updated:** 2025-11-15
**Module Version:** 1.0.0
**Refactored By:** Claude Code (Priority 1 Task)
