use super::crash_report::{AppInfo, PanicInfo, SystemInfo, ThreadInfo};
use std::panic::PanicInfo as StdPanicInfo;

impl AppInfo {
    /// Capture application metadata.
    ///
    /// SAFETY: Never panics. Uses compile-time environment variables.
    pub fn capture() -> Self {
        Self {
            name: env!("CARGO_PKG_NAME").to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            build_profile: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            }
            .to_string(),
        }
    }
}

impl SystemInfo {
    /// Capture system metadata.
    ///
    /// SAFETY: Never panics. Falls back to default values on errors.
    pub fn capture() -> Self {
        Self {
            os: std::env::consts::OS.to_string(),
            os_version: Self::get_os_version(),
            arch: std::env::consts::ARCH.to_string(),
            num_cpus: std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(0),
            total_memory_mb: Self::get_total_memory(),
        }
    }

    /// Get OS version string.
    ///
    /// SAFETY: Never panics. Returns "Unknown" on errors.
    fn get_os_version() -> String {
        use sysinfo::System;

        let sys = System::new_all();
        format!(
            "{} {}",
            System::name().unwrap_or_else(|| "Unknown".to_string()),
            System::os_version().unwrap_or_else(|| "Unknown".to_string())
        )
    }

    /// Get total system memory in MB.
    ///
    /// SAFETY: Never panics. Returns 0 on errors.
    fn get_total_memory() -> u64 {
        use sysinfo::System;

        let sys = System::new_all();
        sys.total_memory() / 1024 / 1024
    }
}

impl PanicInfo {
    /// Extract panic information from std::panic::PanicInfo.
    ///
    /// SAFETY: Never panics. Uses defensive pattern matching.
    pub fn from_panic_info(info: &StdPanicInfo) -> Self {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(|s| s.as_str()))
            .unwrap_or("Unknown panic message");

        let location = info
            .location()
            .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()));

        let payload_type = if info.payload().is::<&str>() {
            "&str"
        } else if info.payload().is::<String>() {
            "String"
        } else {
            "Unknown"
        }
        .to_string();

        Self {
            message: message.to_string(),
            location,
            payload_type,
        }
    }
}

impl ThreadInfo {
    /// Capture current thread information.
    ///
    /// SAFETY: Never panics.
    pub fn current() -> Self {
        let thread = std::thread::current();
        Self {
            id: format!("{:?}", thread.id()),
            name: thread.name().map(|s| s.to_string()),
        }
    }
}
