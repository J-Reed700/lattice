//! Mock implementations for testing
//!
//! This module provides mock implementations of all service traits.
//! Mocks are useful for testing without external dependencies.

mod mock_chunk;
mod mock_context;
// Vertical-slice migration (conversation): mock lives in features/conversation/mocks.rs.
#[path = "../../../features/conversation/mocks.rs"]
mod mock_conversation;
mod mock_document;
// Vertical-slice migration (embedding): mock lives in features/embedding/mocks.rs.
#[path = "../../../features/embedding/mocks.rs"]
mod mock_embedding;
mod mock_file_storage;
// Vertical-slice migration (function_calling): mock lives in features/function_calling/mocks.rs.
#[path = "../../../features/function_calling/mocks.rs"]
mod mock_function;
// Vertical-slice migration (indexing): mock lives in features/indexing/mocks.rs.
#[path = "../../../features/indexing/mocks.rs"]
mod mock_indexing;
// Vertical-slice migration (mentions): mock lives in features/mentions/mocks.rs.
#[path = "../../../features/mentions/mocks.rs"]
mod mock_mention;
mod mock_model;
// Vertical-slice migration (qa): mock lives in features/qa/mocks.rs.
#[path = "../../../features/qa/mocks.rs"]
mod mock_qa;
// Vertical-slice migration (search): mock lives in features/search/mocks.rs.
#[path = "../../../features/search/mocks.rs"]
mod mock_search;
// Vertical-slice migration (web): mocks live in features/web/mocks/.
#[path = "../../../features/web/mocks/web.rs"]
mod mock_web;
#[path = "../../../features/web/mocks/archive.rs"]
mod mock_web_archive;
#[path = "../../../features/web/mocks/capture.rs"]
mod mock_web_capture;

// Re-export all mocks
#[allow(unused_imports)]
pub use mock_chunk::*;
#[allow(unused_imports)]
pub use mock_context::*;
#[allow(unused_imports)]
pub use mock_conversation::*;
#[allow(unused_imports)]
pub use mock_document::*;
#[allow(unused_imports)]
pub use mock_embedding::*;
#[allow(unused_imports)]
pub use mock_file_storage::*;
#[allow(unused_imports)]
pub use mock_function::*;
#[allow(unused_imports)]
pub use mock_indexing::*;
#[allow(unused_imports)]
pub use mock_mention::*;
#[allow(unused_imports)]
pub use mock_model::*;
#[allow(unused_imports)]
pub use mock_qa::*;
#[allow(unused_imports)]
pub use mock_search::*;
#[allow(unused_imports)]
pub use mock_web::*;
#[allow(unused_imports)]
pub use mock_web_archive::*;
#[allow(unused_imports)]
pub use mock_web_capture::*;
