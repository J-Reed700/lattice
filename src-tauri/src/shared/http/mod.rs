//! Shared HTTP clients, request profiles, and URL normalization.

pub mod client;
pub mod url_identity;

pub use client::{reqwest_client_builder, should_disable_system_proxy};
