#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]

use vault::infrastructure::crash::crash_report::{
    AppInfo, CrashReport, PanicInfo, SystemInfo, ThreadInfo,
};

#[test]
fn test_crash_report_generation() {
    use chrono::Utc;

    let report = CrashReport {
        version: "1.0".to_string(),
        timestamp: Utc::now(),
        app_info: AppInfo::capture(),
        system_info: SystemInfo::capture(),
        panic_info: PanicInfo {
            message: "Integration test panic".to_string(),
            location: Some("test.rs:42:10".to_string()),
            payload_type: "&str".to_string(),
        },
        thread_info: ThreadInfo::current(),
        backtrace: Some("test backtrace".to_string()),
    };

    let json = report.to_json_string();

    assert!(json.contains("version"));
    assert!(json.contains("1.0"));
    assert!(json.contains("app_info"));
    assert!(json.contains("recall-desktop"));
    assert!(json.contains("system_info"));
    assert!(json.contains("panic_info"));
    assert!(json.contains("Integration test panic"));
    assert!(json.contains("thread_info"));
}

#[test]
fn test_app_info_capture() {
    let app_info = AppInfo::capture();

    assert_eq!(app_info.name, "recall-desktop");
    assert!(!app_info.version.is_empty());
    assert!(app_info.build_profile == "debug" || app_info.build_profile == "release");
}

#[test]
fn test_system_info_capture() {
    let system_info = SystemInfo::capture();

    assert!(!system_info.os.is_empty());
    assert!(!system_info.os_version.is_empty());
    assert!(!system_info.arch.is_empty());
    assert!(system_info.num_cpus > 0);
    assert!(system_info.total_memory_mb > 0);
}

#[test]
fn test_thread_info_capture() {
    let thread_info = ThreadInfo::current();

    assert!(!thread_info.id.is_empty());
}
