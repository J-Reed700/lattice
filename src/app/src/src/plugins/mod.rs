//! Tauri plugin registration.
//!
//! Each feature owns its plugin in `crate::features::<name>::plugin`.
//! This module just collects them into a single `init_plugins()`
//! function that app bootstrap calls.

use tauri::plugin::TauriPlugin;

/// Initialize all feature plugins.
pub fn init_plugins() -> Vec<TauriPlugin<tauri::Wry>> {
    vec![
        // Core infrastructure
        crate::features::model_management::plugin::init(),
        crate::features::search::plugin::init(),
        crate::features::file::plugin::init(),
        crate::features::credentials::plugin::init(),
        crate::features::health::plugin::init(),
        crate::features::settings::plugin::init(),
        // Metadata & caching
        crate::features::tags::plugin::init(),
        crate::features::favorites::plugin::init(),
        crate::features::cache::plugin::init(),
        crate::features::mentions::plugin::init(),
        crate::features::function_calling::plugin::init(),
        crate::features::daily_notes::plugin::init(),
        // AI services
        crate::features::embedding::plugin::init(),
        crate::features::huggingface::plugin::init(),
        crate::features::extraction::plugin::init(),
        crate::features::web::plugin::init(),
        // Final domains
        crate::features::conversation::plugin::init(),
        crate::features::download::plugin::init(),
        crate::features::batch::plugin::init(),
        crate::features::backup::plugin::init(),
        crate::features::updates::plugin::init(),
        crate::features::qa::plugin::init(),
    ]
}
