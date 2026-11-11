// Domain service modules moved under domains/ for filesystem organization.
pub mod file_cleanup;
pub mod file_type_detector;
pub mod metadata_extraction;

// Directory-backed service modules
#[cfg(test)]
pub mod mocks;

// Service trait definitions for dependency injection
pub mod traits;
