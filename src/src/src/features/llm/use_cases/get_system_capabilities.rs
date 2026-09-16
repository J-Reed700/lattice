//! Get System Capabilities Use Case
//!
//! Detects hardware capabilities for model recommendation.
//!
//! # Purpose
//!
//! Retrieves system information (RAM, CPU, GPU) to help recommend
//! appropriate LLM models based on available hardware resources.
//!
//! # Dependencies
//! - `SystemInfoPort` - Hardware detection
//!
//! # Example
//! ```rust,no_run
//! let use_case = GetSystemCapabilitiesUseCase::new(system_info_port);
//! let caps = use_case.execute().await?;
//! println!("Available RAM: {} GB", caps.available_ram_gb);
//! ```

use crate::application::ports::system_info::SystemInfoPort;
use crate::features::llm::dto::{GpuInfoDto, SystemCapabilitiesDto};
use crate::shared::error::AppError;
use std::sync::Arc;

pub struct GetSystemCapabilitiesUseCase {
    system_info: Arc<dyn SystemInfoPort>,
}

impl GetSystemCapabilitiesUseCase {
    pub fn new(system_info: Arc<dyn SystemInfoPort>) -> Self {
        Self { system_info }
    }

    pub async fn execute(&self) -> Result<SystemCapabilitiesDto, AppError> {
        let info = self.system_info.get_system_info().await?;

        Ok(SystemCapabilitiesDto {
            total_ram_gb: info.total_ram_gb,
            cpu_cores: info.cpu_cores,
            cpu_model: info.cpu_model,
            gpu_info: info.gpu_info.map(|gpu| GpuInfoDto {
                name: gpu.name,
                vram_gb: gpu.vram_gb,
                compute_type: gpu.compute_type.as_str().to_string(),
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::system_info::{ComputeType, GpuInfo, MockSystemInfoPort};

    #[tokio::test]
    async fn test_get_system_capabilities_default() {
        let mock_info = Arc::new(MockSystemInfoPort::new());
        let use_case = GetSystemCapabilitiesUseCase::new(mock_info);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let caps = result.unwrap();
        assert_eq!(caps.total_ram_gb, 16.0);
        assert_eq!(caps.cpu_cores, 8);
        assert_eq!(caps.cpu_model, "Mock CPU");
        assert!(caps.gpu_info.is_none());
    }

    #[tokio::test]
    async fn test_get_system_capabilities_low_end() {
        let mock_info = Arc::new(MockSystemInfoPort::low_end());
        let use_case = GetSystemCapabilitiesUseCase::new(mock_info);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let caps = result.unwrap();
        assert_eq!(caps.total_ram_gb, 4.0);
        assert_eq!(caps.cpu_cores, 4);
        assert!(caps.gpu_info.is_none());
    }

    #[tokio::test]
    async fn test_get_system_capabilities_medium_tier() {
        let mock_info = Arc::new(MockSystemInfoPort::medium_tier());
        let use_case = GetSystemCapabilitiesUseCase::new(mock_info);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let caps = result.unwrap();
        assert_eq!(caps.total_ram_gb, 12.0);
        assert_eq!(caps.cpu_cores, 6);
    }

    #[tokio::test]
    async fn test_get_system_capabilities_high_end_with_gpu() {
        let mock_info = Arc::new(MockSystemInfoPort::high_end());
        let use_case = GetSystemCapabilitiesUseCase::new(mock_info);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let caps = result.unwrap();
        assert_eq!(caps.total_ram_gb, 32.0);
        assert_eq!(caps.cpu_cores, 16);

        assert!(caps.gpu_info.is_some());
        let gpu = caps.gpu_info.unwrap();
        assert_eq!(gpu.name, "Mock GPU");
        assert_eq!(gpu.vram_gb, Some(8.0));
        assert_eq!(gpu.compute_type, "Metal");
    }

    #[tokio::test]
    async fn test_get_system_capabilities_with_cuda_gpu() {
        let mock_info = Arc::new(MockSystemInfoPort::new());
        mock_info.set_gpu(Some(GpuInfo {
            name: "NVIDIA RTX 4090".to_string(),
            vram_gb: Some(24.0),
            compute_type: ComputeType::Cuda,
        }));

        let use_case = GetSystemCapabilitiesUseCase::new(mock_info);
        let result = use_case.execute().await;

        assert!(result.is_ok());
        let caps = result.unwrap();

        assert!(caps.gpu_info.is_some());
        let gpu = caps.gpu_info.unwrap();
        assert_eq!(gpu.name, "NVIDIA RTX 4090");
        assert_eq!(gpu.vram_gb, Some(24.0));
        assert_eq!(gpu.compute_type, "CUDA");
    }

    #[tokio::test]
    async fn test_get_system_capabilities_with_rocm_gpu() {
        let mock_info = Arc::new(MockSystemInfoPort::new());
        mock_info.set_gpu(Some(GpuInfo {
            name: "AMD Radeon RX 7900 XTX".to_string(),
            vram_gb: Some(24.0),
            compute_type: ComputeType::Rocm,
        }));

        let use_case = GetSystemCapabilitiesUseCase::new(mock_info);
        let result = use_case.execute().await;

        assert!(result.is_ok());
        let caps = result.unwrap();

        assert!(caps.gpu_info.is_some());
        let gpu = caps.gpu_info.unwrap();
        assert_eq!(gpu.name, "AMD Radeon RX 7900 XTX");
        assert_eq!(gpu.vram_gb, Some(24.0));
        assert_eq!(gpu.compute_type, "ROCm");
    }

    #[tokio::test]
    async fn test_get_system_capabilities_custom_ram() {
        let mock_info = Arc::new(MockSystemInfoPort::new());
        mock_info.set_ram(64.0);

        let use_case = GetSystemCapabilitiesUseCase::new(mock_info);
        let result = use_case.execute().await;

        assert!(result.is_ok());
        let caps = result.unwrap();
        assert_eq!(caps.total_ram_gb, 64.0);
    }

    #[tokio::test]
    async fn test_get_system_capabilities_custom_cpu() {
        let mock_info = Arc::new(MockSystemInfoPort::new());
        mock_info.set_cpu(24, "AMD Ryzen 9 7950X".to_string());

        let use_case = GetSystemCapabilitiesUseCase::new(mock_info);
        let result = use_case.execute().await;

        assert!(result.is_ok());
        let caps = result.unwrap();
        assert_eq!(caps.cpu_cores, 24);
        assert_eq!(caps.cpu_model, "AMD Ryzen 9 7950X");
    }

    #[tokio::test]
    async fn test_get_system_capabilities_gpu_without_vram() {
        let mock_info = Arc::new(MockSystemInfoPort::new());
        mock_info.set_gpu(Some(GpuInfo {
            name: "Integrated Graphics".to_string(),
            vram_gb: None, // VRAM not detectable
            compute_type: ComputeType::None,
        }));

        let use_case = GetSystemCapabilitiesUseCase::new(mock_info);
        let result = use_case.execute().await;

        assert!(result.is_ok());
        let caps = result.unwrap();

        assert!(caps.gpu_info.is_some());
        let gpu = caps.gpu_info.unwrap();
        assert_eq!(gpu.name, "Integrated Graphics");
        assert!(gpu.vram_gb.is_none());
        assert_eq!(gpu.compute_type, "None");
    }
}
