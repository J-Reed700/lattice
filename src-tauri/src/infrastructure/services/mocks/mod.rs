//! Mock implementations for testing
//!
//! Mocks for cross-cutting / infrastructure service traits. Feature-specific
//! mocks live with their feature (e.g. `crate::features::tags::mocks::MockTagService`).

mod mock_context;
mod mock_file_storage;
mod mock_model;

#[allow(unused_imports)]
pub use mock_context::*;
#[allow(unused_imports)]
pub use mock_file_storage::*;
#[allow(unused_imports)]
pub use mock_model::*;
