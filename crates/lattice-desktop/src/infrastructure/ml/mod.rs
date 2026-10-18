//! Shared machine-learning infrastructure.
//!
//! The embedding engine lives in `features/embedding/`.

pub mod artifact_identity;
pub mod compute_device;
pub(crate) mod instance_pool;
pub(crate) mod model_cache;
pub mod tokenizer;
