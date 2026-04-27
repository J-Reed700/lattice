//! Web infrastructure (legacy `infrastructure::web` entry point).
//!
//! Keeps the existing `crate::infrastructure::web::article_detector::*` etc.
//! paths resolvable after the web feature migrated to features/web/.
//! The submodule files physically live in `features/web/infra_modules/`.
//!
//! Note: Three of the four submodules are unused migration-placeholder
//! stubs. Only `article_detector` has a real implementation.

#[path = "infra_modules/article_detector.rs"]
pub mod article_detector;
#[path = "infra_modules/content_extractor.rs"]
pub mod content_extractor;
#[path = "infra_modules/metadata.rs"]
pub mod metadata;
#[path = "infra_modules/web_fetcher.rs"]
pub mod web_fetcher;

pub use article_detector::WebArticleDetector;
