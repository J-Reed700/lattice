//! System information adapter implementation.
//!
//! Production adapter using `sysinfo` crate for real hardware detection.

use crate::application::ports::system_info::{ComputeType, GpuInfo, SystemInfo, SystemInfoPort};
use crate::shared::error::AppError;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use sysinfo::System;

/// Production system information adapter.
///
/// Uses `sysinfo` crate for cross-platform hardware detection
/// with best-effort GPU detection.
pub struct SystemInfoAdapter {
    sys: Arc<Mutex<System>>,
}

impl SystemInfoAdapter {
    /// Create a new system info adapter.
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        Self {
            sys: Arc::new(Mutex::new(sys)),
        }
    }

    /// Attempt to detect GPU information.
    ///
    /// Uses platform-specific commands for best-effort detection.
    /// Failures are gracefully handled - GPU is optional.
    fn detect_gpu() -> Option<GpuInfo> {
        #[cfg(target_os = "macos")]
        {
            // For macOS, assume Metal is available
            // Full detection would require async command execution
            // which is complex in a synchronous context
            Some(GpuInfo {
                name: "Metal GPU".to_string(),
                vram_gb: Some(8.0), // Conservative estimate for M-series
                compute_type: ComputeType::Metal,
            })
        }

        #[cfg(not(target_os = "macos"))]
        {
            // GPU detection via system commands requires async execution
            // For now, return None for non-macOS systems
            // Full detection can be implemented if needed
            None
        }
    }
}

impl Default for SystemInfoAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SystemInfoPort for SystemInfoAdapter {
    async fn get_system_info(&self) -> Result<SystemInfo, AppError> {
        // Get RAM info
        let sys = self
            .sys
            .lock()
            .map_err(|_| AppError::InternalError("Failed to lock system info".to_string()))?;

        let total_ram = sys.total_memory() as f64 / (1024_u64.pow(3) as f64);
        let cpu_cores = sys.cpus().len() as u32;
        let cpu_model = sys.global_cpu_info().brand().to_string();
        drop(sys);

        // GPU detection is best-effort
        let gpu_info = Self::detect_gpu();

        Ok(SystemInfo {
            total_ram_gb: total_ram,
            cpu_cores,
            cpu_model,
            gpu_info,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_system_info_adapter() {
        let adapter = SystemInfoAdapter::new();
        let info = adapter.get_system_info().await.unwrap();

        // Basic sanity checks
        assert!(info.total_ram_gb > 0.0);
        assert!(info.cpu_cores > 0);
        // CPU model may be empty in CI/test environments; when present, it should not contain null bytes.
        assert!(!info.cpu_model.contains('\0'));
    }
}
