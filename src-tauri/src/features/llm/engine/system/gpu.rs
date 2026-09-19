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
    from_backend_devices()
        .or_else(detect_apple_silicon)
        .or_else(detect_gpu_sysinfo)
}

/// The authoritative form: ask the sidecar that will do the offloading.
///
/// Prefer this wherever an `AppHandle` is in reach. It reports a device only if
/// ggml can actually use it — right driver, right runtime — which is the
/// question every caller is really asking, and it needs no per-platform code.
/// The heuristics below remain only for when the probe cannot run.
pub async fn detect_gpu_with_sidecar(app: &tauri::AppHandle) -> Option<GPUInfo> {
    let devices = super::backend_devices::detect_backend_devices(app).await;
    gpu_from_devices(devices)
        .or_else(detect_apple_silicon)
        .or_else(detect_gpu_sysinfo)
}

/// The probe's cached answer, for callers with no `AppHandle`.
fn from_backend_devices() -> Option<GPUInfo> {
    gpu_from_devices(super::backend_devices::cached_backend_devices()?)
}

/// The largest accelerator decides, since that is the one worth offloading to.
fn gpu_from_devices(devices: &[super::backend_devices::BackendDevice]) -> Option<GPUInfo> {
    let device = devices
        .iter()
        .filter(|device| device.is_accelerator())
        .max_by_key(|device| device.total_mib)?;

    Some(GPUInfo {
        vendor: device.vendor(),
        name: device.name.clone(),
        // Apple's unified memory reports the whole working set here, which is
        // what llama.cpp budgets against, so it is the right number for both.
        vram_gb: (device.total_mib > 0).then(|| device.total_gb()),
        compute_capability: Some(device.backend().to_string()),
    })
}

/// Every Apple Silicon Mac has a Metal GPU. This is a property of the target,
/// not of the running machine, so it is known at compile time and needs no
/// probing.
///
/// Probing was the bug: `sysinfo` names macOS thermal sensors `PMU tdieN` —
/// nothing that reads as a GPU — and returns an empty CPU brand string, so both
/// heuristics below miss and an M3 Max reported no accelerator. Every local
/// model then started with `n_gpu_layers = 0` and a 4K context, turning a
/// two-second query rewrite into a thirty-second timeout.
fn detect_apple_silicon() -> Option<GPUInfo> {
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        return None;
    }

    Some(GPUInfo {
        vendor: GPUVendor::Apple,
        name: apple_silicon_name(),
        vram_gb: None,
        compute_capability: None,
    })
}

/// The chip name is cosmetic — it reaches logs and the settings screen, never a
/// decision — so an unreadable one falls back to a generic label.
fn apple_silicon_name() -> String {
    std::process::Command::new("/usr/sbin/sysctl")
        .args(["-n", "machdep.cpu.brand_string"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|brand| brand.trim().to_string())
        .filter(|brand| !brand.is_empty())
        .map(|brand| format!("{brand} GPU"))
        .unwrap_or_else(|| "Apple Silicon GPU".to_string())
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
    // A sensor label can look GPU-ish ("GPU 1") without naming a vendor. That is
    // not evidence of no GPU, so fall back to the CPU brand rather than
    // concluding the machine has none.
    let mut vendor = map_vendor_from_name(&name);
    let mut name = name;
    if vendor == GPUVendor::Unknown {
        let cpu_brand = system.global_cpu_info().brand().to_string();
        vendor = map_vendor_from_name(&cpu_brand);
        if vendor == GPUVendor::Unknown {
            return None;
        }
        name = format!("{cpu_brand} GPU");
    }

    Some(GPUInfo {
        vendor,
        name,
        vram_gb: None,
        compute_capability: None,
    })
}

pub(super) fn map_vendor_from_name(name: &str) -> GPUVendor {
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
        let _gpu_info = detect_gpu().await;
    }

    /// The failure this guards against was silent: detection returned `None`,
    /// the sidecar started CPU-only, and the only symptom was everything being
    /// slow. On a Mac that can run Metal, saying "no accelerator" is wrong.
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[tokio::test]
    async fn apple_silicon_always_reports_an_accelerator() {
        let info = detect_gpu().await.expect("Apple Silicon always has Metal");
        assert_eq!(info.vendor, GPUVendor::Apple);
        assert!(info.vendor.is_accelerated());
        assert!(!info.name.is_empty());
    }

    /// Thermal sensors on macOS are named `PMU tdieN` and the CPU brand comes
    /// back empty, which is exactly the input that produced the bug.
    #[test]
    fn sensorless_machine_does_not_decide_there_is_no_gpu_on_apple_silicon() {
        assert_eq!(map_vendor_from_name("PMU tdie8"), GPUVendor::Unknown);
        assert_eq!(map_vendor_from_name(""), GPUVendor::Unknown);
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        assert!(detect_apple_silicon().is_some());
    }

    /// A vendorless "GPU" sensor label used to abort detection outright.
    #[test]
    fn a_vendorless_gpu_label_is_still_a_gpu_label() {
        assert!(is_gpu_label("GPU 1"));
        assert_eq!(map_vendor_from_name("GPU 1"), GPUVendor::Unknown);
    }
}
