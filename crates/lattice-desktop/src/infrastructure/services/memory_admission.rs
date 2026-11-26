//! Memory-based admission control for file reads and document indexing.
//!
//! Instead of arbitrary fixed file-size ceilings, operations estimate their peak
//! concurrent RAM footprint from the input file's byte size and check whether
//! that footprint fits inside the machine's currently available memory headroom.

use lattice_core::domain::value_objects::FileFormat;
use std::path::Path;

/// Fraction of currently available OS memory an individual file operation may
/// budget against: at most half (`1 / 2`) of measured available memory, leaving
/// the other half for the OS, WebView, SQLite, and concurrent work.
pub const HEADROOM_DIVISOR: u64 = 2;

/// Peak concurrent RAM multiplier for reading raw bytes over binary IPC:
/// Rust holds the file buffer (`1x`) while Tauri binary IPC transfers it into
/// a WebView `ArrayBuffer` (`1x`).
pub const READ_BYTES_MEMORY_MULTIPLIER: u64 = 2;

/// Peak concurrent RAM multiplier for reading text previews over JSON IPC:
/// Rust holds the decoded UTF-8 text (`1x`) while JSON serialization and the
/// WebView UTF-16 string representation hold up to `~2x`.
pub const READ_CONTENT_MEMORY_MULTIPLIER: u64 = 3;

/// Peak concurrent RAM multiplier for indexing non-audio documents:
/// extraction buffers/parser state (`~1x`), extracted text (`1x`), `Document`
/// aggregate text (`1x`), and chunk/embedding batch preparation (`1x`).
pub const INDEXING_MEMORY_MULTIPLIER: u64 = 4;

/// Probe for currently available physical system memory in bytes.
pub trait MemoryProbe: Send + Sync {
    /// Returns the OS's currently available memory in bytes, or `None` if the
    /// platform did not report a positive reading.
    fn available_memory_bytes(&self) -> Option<u64>;
}

/// Live [`MemoryProbe`] backed by `sysinfo`.
#[derive(Debug, Default, Clone, Copy)]
pub struct SysinfoMemoryProbe;

impl MemoryProbe for SysinfoMemoryProbe {
    fn available_memory_bytes(&self) -> Option<u64> {
        let mut system = sysinfo::System::new();
        system.refresh_memory();
        Some(system.available_memory()).filter(|&bytes| bytes > 0)
    }
}

/// Deterministic [`MemoryProbe`] for tests.
#[cfg(test)]
#[derive(Debug, Clone, Copy)]
pub struct FixedMemoryProbe(pub Option<u64>);

#[cfg(test)]
impl MemoryProbe for FixedMemoryProbe {
    fn available_memory_bytes(&self) -> Option<u64> {
        self.0
    }
}

/// Result of evaluating an operation against measured available memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryAdmissionDecision {
    /// Whether the operation is admitted under current memory headroom.
    pub admitted: bool,
    /// Estimated peak RAM footprint in bytes for this operation.
    pub estimated_bytes: u64,
    /// Available memory headroom (`available_bytes / HEADROOM_DIVISOR`), if known.
    pub headroom_bytes: Option<u64>,
    /// Maximum input file size in bytes that fits within `headroom_bytes`, if known.
    pub max_input_bytes: Option<u64>,
    /// Measured available OS memory in bytes, if known.
    pub available_bytes: Option<u64>,
}

/// Pure admission decision comparing `input_bytes * memory_multiplier` against
/// half of `available_bytes`.
///
/// When `available_bytes` is `None` (platform did not report available memory),
/// admission succeeds rather than falsely blocking all operations.
pub fn check_memory_admission(
    input_bytes: u64,
    memory_multiplier: u64,
    available_bytes: Option<u64>,
) -> MemoryAdmissionDecision {
    let estimated_bytes = input_bytes.saturating_mul(memory_multiplier);
    let Some(available) = available_bytes else {
        return MemoryAdmissionDecision {
            admitted: true,
            estimated_bytes,
            headroom_bytes: None,
            max_input_bytes: None,
            available_bytes: None,
        };
    };

    let headroom = available / HEADROOM_DIVISOR;
    let max_input_bytes = headroom.checked_div(memory_multiplier).unwrap_or(u64::MAX);
    let admitted = estimated_bytes <= headroom;

    MemoryAdmissionDecision {
        admitted,
        estimated_bytes,
        headroom_bytes: Some(headroom),
        max_input_bytes: Some(max_input_bytes),
        available_bytes: Some(available),
    }
}

/// Admission check for reading raw file bytes over Tauri binary IPC.
pub fn check_read_bytes_admission(
    file_size_bytes: u64,
    available_bytes: Option<u64>,
) -> MemoryAdmissionDecision {
    check_memory_admission(
        file_size_bytes,
        READ_BYTES_MEMORY_MULTIPLIER,
        available_bytes,
    )
}

/// Admission check for reading text preview content over Tauri JSON IPC.
pub fn check_read_content_admission(
    file_size_bytes: u64,
    available_bytes: Option<u64>,
) -> MemoryAdmissionDecision {
    check_memory_admission(
        file_size_bytes,
        READ_CONTENT_MEMORY_MULTIPLIER,
        available_bytes,
    )
}

/// Admission check for indexing or reindexing a file at `path`.
///
/// Audio files stream decoded PCM in bounded duration windows rather than
/// scaling RAM with compressed file size, so their file-size-proportional
/// footprint is zero. All other supported document types are checked with
/// [`INDEXING_MEMORY_MULTIPLIER`].
pub fn check_indexing_admission(
    path: &Path,
    file_size_bytes: u64,
    available_bytes: Option<u64>,
) -> MemoryAdmissionDecision {
    let multiplier = if FileFormat::is_audio_path(path) {
        0
    } else {
        INDEXING_MEMORY_MULTIPLIER
    };
    check_memory_admission(file_size_bytes, multiplier, available_bytes)
}

/// Format a byte count into a readable string for error messages.
pub fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    const GIB: u64 = 1024 * MIB;

    if bytes >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{} MiB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{} KiB", bytes / KIB)
    } else {
        format!("{bytes} bytes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * MIB;

    #[test]
    fn admits_when_estimated_footprint_fits_in_half_of_available_memory() {
        // 1 GiB available -> 512 MiB headroom -> up to 128 MiB indexing file
        let decision = check_indexing_admission(Path::new("book.pdf"), 128 * MIB, Some(GIB));
        assert!(decision.admitted);
        assert_eq!(decision.estimated_bytes, 512 * MIB);
        assert_eq!(decision.headroom_bytes, Some(512 * MIB));
        assert_eq!(decision.max_input_bytes, Some(128 * MIB));
    }

    #[test]
    fn refuses_when_estimated_footprint_exceeds_available_headroom() {
        // 100 MiB available -> 50 MiB headroom -> 13 MiB * 4 = 52 MiB > 50 MiB
        let decision = check_indexing_admission(Path::new("large.pdf"), 13 * MIB, Some(100 * MIB));
        assert!(!decision.admitted);
        assert_eq!(decision.estimated_bytes, 52 * MIB);
        assert_eq!(decision.headroom_bytes, Some(50 * MIB));
        assert_eq!(decision.max_input_bytes, Some(12 * MIB + 512 * 1024));
    }

    #[test]
    fn scales_above_old_fixed_caps_when_memory_is_available() {
        // 4 GiB available -> 2 GiB headroom -> 250 MiB binary read & indexing both admitted
        let read_decision = check_read_bytes_admission(250 * MIB, Some(4 * GIB));
        assert!(read_decision.admitted);
        assert_eq!(read_decision.max_input_bytes, Some(GIB));

        let index_decision =
            check_indexing_admission(Path::new("manual.pdf"), 250 * MIB, Some(4 * GIB));
        assert!(index_decision.admitted);
        assert_eq!(index_decision.max_input_bytes, Some(512 * MIB));
    }

    #[test]
    fn admits_streaming_audio_files_regardless_of_compressed_size() {
        let decision =
            check_indexing_admission(Path::new("lecture.wav"), 300 * MIB, Some(64 * MIB));
        assert!(decision.admitted);
        assert_eq!(decision.estimated_bytes, 0);
    }

    #[test]
    fn admits_zero_byte_files_even_under_zero_available_memory() {
        let decision = check_memory_admission(0, INDEXING_MEMORY_MULTIPLIER, Some(0));
        assert!(decision.admitted);
        assert_eq!(decision.estimated_bytes, 0);
    }

    #[test]
    fn admits_when_available_memory_cannot_be_measured() {
        let decision = check_indexing_admission(Path::new("book.pdf"), 200 * MIB, None);
        assert!(decision.admitted);
        assert_eq!(decision.headroom_bytes, None);
        assert_eq!(decision.max_input_bytes, None);
    }

    #[test]
    fn saturates_multiplication_without_overflow() {
        let decision = check_memory_admission(u64::MAX, INDEXING_MEMORY_MULTIPLIER, Some(GIB));
        assert!(!decision.admitted);
        assert_eq!(decision.estimated_bytes, u64::MAX);
    }

    #[test]
    fn read_bytes_and_read_content_use_distinct_multipliers() {
        let bytes = check_read_bytes_admission(10 * MIB, Some(48 * MIB));
        let content = check_read_content_admission(10 * MIB, Some(48 * MIB));
        // 48 MiB available -> 24 MiB headroom: 10*2=20 MiB fits, 10*3=30 MiB does not
        assert!(bytes.admitted);
        assert!(!content.admitted);
    }
}
