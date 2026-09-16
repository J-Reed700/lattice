//! Crash-recovery arithmetic for resuming a partial file.

use crate::features::download::manager::validation::recover_resume_offset;

#[test]
fn crash_recovery_uses_file_length_ahead_of_throttled_progress() {
    assert_eq!(recover_resume_offset(400, 512, Some(1_000)), Some(512));
    assert_eq!(recover_resume_offset(0, 512, Some(1_000)), Some(512));
}

#[test]
fn crash_recovery_rejects_inconsistent_partial_files() {
    assert_eq!(recover_resume_offset(600, 512, Some(1_000)), None);
    assert_eq!(recover_resume_offset(400, 1_001, Some(1_000)), None);
    assert_eq!(recover_resume_offset(0, 0, Some(1_000)), None);
}
