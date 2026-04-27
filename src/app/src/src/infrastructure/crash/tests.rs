use super::*;
use crash_report::{AppInfo, CrashReport, PanicInfo, SystemInfo, ThreadInfo};
use crash_rotation::CrashRotation;
use crash_writer::CrashWriter;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_crash_report_metadata() {
    let app_info = AppInfo::capture();
    assert!(!app_info.name.is_empty());
    assert!(!app_info.version.is_empty());
    assert!(app_info.build_profile == "debug" || app_info.build_profile == "release");

    let system_info = SystemInfo::capture();
    assert!(!system_info.os.is_empty());
    assert!(!system_info.arch.is_empty());

    let thread_info = ThreadInfo::current();
    assert!(!thread_info.id.is_empty());
}

#[test]
fn test_crash_report_serialization() {
    use chrono::Utc;

    let report = CrashReport {
        version: "1.0".to_string(),
        timestamp: Utc::now(),
        app_info: AppInfo::capture(),
        system_info: SystemInfo::capture(),
        panic_info: PanicInfo {
            message: "test panic".to_string(),
            location: Some("test.rs:42:10".to_string()),
            payload_type: "&str".to_string(),
        },
        thread_info: ThreadInfo::current(),
        backtrace: Some("test backtrace".to_string()),
    };

    let json = report.to_json_string();

    assert!(!json.is_empty());
    assert!(serde_json::from_str::<CrashReport>(&json).is_ok());
}

#[test]
fn test_crash_writer_atomic_write() {
    use chrono::Utc;

    let temp_dir = TempDir::new().unwrap();
    let writer = CrashWriter::new(temp_dir.path().to_path_buf());

    let report = CrashReport {
        version: "1.0".to_string(),
        timestamp: Utc::now(),
        app_info: AppInfo::capture(),
        system_info: SystemInfo::capture(),
        panic_info: PanicInfo {
            message: "test panic".to_string(),
            location: Some("test.rs:42:10".to_string()),
            payload_type: "&str".to_string(),
        },
        thread_info: ThreadInfo::current(),
        backtrace: Some("test backtrace".to_string()),
    };

    let path = writer.write(&report).expect("Write should succeed");

    assert!(path.exists());
    assert_eq!(path.extension().unwrap(), "json");

    let temp_files: Vec<_> = fs::read_dir(temp_dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|ext| ext.to_str()) == Some("tmp"))
        .collect();
    assert_eq!(temp_files.len(), 0, "No .tmp files should remain");
}

#[test]
fn test_crash_rotation() {
    let temp_dir = TempDir::new().unwrap();

    for i in 0..15 {
        let filename = format!("crash-2026-01-11T10-30-{:02}Z-test.json", i);
        let path = temp_dir.path().join(&filename);
        fs::write(&path, "{}").unwrap();
    }

    CrashRotation::cleanup_old_crashes(temp_dir.path());

    let remaining: Vec<_> = fs::read_dir(temp_dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let path = e.path();
            path.extension().and_then(|ext| ext.to_str()) == Some("json")
                && path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("crash-"))
                    .unwrap_or(false)
        })
        .collect();

    assert_eq!(remaining.len(), 10, "Should keep only 10 crash files");
}

#[test]
fn test_crash_rotation_with_few_files() {
    let temp_dir = TempDir::new().unwrap();

    for i in 0..5 {
        let filename = format!("crash-2026-01-11T10-30-{:02}Z-test.json", i);
        let path = temp_dir.path().join(&filename);
        fs::write(&path, "{}").unwrap();
    }

    CrashRotation::cleanup_old_crashes(temp_dir.path());

    let remaining: Vec<_> = fs::read_dir(temp_dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let path = e.path();
            path.extension().and_then(|ext| ext.to_str()) == Some("json")
        })
        .collect();

    assert_eq!(remaining.len(), 5, "Should not delete if <= 10 files");
}

#[test]
fn test_crash_writer_directory_creation() {
    let temp_dir = TempDir::new().unwrap();
    let crashes_dir = temp_dir.path().join("nonexistent").join("crashes");

    assert!(!crashes_dir.exists());

    let _writer = CrashWriter::new(crashes_dir.clone());

    assert!(crashes_dir.exists(), "Directory should be created");
}
