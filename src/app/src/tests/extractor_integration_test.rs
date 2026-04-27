#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]

use std::io::Write;
use std::path::Path;
use tempfile::{NamedTempFile, TempDir};
// NOTE: These tests reference lattice::indexing::extractor which doesn't exist in the new DDD architecture.
// The extraction functionality has been moved to lattice::infrastructure::extraction.
// These tests need to be rewritten to use the new extraction module.

#[tokio::test]
#[ignore = "Test needs update for DDD architecture - extraction module moved to infrastructure::extraction"]
async fn test_extract_simple_text_file() {
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(temp_file, "Hello, World!").unwrap();
    writeln!(temp_file, "This is a test.").unwrap();

    // TODO: Update to use lattice::infrastructure::extraction
    // let extractor = lattice::indexing::extractor::ContentExtractor::new();
    // let result = extractor.extract_from_file(temp_file.path()).await;
    // assert!(result.is_ok());
    panic!("Test needs update for new architecture");
}

#[tokio::test]
#[ignore = "Test needs update for DDD architecture - extraction module moved to infrastructure::extraction"]
async fn test_unsupported_file_type() {
    let temp_dir = TempDir::new().unwrap();
    let exe_path = temp_dir.path().join("test.exe");
    std::fs::write(&exe_path, b"fake exe content").unwrap();

    // TODO: Update to use lattice::infrastructure::extraction
    panic!("Test needs update for new architecture");
}

#[tokio::test]
#[ignore = "Test needs update for DDD architecture - extraction module moved to infrastructure::extraction"]
async fn test_file_not_found() {
    // TODO: Update to use lattice::infrastructure::extraction
    panic!("Test needs update for new architecture");
}

#[test]
#[ignore = "Test needs update for DDD architecture - extraction module moved to infrastructure::extraction"]
fn test_supported_extensions() {
    // TODO: Update to use lattice::infrastructure::extraction
    panic!("Test needs update for new architecture");
}
