//! # Batch feature
//!
//! Batch document import (file and URL) with job tracking, progress
//! monitoring, cancellation, retry of failed items, and history.
//!
//! ## File layout
//!
//! | File                              | Canonical module path                                            |
//! |-----------------------------------|------------------------------------------------------------------|
//! | `dto.rs`                          | `crate::application::dtos::batch_dto`                            |
//! | `use_cases/`                      | `crate::application::use_cases::batch`                           |
//! | `services/file_import.rs`         | `crate::infrastructure::services::batch_file_import`             |
//! | `services/url_import.rs`          | `crate::infrastructure::services::batch_url_import`              |
//! | `services/file_import_trait.rs`   | `crate::infrastructure::services::traits` (merged re-exports)    |
//! | `services/url_import_trait.rs`    | `crate::infrastructure::services::traits` (merged re-exports)    |
//! | `commands/file_import.rs`         | `crate::interfaces::commands::batch_file_import`                 |
//! | `commands/url_import.rs`          | `crate::interfaces::commands::batch_url_import`                  |
//! | `commands/history.rs`             | `crate::interfaces::commands::batch_history`                     |
//! | `plugin.rs`                       | `crate::plugins::batch_plugin`                                   |
//!
//! `BatchJobRepositoryPort` stays in `application/ports/`.
//!
//! This feature has two parallel service stacks (file import + URL
//! import) each with its own trait, service, and command — colocated
//! here under `services/` and `commands/` subdirectories.
