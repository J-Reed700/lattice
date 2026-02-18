//! Provider modules for dependency injection.
//!
//! This module contains focused providers that decompose the monolithic
//! ServiceContainer into SOLID-compliant components.

mod database_provider;
mod security_provider;
mod search_provider;
mod file_provider;
mod config_provider;

pub use database_provider::DatabaseProvider;
pub use security_provider::{SecurityProvider, SecurityProviderBuilder, RateLimiters};
pub use search_provider::{SearchProvider, SearchProviderBuilder};
pub use file_provider::{FileProvider, FileProviderBuilder};
pub use config_provider::{
    ConfigProvider, ConfigProviderBuilder, KeyringServiceTrait,
    ProductionKeyringService, MockKeyringService,
};
