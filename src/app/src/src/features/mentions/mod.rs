//! # Mentions feature
//!
//! Inline [[wiki-link]]-style references between documents. Extract /
//! create / update / delete / search / list-by-type / list-for-doc /
//! backlinks. Self-contained vertical slice.
//!
//! ## Public surface
//!
//! - `crate::features::mentions::dto` — mention DTOs
//! - `crate::features::mentions::entity` — Mention, MentionType
//! - `crate::features::mentions::mapper::MentionMapper`
//! - `crate::features::mentions::use_cases` — CRUD, search, list, backlinks
//! - `crate::features::mentions::repository::MentionRepository`
//! - `crate::features::mentions::commands` — Tauri command handlers
//! - `crate::features::mentions::plugin::init()` — Tauri plugin
//! - `crate::features::mentions::mocks::MockMentionRepository` — test mock
//!
//! Legacy trait has been migrated to DDD port (`MentionRepositoryPort`).
//! `MentionRepositoryPort` stays in `application/ports/`.

pub mod commands;
pub mod di;
pub mod dto;
pub mod entity;
pub mod mapper;
pub mod plugin;
pub mod repository;
pub mod use_cases;

#[cfg(test)]
pub mod mocks;
