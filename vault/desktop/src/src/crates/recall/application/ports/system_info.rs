//! # System Information Port
//!
//! Port for accessing system hardware capabilities.
//!
//! ## Purpose
//!
//! Provides an abstraction for detecting system hardware (RAM, CPU, GPU)
//! to enable model recommendation based on available resources.
//!
//! ## Implementations
//!
//! - **Production**: Uses `sysinfo` crate for real hardware detection
//! - **Mock**: Configurable mock for testing different hardware profiles
//!
//! ## Example
//!
//! ```rust,no_run
//! use crate::application::ports::SystemInfoPort;
//!
//! async fn check_capabilities(port: &dyn SystemInfoPort) -> Result<()> {
//!     let info = port.get_system_info().await?;
//!     println!("Total RAM: {} GB", info.total_ram_gb);
//!     println!("CPU cores: {}", info.cpu_cores);
//!     Ok(())
//! }
//! ```

use crate::shared::error::AppError;
use async_trait::async_trait;
use parking_lot::Mutex;
use std::sync::Arc;

/// System hardware information.
#[derive(Debug, Clone)]
pub struct SystemInfo {
    /// Total system RAM in gigabytes
    pub total_ram_gb: f64,
    /// Number of CPU cores
    pub cpu_cores: u32,
    /// CPU model name
    pub cpu_model: String,
    /// GPU information if available
    pub gpu_info: Option<GpuInfo>,
}

/// GPU information.
#[derive(Debug, Clone)]
pub struct GpuInfo {
    /// GPU name/model
    pub name: String,
    /// VRAM in gigabytes (if detectable)
    pub vram_gb: Option<f64>,
    /// Compute type
    pub compute_type: ComputeType,
}

/// GPU compute type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComputeType {
    /// Apple Metal (macOS)
    Metal,
    /// NVIDIA CUDA
    Cuda,
    /// AMD ROCm
    Rocm,
    /// No GPU acceleration
    None,
}

impl ComputeType {
    /// Convert to string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            ComputeType::Metal => "Metal",
            ComputeType::Cuda => "CUDA",
            ComputeType::Rocm => "ROCm",
            ComputeType::None => "None",
        }
    }
}

/// Port for accessing system hardware information.
///
/// Provides system capabilities for model recommendation.
#[async_trait]
pub trait SystemInfoPort: Send + Sync {
    /// Get current system hardware information.
    ///
    /// # Returns
    /// - `Ok(SystemInfo)` - System hardware details
    /// - `Err(AppError)` - Failed to read system information
    async fn get_system_info(&self) -> Result<SystemInfo, AppError>;
}

/// Mock implementation for testing.
///
/// Allows configuring different hardware profiles for testing
/// model recommendations under various system constraints.
pub struct MockSystemInfoPort {
    system_info: Arc<Mutex<SystemInfo>>,
}

impl MockSystemInfoPort {
    /// Create a new mock with default medium-tier hardware.
    pub fn new() -> Self {
        Self {
            system_info: Arc::new(Mutex::new(SystemInfo {
                total_ram_gb: 16.0,
                cpu_cores: 8,
                cpu_model: "Mock CPU".to_string(),
                gpu_info: None,
            })),
        }
    }

    /// Create mock for low-end hardware (< 8GB RAM).
    pub fn low_end() -> Self {
        Self {
            system_info: Arc::new(Mutex::new(SystemInfo {
                total_ram_gb: 4.0,
                cpu_cores: 4,
                cpu_model: "Mock CPU - Low End".to_string(),
                gpu_info: None,
            })),
        }
    }

    /// Create mock for medium-tier hardware (8-16GB RAM).
    pub fn medium_tier() -> Self {
        Self {
            system_info: Arc::new(Mutex::new(SystemInfo {
                total_ram_gb: 12.0,
                cpu_cores: 6,
                cpu_model: "Mock CPU - Medium".to_string(),
                gpu_info: None,
            })),
        }
    }

    /// Create mock for high-end hardware (> 16GB RAM).
    pub fn high_end() -> Self {
        Self {
            system_info: Arc::new(Mutex::new(SystemInfo {
                total_ram_gb: 32.0,
                cpu_cores: 16,
                cpu_model: "Mock CPU - High End".to_string(),
                gpu_info: Some(GpuInfo {
                    name: "Mock GPU".to_string(),
                    vram_gb: Some(8.0),
                    compute_type: ComputeType::Metal,
                }),
            })),
        }
    }

    /// Set RAM configuration.
    pub fn set_ram(&self, total: f64) {
        let mut info = self.system_info.lock();
        info.total_ram_gb = total;
    }

    /// Set CPU configuration.
    pub fn set_cpu(&self, cores: u32, model: String) {
        let mut info = self.system_info.lock();
        info.cpu_cores = cores;
        info.cpu_model = model;
    }

    /// Set GPU configuration.
    pub fn set_gpu(&self, gpu: Option<GpuInfo>) {
        self.system_info.lock().gpu_info = gpu;
    }
}

impl Default for MockSystemInfoPort {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SystemInfoPort for MockSystemInfoPort {
    async fn get_system_info(&self) -> Result<SystemInfo, AppError> {
        Ok(self.system_info.lock().clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_default_config() {
        let mock = MockSystemInfoPort::new();
        let info = mock.get_system_info().await.unwrap();

        assert_eq!(info.total_ram_gb, 16.0);
        assert_eq!(info.cpu_cores, 8);
        assert_eq!(info.cpu_model, "Mock CPU");
        assert!(info.gpu_info.is_none());
    }

    #[tokio::test]
    async fn test_mock_low_end() {
        let mock = MockSystemInfoPort::low_end();
        let info = mock.get_system_info().await.unwrap();

        assert_eq!(info.total_ram_gb, 4.0);
        assert_eq!(info.cpu_cores, 4);
        assert!(info.gpu_info.is_none());
    }

    #[tokio::test]
    async fn test_mock_high_end() {
        let mock = MockSystemInfoPort::high_end();
        let info = mock.get_system_info().await.unwrap();

        assert_eq!(info.total_ram_gb, 32.0);
        assert_eq!(info.cpu_cores, 16);
        assert!(info.gpu_info.is_some());

        let gpu = info.gpu_info.unwrap();
        assert_eq!(gpu.name, "Mock GPU");
        assert_eq!(gpu.vram_gb, Some(8.0));
        assert_eq!(gpu.compute_type, ComputeType::Metal);
    }

    #[tokio::test]
    async fn test_mock_set_ram() {
        let mock = MockSystemInfoPort::new();
        mock.set_ram(64.0);

        let info = mock.get_system_info().await.unwrap();
        assert_eq!(info.total_ram_gb, 64.0);
    }

    #[tokio::test]
    async fn test_mock_set_gpu() {
        let mock = MockSystemInfoPort::new();
        mock.set_gpu(Some(GpuInfo {
            name: "NVIDIA RTX 4090".to_string(),
            vram_gb: Some(24.0),
            compute_type: ComputeType::Cuda,
        }));

        let info = mock.get_system_info().await.unwrap();
        assert!(info.gpu_info.is_some());

        let gpu = info.gpu_info.unwrap();
        assert_eq!(gpu.name, "NVIDIA RTX 4090");
        assert_eq!(gpu.vram_gb, Some(24.0));
        assert_eq!(gpu.compute_type, ComputeType::Cuda);
    }

    #[test]
    fn test_compute_type_as_str() {
        assert_eq!(ComputeType::Metal.as_str(), "Metal");
        assert_eq!(ComputeType::Cuda.as_str(), "CUDA");
        assert_eq!(ComputeType::Rocm.as_str(), "ROCm");
        assert_eq!(ComputeType::None.as_str(), "None");
    }
}
