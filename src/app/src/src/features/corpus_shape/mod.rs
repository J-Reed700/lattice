//! # Corpus-shape feature (Phase 5.3)
//!
//! Clusters a lattice's documents over their mean-pooled chunk embeddings
//! using HDBSCAN, assigns each cluster an LLM-generated 3-5 word label,
//! and caches labels across re-runs via a stability fingerprint so small
//! membership churn doesn't reshuffle the whole label set.
//!
//! Scope: **backend-only** in 5.3. The one user-visible artifact is a
//! debug JSON file written via `cluster_vault_debug` for Josh to eyeball
//! before any UI (Phase 5.4) is built on top.
//!
//! ## Public surface
//!
//! - `entity` — `Cluster`, `ClusterRun`, `ClusterMember`, `LabelSource`
//! - `clustering` — pure HDBSCAN stage + mean-pool helper
//! - `fingerprint` — stability fingerprint + Jaccard similarity
//! - `labeling` — LLM labeling prompt + response parsing
//! - `repository` — `ClusterRepositoryPort` + `SqliteClusterRepository`
//! - `use_cases::RunClusteringUseCase` — orchestrates the whole pipeline
//! - `commands` — Tauri commands (`cluster_vault_debug`, `cluster_vault_run`,
//!   `list_clusters`)
//! - `plugin::init()` — Tauri plugin registration
//!
//! ## Non-goals (for 5.3)
//!
//! - Automatic filing / moving documents into clusters.
//! - A scheduled / background clustering job.
//! - UI surfaces — Phase 5.4 adds the FileBrowser integration.
//! - Tuning knobs in settings — Josh tunes the constants directly for 5.3.

pub mod clustering;
pub mod commands;
pub mod entity;
pub mod fingerprint;
pub mod labeling;
pub mod plugin;
pub mod repository;
pub mod use_cases;

#[cfg(test)]
mod tests;
