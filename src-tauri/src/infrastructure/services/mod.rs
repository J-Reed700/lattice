// Domain service modules moved under domains/ for filesystem organization.
pub mod file_cleanup;
pub mod file_type_detector;
pub mod metadata_extraction;
pub mod startup_reconciliation;
// Validated path used by FileCleanupService (download manager dep). Distinct
// from `shared::types::ValidatedFilePath`; do not consolidate without
// migrating FileCleanupService's API expectations.
pub mod validated_path;

// Directory-backed service modules
#[cfg(test)]
pub mod mocks;

// Service trait definitions for dependency injection
pub mod traits;
