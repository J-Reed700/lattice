//! GPU detection and capabilities.

use serde::{Deserialize, Serialize};
use sysinfo::{Components, System};

/// GPU vendor/type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GPUVendor {
    /// Apple Silicon (M1, M2, M3, etc.) with Metal
    Apple,
    /// NVIDIA GPU (backend availability is platform-specific)
    Nvidia,
    /// AMD GPU (backend availability is platform-specific)
    AMD,
    /// Intel integrated graphics (backend availability is platform-specific)
    Intel,
    /// Unknown or unsupported GPU
    Unknown,
}

impl GPUVendor {
    /// Get the acceleration backend name for this vendor.
    pub fn backend_name(&self) -> &'static str {
        match self {
            GPUVendor::Apple => "metal",
            GPUVendor::Nvidia => "gpu",
            GPUVendor::AMD => "gpu",
            GPUVendor::Intel => "gpu",
            GPUVendor::Unknown => "cpu",
        }
    }

    pub fn is_accelerated(&self) -> bool {
        !matches!(self, GPUVendor::Unknown)
    }
}

/// GPU information and capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GPUInfo {
    /// GPU vendor/type
    pub vendor: GPUVendor,

    /// GPU model name (e.g., "Apple M1 Pro", "NVIDIA GeForce RTX 3080")
    pub name: String,

    /// VRAM in GB (if detectable)
    pub vram_gb: Option<f64>,

    /// Compute capability or architecture version
    pub compute_capability: Option<String>,
}

/// Detect GPU information for the current system.
///
/// This is a best-effort detection that may not work on all systems.
/// Falls back to CPU-only if GPU cannot be detected.
pub async fn detect_gpu() -> Option<GPUInfo> {
    detect_gpu_sysinfo()
}

fn detect_gpu_sysinfo() -> Option<GPUInfo> {
    let components = Components::new_with_refreshed_list();
    let mut system = System::new();
    system.refresh_cpu();

    let mut name = components
        .iter()
        .map(|component| component.label())
        .find(|label| is_gpu_label(label))
        .map(|label| label.to_string());

    if name.is_none() {
        let cpu_brand = system.global_cpu_info().brand().to_string();
        if cpu_brand.to_lowercase().contains("apple") {
            name = Some(format!("{cpu_brand} GPU"));
        }
    }

    let name = name?;
    let vendor = map_vendor_from_name(&name);
    if vendor == GPUVendor::Unknown {
        return None;
    }

    Some(GPUInfo {
        vendor,
        name,
        vram_gb: None,
        compute_capability: None,
    })
}

fn map_vendor_from_name(name: &str) -> GPUVendor {
    let lower = name.to_lowercase();
    if lower.contains("apple")
        || lower.contains("m1")
        || lower.contains("m2")
        || lower.contains("m3")
    {
        GPUVendor::Apple
    } else if lower.contains("nvidia")
        || lower.contains("geforce")
        || lower.contains("rtx")
        || lower.contains("gtx")
        || lower.contains("quadro")
        || lower.contains("tesla")
    {
        GPUVendor::Nvidia
    } else if lower.contains("amd")
        || lower.contains("radeon")
        || lower.contains("rx")
        || lower.contains("vega")
    {
        GPUVendor::AMD
    } else if lower.contains("intel")
        || lower.contains("arc")
        || lower.contains("iris")
        || lower.contains("uhd")
        || lower.contains("xe")
    {
        GPUVendor::Intel
    } else {
        GPUVendor::Unknown
    }
}

fn is_gpu_label(label: &str) -> bool {
    let lower = label.to_lowercase();
    lower.contains("gpu")
        || lower.contains("nvidia")
        || lower.contains("geforce")
        || lower.contains("rtx")
        || lower.contains("gtx")
        || lower.contains("amd")
        || lower.contains("radeon")
        || lower.contains("intel")
        || lower.contains("arc")
        || lower.contains("iris")
        || lower.contains("uhd")
        || lower.contains("xe")
        || lower.contains("apple")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_vendor_backend() {
        assert_eq!(GPUVendor::Apple.backend_name(), "metal");
        assert_eq!(GPUVendor::Nvidia.backend_name(), "gpu");
        assert_eq!(GPUVendor::AMD.backend_name(), "gpu");
        assert_eq!(GPUVendor::Intel.backend_name(), "gpu");
        assert_eq!(GPUVendor::Unknown.backend_name(), "cpu");
    }

    #[tokio::test]
    async fn test_detect_gpu() {
        // Should not panic, may return None on systems without GPU
        let _gpu_info = detect_gpu().await;
    }
}
