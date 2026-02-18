//! Interfaces Layer - External Boundaries
//!
//! This is the outermost layer in the DDD architecture.
//! It contains adapters for external systems to interact with the application:
//!
//! - **Commands**: Tauri IPC command handlers (thin controllers)
//! - **Event Handlers**: Domain and file system event processors
//! - **Dependency Injection**: Container that wires everything together
//!
//! ## Layer Responsibilities
//!
//! The Interfaces layer:
//! 1. Receives external requests (Tauri commands, file system events)
//! 2. Applies cross-cutting concerns (rate limiting, validation, audit logging)
//! 3. Delegates to Application Use Cases
//! 4. Maps responses back to external format
//!
//! ## Command Pattern
//!
//! Commands should be < 50 lines and follow this structure:
//!
//! ```rust
//! #[tauri::command]
//! async fn my_command(
//!     container: State<'_, Container>,
//!     input: RequestDto,
//! ) -> Result<ResponseDto> {
//!     // 1. Rate limiting
//!     container.security_context().rate_limiters.operation.check()?;
//!
//!     // 2. Input validation
//!     container.security_context().input_validator.validate(&input)?;
//!
//!     // 3. Get use case
//!     let use_case = container.my_use_case();
//!
//!     // 4. Execute
//!     let response = use_case.execute(input).await?;
//!
//!     // 5. Audit log
//!     audit_success!(action = AuditAction::Operation, ...);
//!
//!     Ok(response)
//! }
//! ```
//!
//! ## Dependency Flow
//!
//! External Request → Command → Use Case → Domain/Ports → Infrastructure
//!
//! Commands NEVER:
//! - Contain business logic (use Application layer)
//! - Access database directly (use Repositories via Use Cases)
//! - Access infrastructure directly (use Ports via Use Cases)

// Commands module - now public to allow bindings generation
// Used by both IPC domain adapters (internal) and export_bindings binary (external)
pub mod commands;
pub mod di;
pub mod dto;
pub mod event_handlers;

// Re-export for convenience
pub use commands::*;
pub use di::Container;
pub use event_handlers::*;
