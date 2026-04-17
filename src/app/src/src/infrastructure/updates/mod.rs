//! Updates Infrastructure
//!
//! Vertical-slice migration: the adapter now physically lives in
//! `features/updates/adapter.rs`. This module keeps the legacy
//! `crate::infrastructure::updates::update_checker_adapter` path
//! resolvable for the DI container (Strangler Fig).

#[path = "../../features/updates/adapter.rs"]
pub mod update_checker_adapter;

pub use update_checker_adapter::UpdateCheckerAdapter;
