# Error Handling (`shared::error`)

Lattice keeps its serializable `AppError`, `DomainError`, and `ApplicationError`
contracts here. Domain and application paths re-export their error types for
compatibility. The IPC adapter maps these errors to the frontend contract.

## Module structure

```
shared/error/
├── mod.rs          # Re-exports the shared contracts and result helpers
├── domain.rs       # DomainError and its unit tests
├── application.rs  # ApplicationError and its unit tests
├── types.rs        # AppError, ErrorResponse, ResultExt, and their unit tests
└── examples.rs     # Examples compiled only for #[cfg(any(test, doc))]
```

## The types

- **`AppError`** (`types.rs`): a `Serialize` + `specta::Type` enum with
  semantic variants such as `Io { message, kind }`, `Database`, `NotFound`,
  `InvalidInput`, `Network`, `FileNotFound { path }`, `EmbeddingFailed`,
  `QueueFull`, `RateLimitExceeded`, `KeyringError`, `Backup*`, `Migration`,
  `AiModelsNotInstalled`, `ModelLoadFailed`, `ConcurrentModification`, and
  wrappers `Domain(Box<DomainError>)` (from this module's `domain.rs`) and
  `Application(Box<ApplicationError>)` (from this module's `application.rs`).
- **`Result<T>`** = `std::result::Result<T, AppError>`.
- **`ResultExt`**: `.context("...")` and `.with_context(|| ...)` on any
  `Result<T, E>` where `E: Into<AppError>`.
- **`ErrorResponse`**: `{ code, message, context, suggestions, recoverable,
  status_code, type }`, built with `ErrorResponse::from(app_error)`.

`From` conversions exist for `io::Error`, `serde_json::Error`, `sqlx::Error`,
`keyring::Error`, `anyhow::Error`, `ndarray::ShapeError`, `String`/`&str`,
`DomainTypeError`, `LLMError`, `DownloadError`, `DomainError` and
`ApplicationError`, so `?` works across layers. Driver conversions live in
`infrastructure/error_conversions.rs`; LLM and download conversions live beside
those types. The shared contracts do not import infrastructure or features.

Helpers on `AppError`: constructors (`not_found`, `validation_failed`,
`rate_limited`, `file_not_found`, `service_unavailable`, `invalid_input`,
`database`, `network`, `internal`), `with_context`, `with_suggestion`, and
classifiers `error_code()`, `to_user_friendly_message()`, `is_recoverable()`,
`is_user_fixable()`, `is_fatal()`, `get_context()`, `get_suggestions()`.

## What crosses IPC

Most Tauri commands return `Result<T, ApiError>`. `ApiError`
(`shared/ipc/mod.rs`) is `{ code: ErrorCode, message, details? }`, and
`From<AppError> for ApiError` maps each variant to an `ErrorCode`; domain and
application errors map through their own `From` impls. A smaller number of
commands still return `Result<T, AppError>` or `Result<T, String>`. Whatever a
command returns is what `src/lib/bindings.ts` types on the frontend.

```rust
use crate::shared::error::{AppError, Result, ResultExt};

async fn load(path: &std::path::Path) -> Result<String> {
    let text = tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("reading {}", path.display()))?;
    if text.is_empty() {
        return Err(AppError::invalid_input("file is empty"));
    }
    Ok(text)
}
```

## Rules

- No `unwrap`/`expect`/`panic!`/unchecked indexing in non-test code: the crate
  denies those clippy lints (`Cargo.toml` `[lints.clippy]`).
- Log the technical error with `tracing::error!`; return the user-facing one.

## Testing

```bash
cd src-tauri
cargo test --lib shared::error
```
