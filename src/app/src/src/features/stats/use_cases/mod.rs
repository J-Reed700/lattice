//! System statistics use cases.
//!
//! This module contains use cases for retrieving system-wide statistics
//! such as document counts, chunk counts, tag counts, and storage size.

pub mod get_system_stats;

pub use get_system_stats::GetSystemStatsUseCase;
