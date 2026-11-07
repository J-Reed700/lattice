#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::features::llm::engine::system::backend_devices::parse_list_devices;
use lattice_core::domain::models::fit::{Accelerator, AcceleratorKind, Backend};

const GIB: u64 = 1 << 30;
const MIB: u64 = 1 << 20;

const HYBRID_LAPTOP: &str = "\
Available devices:
  Vulkan0: Intel(R) Iris(R) Xe Graphics (32581 MiB, 32581 MiB free)
  Vulkan1: NVIDIA GeForce RTX 4070 Laptop GPU (7948 MiB, 7180 MiB free)
";

const DESKTOP_4080: &str = "\
ggml_cuda_init: found 1 CUDA devices:
Available devices:
  CUDA0: NVIDIA GeForce RTX 4080 (16376 MiB, 15200 MiB free)
";

const APPLE: &str = "\
Available devices:
  MTL0: Apple M3 Max (28753 MiB, 28753 MiB free)
  BLAS: Accelerate (0 MiB, 0 MiB free)
";

const CPU_ONLY: &str = "\
Available devices:
  CPU: AMD Ryzen 9 7950X (0 MiB, 0 MiB free)
";

const VULKAN_AMD: &str = "\
Available devices:
  Vulkan0: AMD Radeon RX 7900 XTX (24560 MiB, 24100 MiB free)
";

fn profile(output: &str) -> MachineProfile {
    machine_profile(&parse_list_devices(output), 32 * GIB, 500 * GIB, "windows")
}

fn find(profile: &MachineProfile, kind: AcceleratorKind) -> Vec<&Accelerator> {
    profile
        .accelerators
        .iter()
        .filter(|a| a.kind == kind)
        .collect()
}

#[test]
fn a_hybrid_laptop_has_one_dedicated_card_and_one_integrated_gpu_with_their_names() {
    let profile = profile(HYBRID_LAPTOP);

    assert_eq!(profile.accelerators.len(), 2);
    let dedicated = find(&profile, AcceleratorKind::Dedicated);
    let integrated = find(&profile, AcceleratorKind::Integrated);
    assert_eq!(dedicated.len(), 1);
    assert_eq!(integrated.len(), 1);
    assert_eq!(dedicated[0].name, "NVIDIA GeForce RTX 4070 Laptop GPU");
    assert_eq!(dedicated[0].total_bytes, 7948 * MIB);
    assert_eq!(dedicated[0].backend, Backend::Vulkan);
    assert_eq!(integrated[0].name, "Intel(R) Iris(R) Xe Graphics");
    assert_eq!(integrated[0].backend, Backend::Vulkan);
}

#[test]
fn a_desktop_card_alone_is_one_dedicated_accelerator() {
    let profile = profile(DESKTOP_4080);

    assert_eq!(profile.accelerators.len(), 1);
    let card = &profile.accelerators[0];
    assert_eq!(card.kind, AcceleratorKind::Dedicated);
    assert_eq!(card.backend, Backend::Cuda);
    assert_eq!(card.name, "NVIDIA GeForce RTX 4080");
    assert_eq!(card.total_bytes, 16376 * MIB);
}

#[test]
fn an_apple_metal_device_is_one_unified_accelerator_of_its_listed_size() {
    let profile = machine_profile(&parse_list_devices(APPLE), 36 * GIB, 500 * GIB, "macos");

    assert_eq!(profile.accelerators.len(), 1);
    let device = &profile.accelerators[0];
    assert_eq!(device.kind, AcceleratorKind::Unified);
    assert_eq!(device.backend, Backend::Metal);
    assert_eq!(device.name, "Apple M3 Max");
    assert_eq!(device.total_bytes, 28753 * MIB);
    assert_eq!(profile.os, "macos");
}

#[test]
fn no_accelerators_is_a_processor_only_profile() {
    assert!(profile(CPU_ONLY).accelerators.is_empty());
    assert!(machine_profile(&[], 16 * GIB, GIB, "linux")
        .accelerators
        .is_empty());
}

#[test]
fn a_vulkan_card_maps_to_the_vulkan_backend() {
    let profile = profile(VULKAN_AMD);

    assert_eq!(profile.accelerators.len(), 1);
    assert_eq!(profile.accelerators[0].backend, Backend::Vulkan);
    assert_eq!(profile.accelerators[0].kind, AcceleratorKind::Dedicated);
}

#[test]
fn the_host_facts_pass_through() {
    let profile = machine_profile(&parse_list_devices(CPU_ONLY), 24 * GIB, 77 * GIB, "linux");

    assert_eq!(profile.system_ram_bytes, 24 * GIB);
    assert_eq!(profile.models_volume_free_bytes, 77 * GIB);
    assert_eq!(profile.os, "linux");
}

#[test]
fn the_free_space_is_the_volume_the_models_live_on() {
    let volumes = [
        VolumeSpace {
            mount_point: PathBuf::from("/"),
            free_bytes: 10 * GIB,
        },
        VolumeSpace {
            mount_point: PathBuf::from("/data"),
            free_bytes: 500 * GIB,
        },
        VolumeSpace {
            mount_point: PathBuf::from("/database"),
            free_bytes: 3 * GIB,
        },
    ];

    assert_eq!(
        models_volume_free_bytes(Path::new("/data/lattice/models"), &volumes),
        500 * GIB
    );
    assert_eq!(
        models_volume_free_bytes(Path::new("/home/me/models"), &volumes),
        10 * GIB
    );
    assert_eq!(
        models_volume_free_bytes(Path::new("relative/models"), &[]),
        0
    );
}
