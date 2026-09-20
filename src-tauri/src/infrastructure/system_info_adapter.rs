//! # System Information Adapter
//!
//! Infrastructure implementation of SystemInfoPort using `sysinfo` crate
//! and platform-specific APIs for GPU detection.
//!
//! ## Platform Support
//!
//! - **macOS**: Uses `sysinfo` + `system_profiler` for GPU detection
//! - **Linux**: Uses `sysinfo` + `/proc/driver/nvidia/gpus` for NVIDIA
//! - **Windows**: Uses `sysinfo` + WMI for GPU detection
//!
//! ## Hardware Detection
//!
//! 1. **RAM**: Total and available memory from `sysinfo`
//! 2. **CPU**: Core count and model from `sysinfo`
//! 3. **GPU**: Platform-specific detection:
//!    - macOS: Metal via `system_profiler SPDisplaysDataType`
//!    - Linux: NVIDIA via `/proc/driver/nvidia/gpus`, AMD/Intel via `lspci`
//!    - Windows: DirectX/CUDA via WMI
//! 4. **Disk**: Available space from `sysinfo`
//!
//! ## Example
//!
//! ```rust,no_run
//! use lattice::infrastructure::system_info_adapter::SystemInfoAdapter;
//! use lattice::application::ports::SystemInfoPort;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let adapter = SystemInfoAdapter::new();
//! let info = adapter.get_system_info().await?;
//!
//! println!("RAM: {} GB", info.total_ram_gb);
//! println!("GPU: {:?}", info.gpu_info);
//! # Ok(())
//! # }
//! ```

use crate::application::ports::{ComputeType, GpuInfo, SystemInfo, SystemInfoPort};
use crate::shared::error::AppError;
use async_trait::async_trait;
use std::process::Command;
use sysinfo::System;

/// Infrastructure adapter for system information detection.
///
/// Uses `sysinfo` crate for cross-platform hardware detection,
/// with platform-specific APIs for GPU information.
pub struct SystemInfoAdapter {
    system: parking_lot::Mutex<System>,
}

impl SystemInfoAdapter {
    /// Create a new system information adapter.
    pub fn new() -> Self {
        Self {
            system: parking_lot::Mutex::new(System::new_all()),
        }
    }

    /// Detect GPU information.
    ///
    /// The sidecar's own device list answers this for every platform at once,
    /// so it is consulted first; see
    /// [`backend_devices`](crate::features::llm::engine::system::backend_devices)
    /// for why asking beats guessing. The platform-specific probes below are the
    /// fallback for when that has not run, and they are weaker than they look —
    /// the Windows one cannot see an AMD or Intel card at all.
    fn detect_gpu_info(&self) -> Option<GpuInfo> {
        // The Intel macOS build ships CPU inference only. A Metal-capable
        // display adapter must not make onboarding recommend GPU-sized models.
        if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
            return None;
        }
        if let Some(gpu) = Self::gpu_from_sidecar_probe() {
            return Some(gpu);
        }
        self.detect_gpu_platform()
    }

    /// The cached `--list-devices` answer, if anything has probed yet.
    fn gpu_from_sidecar_probe() -> Option<GpuInfo> {
        use crate::features::llm::engine::system::{cached_backend_devices, GPUVendor};

        let device = cached_backend_devices()?
            .iter()
            .filter(|device| device.is_accelerator())
            .max_by_key(|device| device.total_mib)?;

        Some(GpuInfo {
            name: device.name.clone(),
            vram_gb: (device.total_mib > 0).then(|| device.total_gb()),
            compute_type: match device.vendor() {
                GPUVendor::Apple => ComputeType::Metal,
                GPUVendor::Nvidia => ComputeType::Cuda,
                GPUVendor::AMD => ComputeType::Rocm,
                // The port models only these three backends, and Intel/Arc runs
                // through SYCL or Vulkan; `None` here would read as "no GPU",
                // which is worse than naming the nearest neighbour.
                GPUVendor::Intel | GPUVendor::Unknown => ComputeType::None,
            },
        })
    }

    /// Platform-specific fallback, used only when the probe has not run.
    fn detect_gpu_platform(&self) -> Option<GpuInfo> {
        #[cfg(target_os = "macos")]
        {
            self.detect_gpu_macos()
        }

        #[cfg(target_os = "linux")]
        {
            self.detect_gpu_linux()
        }

        #[cfg(target_os = "windows")]
        {
            self.detect_gpu_windows()
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            None
        }
    }

    /// Detect GPU on macOS using system_profiler.
    #[cfg(target_os = "macos")]
    fn detect_gpu_macos(&self) -> Option<GpuInfo> {
        let output = Command::new("system_profiler")
            .arg("SPDisplaysDataType")
            .output()
            .ok()?;

        let output_str = String::from_utf8_lossy(&output.stdout);

        if output_str.contains("Apple M") || output_str.contains("Metal") {
            // Apple Silicon
            let name = output_str
                .lines()
                .find(|line| line.contains("Chipset Model:"))
                .and_then(|line| line.split(':').nth(1))
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| "Apple Silicon GPU".to_string());

            Some(GpuInfo {
                name,
                vram_gb: None, // Unified memory on Apple Silicon
                compute_type: ComputeType::Metal,
            })
        } else if output_str.contains("NVIDIA") {
            // NVIDIA eGPU
            let name = output_str
                .lines()
                .find(|line| line.contains("Chipset Model:"))
                .and_then(|line| line.split(':').nth(1))
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| "NVIDIA GPU".to_string());

            // Try to extract VRAM
            let vram_gb = output_str
                .lines()
                .find(|line| line.contains("VRAM"))
                .and_then(|line| line.split(':').nth(1))
                .and_then(|s| {
                    let parts: Vec<&str> = s.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let value: f64 = parts.first()?.parse().ok()?;
                        let unit = parts.get(1)?;
                        Some(if unit.starts_with("GB") {
                            value
                        } else {
                            value / 1024.0
                        })
                    } else {
                        None
                    }
                });

            Some(GpuInfo {
                name,
                vram_gb,
                compute_type: ComputeType::Cuda,
            })
        } else {
            None
        }
    }

    /// Detect GPU on Linux.
    #[cfg(target_os = "linux")]
    fn detect_gpu_linux(&self) -> Option<GpuInfo> {
        // Try NVIDIA first
        if let Some(gpu) = self.detect_nvidia_gpu_linux() {
            return Some(gpu);
        }

        // Try AMD via lspci
        if let Some(gpu) = self.detect_amd_gpu_linux() {
            return Some(gpu);
        }

        None
    }

    /// Detect NVIDIA GPU on Linux.
    #[cfg(target_os = "linux")]
    fn detect_nvidia_gpu_linux(&self) -> Option<GpuInfo> {
        if !std::path::Path::new("/proc/driver/nvidia/version").exists() {
            return None;
        }

        // Try nvidia-smi
        let output = Command::new("nvidia-smi")
            .arg("--query-gpu=name,memory.total")
            .arg("--format=csv,noheader")
            .output()
            .ok()?;

        let output_str = String::from_utf8_lossy(&output.stdout);
        let mut parts = output_str.trim().splitn(2, ',');
        let name = parts.next()?.trim().to_string();
        let vram_str = parts.next()?.trim();
        let vram_gb = vram_str
            .split_whitespace()
            .next()
            .and_then(|s| s.parse::<f64>().ok())
            .map(|mb| mb / 1024.0);

        Some(GpuInfo {
            name,
            vram_gb,
            compute_type: ComputeType::Cuda,
        })
    }

    /// Detect AMD GPU on Linux via lspci.
    #[cfg(target_os = "linux")]
    fn detect_amd_gpu_linux(&self) -> Option<GpuInfo> {
        let output = Command::new("lspci").output().ok()?;

        let output_str = String::from_utf8_lossy(&output.stdout);

        // Look for AMD/ATI VGA
        let gpu_line = output_str
            .lines()
            .find(|line| line.contains("VGA") && (line.contains("AMD") || line.contains("ATI")))?;

        let name = gpu_line
            .split(':')
            .nth(2)
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "AMD GPU".to_string());

        Some(GpuInfo {
            name,
            vram_gb: None, // Cannot detect VRAM via lspci
            compute_type: ComputeType::Rocm,
        })
    }

    /// Detect GPU on Windows via WMI.
    #[cfg(target_os = "windows")]
    fn detect_gpu_windows(&self) -> Option<GpuInfo> {
        // Try nvidia-smi first
        if let Some(gpu) = self.detect_nvidia_gpu_windows() {
            return Some(gpu);
        }

        // Fallback: Windows WMI query for GPU
        // This would require additional dependencies, so we'll use a simpler approach
        // by checking for common GPU drivers

        None
    }

    /// Detect NVIDIA GPU on Windows via nvidia-smi.
    #[cfg(target_os = "windows")]
    fn detect_nvidia_gpu_windows(&self) -> Option<GpuInfo> {
        let output = Command::new("nvidia-smi")
            .arg("--query-gpu=name,memory.total")
            .arg("--format=csv,noheader")
            .output()
            .ok()?;

        let output_str = String::from_utf8_lossy(&output.stdout);
        // "NVIDIA GeForce RTX 4090, 24564 MiB" -> ("NVIDIA GeForce RTX 4090", "24564 MiB").
        // split_once gives us both halves without indexing, which `clippy::indexing_slicing`
        // (denied crate-wide) rejects, and it returns None when nvidia-smi prints nothing.
        let (name, vram_str) = output_str.trim().split_once(',')?;
        let vram_gb = vram_str
            .trim()
            .split_whitespace()
            .next()
            .and_then(|s| s.parse::<f64>().ok())
            .map(|mb| mb / 1024.0);

        Some(GpuInfo {
            name: name.trim().to_string(),
            vram_gb,
            compute_type: ComputeType::Cuda,
        })
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
        // Refresh system information
        {
            let mut sys = self.system.lock();
            sys.refresh_all();
        }

        let sys = self.system.lock();

        let total_ram_gb = sys.total_memory() as f64 / (1024.0 * 1024.0 * 1024.0);

        let cpu_cores = sys.cpus().len() as u32;
        let cpu_model = sys
            .cpus()
            .first()
            .map(|cpu| cpu.brand().to_string())
            .unwrap_or_else(|| "Unknown CPU".to_string());

        drop(sys); // Release lock before GPU detection

        let gpu_info = self.detect_gpu_info();

        Ok(SystemInfo {
            total_ram_gb,
            cpu_cores,
            cpu_model,
            gpu_info,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    #[test]
    fn intel_mac_does_not_advertise_gpu_inference() {
        assert!(SystemInfoAdapter::new().detect_gpu_info().is_none());
    }

    #[tokio::test]
    async fn test_adapter_returns_valid_info() {
        let adapter = SystemInfoAdapter::new();
        let info = adapter.get_system_info().await.unwrap();

        // Basic sanity checks
        assert!(info.total_ram_gb > 0.0);
        assert!(info.cpu_cores > 0);
        assert!(!info.cpu_model.is_empty());
    }

    #[tokio::test]
    async fn test_adapter_multiple_calls() {
        let adapter = SystemInfoAdapter::new();

        let info1 = adapter.get_system_info().await.unwrap();
        let info2 = adapter.get_system_info().await.unwrap();

        // Total RAM and CPU should be consistent
        assert_eq!(info1.total_ram_gb, info2.total_ram_gb);
        assert_eq!(info1.cpu_cores, info2.cpu_cores);
    }
}
