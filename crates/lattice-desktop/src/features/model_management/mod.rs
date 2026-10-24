//! Model lifecycle: downloaded models tracking, active model selection, and catalog adapters.

pub mod cache_adapter;
pub mod commands;
pub mod commands_extra;
pub mod di;
pub mod domain;
pub mod huggingface_adapter;
pub mod model_manager;
pub mod plugin;
pub mod use_cases;
