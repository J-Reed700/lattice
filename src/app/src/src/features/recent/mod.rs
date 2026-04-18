//! # Recent documents feature
//!
//! Track and list recently-accessed documents. No dedicated Tauri
//! plugin — commands are exposed through the file plugin.
//! Self-contained vertical slice.
//!
//! ## Public surface
//!
//! - `crate::features::recent::dto` — `RecentDocumentDto`,
//!   `GetRecentDocumentsRequestDto`, `GetRecentDocumentsResponseDto`
//! - `crate::features::recent::mapper::RecentDocumentMapper`
//! - `crate::features::recent::use_cases` — `GetRecentDocumentsUseCase`,
//!   `TrackAccessUseCase`, `ClearRecentHistoryUseCase`
//! - `crate::features::recent::repository::RecentDocumentsRepository`
//! - `crate::features::recent::commands` — Tauri command handlers
//!
//! `RecentDocumentsRepositoryPort` stays in `application/ports/`.

pub mod commands;
pub mod di;
pub mod dto;
pub mod mapper;
pub mod repository;
pub mod use_cases;
