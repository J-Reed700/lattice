//! # Download manager
//!
//! Orchestrates download sessions: accepts requests, queues them against a
//! concurrency limit, drives one transfer task per active session, and
//! publishes progress and lifecycle events.
//!
//! This module is a façade over focused submodules:
//!
//! - [`state`] — `DownloadManagerService` and its bookkeeping (active table,
//!   pending queue, auth tokens, event channel, stop signal).
//! - [`queue`] — draining the pending queue while slots are free.
//! - [`task`] — the life of one in-flight transfer, from resume-offset
//!   recovery through validation, checksum verification and finalisation.
//! - [`validation`] — resume arithmetic and downloaded-file checks.
//! - [`operations`] — the `DownloadManager` implementation callers drive.
//!
//! The port it implements, with `DownloadRequest`, `DownloadBatchItem` and
//! `DownloadEvent`, lives in `lattice_core::application::ports::download_manager`.
//! Domain types (`DownloadSession`, `DownloadError`, …) live in
//! `lattice_core::domain::download`; UI-facing events live in
//! `crate::features::download::events`.

mod operations;
mod queue;
mod state;
mod task;
mod validation;

#[cfg(test)]
mod tests;

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod pause_resume_tests;

pub use state::DownloadManagerService;
