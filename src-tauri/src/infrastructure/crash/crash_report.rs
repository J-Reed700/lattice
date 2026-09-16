use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::panic::PanicHookInfo as StdPanicHookInfo;

/// Structured crash report with all debugging information.
///
/// Design Philosophy:
/// - Immutable after creation (value object)
/// - Serializable to JSON for storage
/// - Complete: captures ALL relevant crash context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashReport {
    /// Crash report format version (for future schema evolution)
    pub version: String,

    /// When the crash occurred (UTC)
    pub timestamp: DateTime<Utc>,

    /// Application metadata
    pub app_info: AppInfo,

    /// System/hardware metadata
    pub system_info: SystemInfo,

    /// Panic information
    pub panic_info: PanicInfo,

    /// Thread that panicked
    pub thread_info: ThreadInfo,

    /// Full stack trace (if available)
    pub backtrace: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub build_profile: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub os: String,
    pub os_version: String,
    pub arch: String,
    pub num_cpus: usize,
    pub total_memory_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanicInfo {
    pub message: String,
    pub location: Option<String>,
    pub payload_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadInfo {
    pub id: String,
    pub name: Option<String>,
}

impl CrashReport {
    /// Create a new crash report from panic information.
    ///
    /// SAFETY: This function MUST NOT PANIC.
    /// All operations are defensive with fallback values.
    pub fn from_panic(panic_info: &StdPanicHookInfo) -> Self {
        Self {
            version: "1.0".to_string(),
            timestamp: Utc::now(),
            app_info: AppInfo::capture(),
            system_info: SystemInfo::capture(),
            panic_info: PanicInfo::from_panic_info(panic_info),
            thread_info: ThreadInfo::current(),
            backtrace: Self::capture_backtrace(),
        }
    }

    /// Serialize to JSON (fallback to debug format if JSON fails).
    ///
    /// SAFETY: Never panics. Returns a valid string representation.
    pub fn to_json_string(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| format!("{:#?}", self))
    }

    /// Capture stack trace (defensive, returns None if unavailable).
    fn capture_backtrace() -> Option<String> {
        let bt = std::backtrace::Backtrace::force_capture();
        if bt.status() == std::backtrace::BacktraceStatus::Captured {
            Some(format!("{}", bt))
        } else {
            None
        }
    }
}
