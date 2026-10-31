//! The one place Lattice asks what compute devices exist: the bundled
//! llama-server's own `--list-devices`, which enumerates ggml's backends
//! (CUDA, ROCm, Vulkan, SYCL, Metal) with free VRAM and lists a device only
//! when it is usable for inference.

use std::time::Duration;

use lattice_core::application::ports::{AcceleratorProbe, ComputeType, GpuInfo};
use tauri::AppHandle;
use tokio::sync::OnceCell;

use super::gpu::{map_vendor_from_name, GPUVendor};

/// Device enumeration loads each backend's runtime, so it is slower than
/// `--version` but still bounded. Past this the answer is not worth waiting for
/// and the heuristics take over.
const PROBE_TIMEOUT: Duration = Duration::from_secs(20);

/// One compute device as ggml reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendDevice {
    /// Backend-qualified id, e.g. `CUDA0`, `MTL0`, `Vulkan1`, `BLAS`.
    pub id: String,
    /// Human-readable device name, e.g. `NVIDIA GeForce RTX 4090`.
    pub name: String,
    pub total_mib: u64,
    pub free_mib: u64,
}

impl BackendDevice {
    /// The alphabetic prefix of the id names the backend: `CUDA0` -> `CUDA`.
    pub fn backend(&self) -> &str {
        let end = self
            .id
            .find(|c: char| c.is_ascii_digit())
            .unwrap_or(self.id.len());
        &self.id[..end]
    }

    /// Which vendor's hardware this is, decided by backend first because a
    /// backend only ever runs on its own vendor's devices. Vulkan and SYCL are
    /// the exceptions — they are portable, so the device name decides.
    pub fn vendor(&self) -> GPUVendor {
        match self.backend().to_ascii_lowercase().as_str() {
            "cuda" => GPUVendor::Nvidia,
            "rocm" | "hip" => GPUVendor::AMD,
            "mtl" | "metal" => GPUVendor::Apple,
            "vulkan" | "sycl" | "opencl" => map_vendor_from_name(&self.name),
            _ => GPUVendor::Unknown,
        }
    }

    /// `BLAS`, `CPU` and `RPC` are listed beside the real devices but none of
    /// them is an accelerator we can offload layers to. BLAS in particular
    /// reports `0 MiB` and is just the CPU's linear-algebra library.
    pub fn is_accelerator(&self) -> bool {
        !matches!(
            self.backend().to_ascii_lowercase().as_str(),
            "blas" | "cpu" | "rpc" | ""
        ) && self.vendor().is_accelerated()
    }

    /// Whether this GPU runs from the system's RAM, told by its name since
    /// `--list-devices` gives no type.
    fn is_integrated(&self) -> bool {
        let name = self.name.to_ascii_lowercase();
        let named = |models: &[&str]| models.iter().any(|model| name.contains(model));
        match self.vendor() {
            GPUVendor::Intel => !named(&["arc(tm) a", "arc a", "arc(tm) b5", "arc b5", "arc pro"]),
            GPUVendor::AMD => !named(&[" rx ", "(tm) rx", "radeon pro", "firepro"]),
            _ => false,
        }
    }

    pub fn total_gb(&self) -> f64 {
        self.total_mib as f64 / 1024.0
    }
}

/// The device a server's layers land on, the largest when there are several.
/// llama.cpp leaves an integrated GPU out while a dedicated one is listed, and
/// the memory an integrated GPU lists is the machine's RAM, so listing the
/// most does not make it the one.
pub fn offload_device(devices: &[BackendDevice]) -> Option<&BackendDevice> {
    let accelerators = || devices.iter().filter(|device| device.is_accelerator());
    accelerators()
        .filter(|device| !device.is_integrated())
        .max_by_key(|device| device.total_mib)
        .or_else(|| accelerators().max_by_key(|device| device.total_mib))
}

/// A GPU for a small server when the offload device has no room for it: the
/// integrated one beside a dedicated card, which draws on the system's RAM and
/// leaves the card to its models.
pub fn spare_device(devices: &[BackendDevice]) -> Option<&BackendDevice> {
    let offload = offload_device(devices)?;
    devices
        .iter()
        .filter(|device| device.is_accelerator() && device.is_integrated())
        .filter(|device| device.id != offload.id)
        .max_by_key(|device| device.total_mib)
}

/// Parse the `Available devices:` block of `llama-server --list-devices`.
///
/// Everything before the header is backend chatter (Metal capability dumps,
/// CUDA driver notices) and is ignored. A line that does not match the shape is
/// skipped rather than failing the probe, so a future column or a reworded
/// banner costs us one device, never the whole answer.
pub fn parse_list_devices(output: &str) -> Vec<BackendDevice> {
    let mut devices = Vec::new();
    let mut in_block = false;

    for line in output.lines() {
        let trimmed = line.trim();
        if !in_block {
            if trimmed.eq_ignore_ascii_case("Available devices:") {
                in_block = true;
            }
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        if let Some(device) = parse_device_line(trimmed) {
            devices.push(device);
        }
    }

    devices
}

/// `CUDA0: NVIDIA GeForce RTX 4090 (24210 MiB, 23486 MiB free)`
fn parse_device_line(line: &str) -> Option<BackendDevice> {
    let (id, rest) = line.split_once(':')?;
    let id = id.trim();
    if id.is_empty() || id.contains(char::is_whitespace) {
        return None;
    }

    let open = rest.rfind('(')?;
    let close = rest.rfind(')')?;
    if close < open {
        return None;
    }
    let name = rest[..open].trim().to_string();
    if name.is_empty() {
        return None;
    }

    let mut memory = rest[open + 1..close].split(',');
    let total_mib = parse_mib(memory.next()?)?;
    let free_mib = memory.next().and_then(parse_mib).unwrap_or(total_mib);

    Some(BackendDevice {
        id: id.to_string(),
        name,
        total_mib,
        free_mib,
    })
}

/// `24210 MiB` or `23486 MiB free`.
fn parse_mib(field: &str) -> Option<u64> {
    let mut parts = field.split_whitespace();
    let value = parts.next()?.parse::<u64>().ok()?;
    match parts.next() {
        Some(unit) if unit.eq_ignore_ascii_case("MiB") || unit.eq_ignore_ascii_case("MB") => {
            Some(value)
        }
        _ => None,
    }
}

/// What the sidecar reported it can run on, probed once per app: enumeration
/// costs a backend init, and the set of installed GPUs does not change while
/// the app runs.
///
/// The container owns one. The model loads and the startup preflight probe
/// through it; the system-info adapter, which has no `AppHandle`, reads what
/// they found.
#[derive(Debug, Default)]
pub struct BackendDevices {
    probed: OnceCell<Vec<BackendDevice>>,
}

impl BackendDevices {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ask the sidecar what it can run on, caching the answer.
    ///
    /// An empty slice means the probe ran and found nothing, or could not run
    /// at all; both leave the caller on its fallback path rather than asserting
    /// a machine has no GPU.
    pub async fn detect(&self, app: &AppHandle) -> &[BackendDevice] {
        self.probed
            .get_or_init(|| async { probe(app).await.unwrap_or_default() })
            .await
            .as_slice()
    }

    /// The cached answer for callers with no `AppHandle`. `None` means nothing
    /// has probed yet, which is different from "no devices".
    pub fn cached(&self) -> Option<&[BackendDevice]> {
        self.probed.get().map(Vec::as_slice)
    }
}

/// The system-info adapter's view of the probe: the largest accelerator the
/// sidecar listed, or `None` before anything has probed.
impl AcceleratorProbe for BackendDevices {
    fn probed_gpu(&self) -> Option<GpuInfo> {
        let device = offload_device(self.cached()?)?;

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
}

async fn probe(app: &AppHandle) -> Option<Vec<BackendDevice>> {
    let (stdout, stderr) = super::super::sidecar_manager::SpawnedChild::run_bounded_probe(
        app,
        super::super::sidecar_manager::SidecarBinary::Primary,
        "--list-devices",
        "probe://llama-server/list-devices",
        PROBE_TIMEOUT,
    )
    .await?;

    // ggml writes its banner to stderr and the device list to stdout, but which
    // stream carries what has moved between builds, so read both.
    let mut text = String::from_utf8_lossy(&stdout).into_owned();
    text.push('\n');
    text.push_str(&String::from_utf8_lossy(&stderr));

    let devices = parse_list_devices(&text);
    let accelerators: Vec<&BackendDevice> = devices.iter().filter(|d| d.is_accelerator()).collect();
    tracing::info!(
        device_count = devices.len(),
        accelerators = accelerators.len(),
        detail = %devices
            .iter()
            .map(|d| format!("{}={} ({} MiB)", d.id, d.name, d.total_mib))
            .collect::<Vec<_>>()
            .join(", "),
        "llama-server device probe"
    );

    Some(devices)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Captured from the pinned b8981 build on an M3 Max.
    const METAL: &str = "\
ggml_metal_device_init: GPU name:   MTL0 (Apple M3 Max)
ggml_metal_device_init: recommendedMaxWorkingSetSize  = 30150.67 MB
Available devices:
  MTL0: Apple M3 Max (28753 MiB, 28753 MiB free)
  BLAS: Accelerate (0 MiB, 0 MiB free)
";

    const CUDA: &str = "\
ggml_cuda_init: found 2 CUDA devices:
Available devices:
  CUDA0: NVIDIA GeForce RTX 4090 (24210 MiB, 23486 MiB free)
  CUDA1: NVIDIA GeForce RTX 3090 (24576 MiB, 24576 MiB free)
";

    const VULKAN_AMD: &str = "\
Available devices:
  Vulkan0: AMD Radeon RX 7900 XTX (24560 MiB, 24100 MiB free)
";

    const CPU_ONLY: &str = "\
Available devices:
  CPU: AMD Ryzen 9 7950X (0 MiB, 0 MiB free)
";

    /// Captured from a laptop whose servers all run on the 8 GB card.
    const HYBRID_LAPTOP: &str = "\
Available devices:
  Vulkan0: Intel(R) Iris(R) Xe Graphics (32581 MiB, 32581 MiB free)
  Vulkan1: NVIDIA GeForce RTX 4070 Laptop GPU (7948 MiB, 7180 MiB free)
";

    fn device(name: &str, total_mib: u64) -> BackendDevice {
        BackendDevice {
            id: "Vulkan0".into(),
            name: name.into(),
            total_mib,
            free_mib: total_mib,
        }
    }

    #[test]
    fn a_dedicated_card_is_the_offload_device_beside_a_larger_integrated_gpu() {
        let devices = parse_list_devices(HYBRID_LAPTOP);
        let device = offload_device(&devices).unwrap();
        assert_eq!(device.id, "Vulkan1");
        assert_eq!(device.free_mib, 7180);
    }

    #[test]
    fn an_integrated_gpu_on_its_own_is_the_offload_device() {
        let devices = [device("Intel(R) UHD Graphics 770", 16000)];
        assert_eq!(offload_device(&devices), devices.first());
    }

    #[test]
    fn the_largest_dedicated_card_is_the_offload_device() {
        let devices = parse_list_devices(CUDA);
        assert_eq!(offload_device(&devices).unwrap().id, "CUDA1");
    }

    #[test]
    fn no_accelerator_means_no_offload_device() {
        assert_eq!(offload_device(&parse_list_devices(CPU_ONLY)), None);
    }

    #[test]
    fn the_integrated_gpu_is_spare_beside_a_dedicated_card() {
        let devices = parse_list_devices(HYBRID_LAPTOP);

        let spare = spare_device(&devices).expect("the Iris Xe beside the RTX");
        assert_eq!(spare.id, "Vulkan0");
    }

    #[test]
    fn nothing_is_spare_without_both_kinds_of_gpu() {
        assert!(spare_device(&[device("Intel(R) Iris(R) Xe Graphics", 32_581)]).is_none());
        assert!(spare_device(&[device("NVIDIA GeForce RTX 4070", 7_948)]).is_none());
        assert!(spare_device(&parse_list_devices(CPU_ONLY)).is_none());
    }

    #[test]
    fn a_gpu_name_tells_integrated_from_dedicated() {
        for name in [
            "Intel(R) Iris(R) Xe Graphics",
            "Intel(R) UHD Graphics 630",
            "Intel(R) Arc(TM) Graphics",
            "Intel(R) Arc(TM) 140V GPU",
            "AMD Radeon(TM) Graphics",
            "AMD Radeon 780M Graphics",
            "AMD Radeon Vega 8 Graphics",
        ] {
            assert!(device(name, 16000).is_integrated(), "{name}");
        }
        for name in [
            "NVIDIA GeForce RTX 4070 Laptop GPU",
            "Intel(R) Arc(TM) A770 Graphics",
            "Intel(R) Arc(TM) B580 Graphics",
            "AMD Radeon RX 7900 XTX",
            "AMD Radeon(TM) RX 6600M",
            "AMD Radeon PRO W7900",
        ] {
            assert!(!device(name, 16000).is_integrated(), "{name}");
        }
    }

    #[test]
    fn metal_device_is_read_with_its_memory() {
        let devices = parse_list_devices(METAL);
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].id, "MTL0");
        assert_eq!(devices[0].name, "Apple M3 Max");
        assert_eq!(devices[0].total_mib, 28753);
        assert_eq!(devices[0].free_mib, 28753);
        assert_eq!(devices[0].vendor(), GPUVendor::Apple);
        assert!(devices[0].is_accelerator());
    }

    /// The bug that started this: a machine with Metal reported as CPU-only.
    #[test]
    fn a_metal_machine_reports_an_accelerator() {
        assert!(parse_list_devices(METAL).iter().any(|d| d.is_accelerator()));
    }

    #[test]
    fn blas_sits_beside_the_real_devices_but_is_not_one() {
        let blas = &parse_list_devices(METAL)[1];
        assert_eq!(blas.id, "BLAS");
        assert!(!blas.is_accelerator());
    }

    #[test]
    fn every_cuda_device_is_listed_with_free_memory_separate_from_total() {
        let devices = parse_list_devices(CUDA);
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].vendor(), GPUVendor::Nvidia);
        assert_eq!(devices[0].total_mib, 24210);
        assert_eq!(devices[0].free_mib, 23486);
        assert_eq!(devices[1].id, "CUDA1");
    }

    /// Vulkan runs on anyone's hardware, so the backend cannot name the vendor.
    #[test]
    fn a_portable_backend_takes_its_vendor_from_the_device_name() {
        let devices = parse_list_devices(VULKAN_AMD);
        assert_eq!(devices[0].backend(), "Vulkan");
        assert_eq!(devices[0].vendor(), GPUVendor::AMD);
        assert!(devices[0].is_accelerator());
    }

    #[test]
    fn a_machine_with_no_gpu_reports_no_accelerator() {
        let devices = parse_list_devices(CPU_ONLY);
        assert_eq!(devices.len(), 1);
        assert!(!devices[0].is_accelerator());
    }

    #[test]
    fn backend_chatter_before_the_header_is_not_mistaken_for_a_device() {
        let devices = parse_list_devices(METAL);
        assert!(devices.iter().all(|d| !d.name.contains("recommendedMax")));
    }

    /// A reworded banner or an extra column should cost one device, never the
    /// whole probe.
    #[test]
    fn an_unparseable_line_is_skipped_rather_than_failing_the_probe() {
        let text = "Available devices:\n  what even is this\n  MTL0: Apple M3 Max (100 MiB, 100 MiB free)\n";
        let devices = parse_list_devices(text);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id, "MTL0");
    }

    #[test]
    fn output_without_the_header_yields_nothing_rather_than_garbage() {
        assert!(parse_list_devices("llama-server --help\n  -m FNAME\n").is_empty());
    }

    /// A device description may itself contain parentheses, so the memory
    /// column is the LAST parenthesised group, not the first.
    #[test]
    fn a_name_with_parentheses_does_not_swallow_the_memory_column() {
        let devices =
            parse_list_devices("Available devices:\n  CUDA0: NVIDIA RTX 4090 (Compute 8.9) (24210 MiB, 23486 MiB free)\n");
        assert_eq!(devices[0].name, "NVIDIA RTX 4090 (Compute 8.9)");
        assert_eq!(devices[0].total_mib, 24210);
    }

    /// Every backend prints through one format string in ggml:
    /// `  %s: %s (%zu MiB, %zu MiB free)`. Verified against the pinned build
    /// with `strings`. These are the device names each backend registers.
    #[test]
    fn the_shared_format_string_parses_for_every_backend() {
        for (line, vendor) in [
            (
                "  CUDA0: NVIDIA GeForce RTX 4090 (24210 MiB, 23486 MiB free)",
                GPUVendor::Nvidia,
            ),
            (
                "  ROCm0: AMD Radeon RX 7900 XTX (24560 MiB, 24100 MiB free)",
                GPUVendor::AMD,
            ),
            (
                "  Vulkan0: Intel Arc A770 (16384 MiB, 16000 MiB free)",
                GPUVendor::Intel,
            ),
            (
                "  SYCL0: Intel Arc A770 (16384 MiB, 16000 MiB free)",
                GPUVendor::Intel,
            ),
            (
                "  MTL0: Apple M3 Max (28753 MiB, 28753 MiB free)",
                GPUVendor::Apple,
            ),
        ] {
            let device = parse_device_line(line.trim()).expect("shared format parses");
            assert_eq!(device.vendor(), vendor, "vendor for {line}");
            assert!(device.is_accelerator(), "accelerator for {line}");
            assert!(device.total_mib > 0, "memory for {line}");
        }
    }

    /// The parser fixtures above are hand-written; this one runs the real
    /// pinned binary. Ignored by default because it needs the sidecar built,
    /// but it is what proves the fixtures match reality on any dev machine:
    /// `cargo test -- --ignored live_sidecar`.
    #[test]
    #[ignore = "requires the llama-server sidecar to be built"]
    fn live_sidecar_device_list_parses_on_this_machine() {
        let binary = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/debug/llama-server");
        if !binary.exists() {
            eprintln!("skipping: no sidecar at {}", binary.display());
            return;
        }

        let output = std::process::Command::new(&binary)
            .arg("--list-devices")
            .output()
            .expect("sidecar runs");
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        text.push('\n');
        text.push_str(&String::from_utf8_lossy(&output.stderr));

        let devices = parse_list_devices(&text);
        eprintln!("PARSED {} DEVICES:", devices.len());
        for device in &devices {
            eprintln!(
                "  {} | {} | {} MiB | accelerator={} | vendor={:?}",
                device.id,
                device.name,
                device.total_mib,
                device.is_accelerator(),
                device.vendor()
            );
        }

        assert!(
            !devices.is_empty(),
            "the binary lists devices but the parser found none:\n{text}"
        );
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        assert!(
            devices.iter().any(|d| d.is_accelerator()),
            "Apple Silicon must report an accelerator"
        );
    }
}
