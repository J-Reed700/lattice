//! Shared HTTP clients, request profiles, and URL normalization.

pub mod auth_header;
pub mod client;
pub mod url_identity;

pub use auth_header::auth_header_map;
pub use client::{reqwest_client_builder, should_disable_system_proxy};
pub use url_identity::normalize_web_source_url;
