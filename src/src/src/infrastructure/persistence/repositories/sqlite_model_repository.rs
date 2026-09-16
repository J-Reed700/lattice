//! Compatibility path for the current model repository.
//! SQL and schema mapping have one implementation in the model-management slice.
pub use crate::features::model_management::repository_tx::SqliteModelRepository;
