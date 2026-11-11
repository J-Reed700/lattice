//! Shared HTTP clients, request profiles, and URL normalization.

pub mod client;
pub mod url_identity;

pub use client::{loopback_client_builder, reqwest_client_builder};
pub use url_identity::{host_of, normalize_web_source_url};
