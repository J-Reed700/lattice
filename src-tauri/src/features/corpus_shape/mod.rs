//! # Corpus-shape feature
//!
//! Clusters a lattice's documents over their mean-pooled chunk embeddings
//! using HDBSCAN, assigns each cluster an LLM-generated 3-5 word label,
//! and caches labels across re-runs via a stability fingerprint so small
//! membership churn doesn't reshuffle the whole label set.
//!
//! `cluster_vault_debug` can write a JSON report for tuning and diagnostics.
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
//! ## Non-goals
//!
//! - Automatic filing / moving documents into clusters.
//! - A scheduled / background clustering job.
//! - UI surfaces.
//! - User-facing tuning controls.

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
