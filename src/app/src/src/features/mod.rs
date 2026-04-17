//! # Features
//!
//! Vertical feature slices. Each feature is a self-contained module
//! that groups its use cases, adapters, DTOs, commands, and Tauri plugin
//! into a single directory.
//!
//! Features consume shared ports defined in `application/ports/` and
//! shared primitives from `core/` / `shared/`. They do not own cross-cutting
//! abstractions.

pub mod backup;
pub mod batch;
pub mod cache;
pub mod config;
pub mod credentials;
pub mod custom_model;
pub mod daily_notes;
pub mod download;
pub mod embedding;
pub mod extraction;
pub mod favorites;
pub mod file;
pub mod function_calling;
pub mod health;
pub mod huggingface;
pub mod initialization;
pub mod mentions;
pub mod metrics;
pub mod qa;
pub mod recent;
pub mod settings;
pub mod stats;
pub mod tags;
pub mod updates;
pub mod web;
