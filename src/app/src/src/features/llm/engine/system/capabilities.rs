//! System capabilities detection and reporting.

use serde::{Deserialize, Serialize};
use sysinfo::System;

use super::gpu::{detect_gpu, GPUInfo};
use super::platform::Platform;

/// Complete system hardware capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemCapabilities {
    /// Total system RAM in GB
    pub total_ram_gb: f64,

    /// Currently available RAM in GB
    pub available_ram_gb: f64,

    /// GPU information (if available)
    pub gpu: Option<GPUInfo>,

    /// Number of physical CPU cores
    pub cpu_cores: usize,

    /// Number of logical CPU threads
    pub cpu_threads: usize,

    /// CPU model name
    pub cpu_model: Option<String>,

    /// CPU architecture (e.g., "x86_64", "arm64")
    pub cpu_architecture: Option<String>,

    /// Operating system platform
    pub platform: Platform,

    /// OS version string
    pub os_version: Option<String>,
}

impl SystemCapabilities {
    /// Check if GPU acceleration is available.
    pub fn has_gpu(&self) -> bool {
        self.gpu.is_some()
    }

    /// Get GPU VRAM in GB, if available.
    pub fn gpu_vram_gb(&self) -> Option<f64> {
        self.gpu.as_ref().and_then(|gpu| gpu.vram_gb)
    }

    /// Check if system has sufficient RAM for a given requirement (in GB).
    pub fn has_sufficient_ram(&self, required_gb: f64) -> bool {
        self.available_ram_gb >= required_gb
    }

    /// Get a human-readable summary of capabilities.
    pub fn summary(&self) -> String {
        let gpu_str = if let Some(ref gpu) = self.gpu {
            format!(
                "{} ({}{})",
                gpu.name,
                gpu.vendor.backend_name(),
                if let Some(vram) = gpu.vram_gb {
                    format!(", {:.1}GB VRAM", vram)
                } else {
                    String::new()
                }
            )
        } else {
            "No GPU detected (CPU only)".to_string()
        };

        format!(
            "Platform: {}\nRAM: {:.1}GB total, {:.1}GB available\nCPU: {} cores ({} threads){}\nGPU: {}",
            self.platform,
            self.total_ram_gb,
            self.available_ram_gb,
            self.cpu_cores,
            self.cpu_threads,
            self.cpu_model
                .as_ref()
                .map(|m| format!(" - {}", m))
                .unwrap_or_default(),
            gpu_str
        )
    }

    /// Convert RAM from MB to GB.
    fn mb_to_gb(mb: u64) -> f64 {
        mb as f64 / 1024.0
    }

    /// Convert RAM from bytes to GB.
    fn bytes_to_gb(bytes: u64) -> f64 {
        bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    }
}

/// Detect system capabilities.
///
/// This function gathers information about:
/// - RAM (total and available)
/// - CPU (cores, threads, model)
/// - GPU (vendor, model, VRAM)
/// - Platform (OS type and version)
///
/// # Returns
/// A `SystemCapabilities` struct with detected hardware information.
pub async fn detect_capabilities() -> SystemCapabilities {
    let mut sys = System::new();
    // Only refresh what we need instead of everything
    sys.refresh_memory();
    sys.refresh_cpu();

    // Detect RAM
    let total_ram_gb = SystemCapabilities::bytes_to_gb(sys.total_memory());

    // Use total RAM for availability check - let the OS handle memory management
    // The app shouldn't prevent downloads based on "available" memory since:
    // 1. OS will swap/manage memory automatically
    // 2. "available" memory reporting is unreliable on some platforms (macOS)
    // 3. User knows their system better than we do
    let available_ram_gb = total_ram_gb;

    // Detect CPU
    let cpu_cores = sys.physical_core_count().unwrap_or(1);
    let cpu_threads = sys.cpus().len();

    // Get CPU model name (from first CPU)
    let cpu_model = sys.cpus().first().map(|cpu| {
        let brand = cpu.brand();
        if brand.is_empty() {
            "Unknown CPU".to_string()
        } else {
            brand.to_string()
        }
    });

    // Detect CPU architecture
    let cpu_architecture = Some(std::env::consts::ARCH.to_string());

    // Detect platform
    let platform = Platform::detect();

    // Detect OS version
    let os_version = System::long_os_version();

    // Detect GPU
    let gpu = detect_gpu().await;

    SystemCapabilities {
        total_ram_gb,
        available_ram_gb,
        gpu,
        cpu_cores,
        cpu_threads,
        cpu_model,
        cpu_architecture,
        platform,
        os_version,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_detect_capabilities() {
        let caps = detect_capabilities().await;

        // Basic sanity checks
        assert!(caps.total_ram_gb > 0.0);
        assert!(caps.available_ram_gb > 0.0);
        assert!(caps.available_ram_gb <= caps.total_ram_gb);
        assert!(caps.cpu_cores > 0);
        assert!(caps.cpu_threads >= caps.cpu_cores);
    }

    #[tokio::test]
    async fn test_has_sufficient_ram() {
        let caps = detect_capabilities().await;

        // Should have at least 1GB
        assert!(caps.has_sufficient_ram(1.0));

        // Should not have 1000GB (on most systems)
        assert!(!caps.has_sufficient_ram(1000.0));
    }

    #[tokio::test]
    async fn test_summary() {
        let caps = detect_capabilities().await;
        let summary = caps.summary();

        // Summary should contain key info
        assert!(summary.contains("Platform:"));
        assert!(summary.contains("RAM:"));
        assert!(summary.contains("CPU:"));
        assert!(summary.contains("GPU:"));
    }

    #[test]
    fn test_conversions() {
        let gb_from_bytes = SystemCapabilities::bytes_to_gb(1024 * 1024 * 1024);
        assert_eq!(gb_from_bytes, 1.0);

        let gb_from_mb = SystemCapabilities::mb_to_gb(1024);
        assert_eq!(gb_from_mb, 1.0);
    }
}
