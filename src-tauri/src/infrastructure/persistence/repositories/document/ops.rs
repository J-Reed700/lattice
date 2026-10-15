//! Document Repository Operations
//!
//! Generic SQL operations for document persistence that work with any SQLite executor.
//! These functions can be used with both connection pools and transactions.

mod find;
mod lookup;
mod parts;
mod write;

pub use find::{
    find_aggregate_by_checksum_tx, find_all, find_all_paginated, find_by_checksum, find_by_filter,
    find_by_id, find_by_path, find_by_path_pattern, find_metadata_by_ids, list_metadata,
};
pub use lookup::{
    count, count_by_checksum, count_chunks, count_documents, exists, exists_by_path,
    find_checksum_by_id, find_file_path_by_id, find_id_by_path, list_checksums,
};
pub use parts::{fetch_chunks_for_document, fetch_tags_for_document, save_chunks, save_tags};
pub use write::{delete, delete_batch_optimized, rename, save, save_aggregate, save_batch};
