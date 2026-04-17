//! # Favorites feature
//!
//! User-starred documents. Add / remove / list / check.
//! Self-contained vertical slice.
//!
//! ## Public surface
//!
//! - `crate::features::favorites::dto` — `FavoriteDto`, request/response DTOs
//! - `crate::features::favorites::mapper::FavoriteMapper`
//! - `crate::features::favorites::use_cases` — Add/Remove/List/IsFavorite
//! - `crate::features::favorites::repository::FavoritesRepository`
//! - `crate::features::favorites::commands` — Tauri command handlers
//! - `crate::features::favorites::plugin::init()` — Tauri plugin
//!
//! `FavoritesRepositoryPort` stays in `application/ports/`.

pub mod commands;
pub mod dto;
pub mod mapper;
pub mod plugin;
pub mod repository;
pub mod use_cases;
