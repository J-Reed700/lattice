//! Mock implementations for testing
//!
//! This module provides mock implementations of all service traits.
//! Mocks are useful for testing without external dependencies.

mod mock_chunk;
mod mock_context;
mod mock_conversation;
mod mock_document;
mod mock_embedding;
mod mock_file_storage;
mod mock_function;
mod mock_indexing;
mod mock_mention;
mod mock_model;
mod mock_qa;
mod mock_search;
mod mock_tag;
mod mock_web;
mod mock_web_archive;
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
pub use mock_tag::*;
#[allow(unused_imports)]
pub use mock_web::*;
#[allow(unused_imports)]
pub use mock_web_archive::*;
#[allow(unused_imports)]
pub use mock_web_capture::*;
