//! # Tags feature
//!
//! Document tagging. Create / update / delete / apply / remove /
//! search / auto-generate tags. Includes AI-powered tag generation
//! via LLM. Self-contained vertical slice.
//!
//! ## Public surface
//!
//! - `crate::features::tags::entity::Tag` — domain entity
//! - `crate::features::tags::dto` — DTOs (TagDto, CreateTagRequestDto, …)
//! - `crate::features::tags::mapper::TagMapper` — application mapper
//! - `crate::features::tags::persistence_mapper::{TagMapper, TagModel}` —
//!   infrastructure mapper (DB model ↔ entity)
//! - `crate::features::tags::use_cases` — CRUD, apply/remove, search,
//!   auto-generate use cases
//! - `crate::features::tags::generator::TagGenerator` — LLM prompt helpers
//! - `crate::features::tags::repository::TagRepository`
//! - `crate::features::tags::service::TagService` — DocumentLockGuard helper
//! - `crate::features::tags::service_impl::TagServiceImpl`
//! - `crate::features::tags::{TagServiceTrait, TagRepositoryTrait}` — traits
//! - `crate::features::tags::{MockTagService, MockTagRepository}` — test mocks
//! - `crate::features::tags::commands` — Tauri command handlers
//! - `crate::features::tags::plugin::init()` — Tauri plugin

pub mod commands;
pub mod dto;
pub mod entity;
pub mod generator;
pub mod mapper;
pub mod persistence_mapper;
pub mod plugin;
pub mod repository;
pub mod service;
pub mod service_impl;
pub mod trait_def;
pub mod use_cases;

#[cfg(test)]
pub mod mocks;

// Re-export public traits at the feature root.
pub use trait_def::{TagRepositoryTrait, TagServiceTrait};
