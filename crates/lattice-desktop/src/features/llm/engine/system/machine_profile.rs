//! Builds the recommender's [`MachineProfile`] from what the backend and the host report.

use super::backend_devices::BackendDevice;
use lattice_core::domain::models::fit::{Accelerator, AcceleratorKind, Backend, MachineProfile};
use std::path::{Path, PathBuf};

const BYTES_PER_MIB: u64 = 1 << 20;

/// Free space on one mounted volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeSpace {
    pub mount_point: PathBuf,
    pub free_bytes: u64,
}

/// The profile of this computer: the accelerators `devices` lists (a dedicated card, an integrated
/// GPU, or a unified Metal device, names kept; BLAS, CPU and RPC entries are not accelerators),
/// with `system_ram_bytes`, the models volume's free space and the operating system.
pub fn machine_profile(
    devices: &[BackendDevice],
    system_ram_bytes: u64,
    models_volume_free_bytes: u64,
    os: &str,
) -> MachineProfile {
    MachineProfile {
        accelerators: devices
            .iter()
            .filter(|device| device.is_accelerator())
            .filter_map(accelerator)
            .collect(),
        system_ram_bytes,
        models_volume_free_bytes,
        os: os.to_owned(),
    }
}

fn accelerator(device: &BackendDevice) -> Option<Accelerator> {
    let backend = match device.backend().to_ascii_lowercase().as_str() {
        "cuda" => Backend::Cuda,
        "rocm" | "hip" => Backend::Rocm,
        "vulkan" => Backend::Vulkan,
        "mtl" | "metal" => Backend::Metal,
        "sycl" => Backend::Sycl,
        _ => return None,
    };
    let kind = if backend == Backend::Metal {
        AcceleratorKind::Unified
    } else if device.is_integrated() {
        AcceleratorKind::Integrated
    } else {
        AcceleratorKind::Dedicated
    };
    Some(Accelerator {
        name: device.name.clone(),
        total_bytes: device.total_mib.saturating_mul(BYTES_PER_MIB),
        kind,
        backend,
    })
}

/// Free space on the volume `models_dir` lives on: the volume with the longest mount point that
/// is a parent of it, not the sum of all disks. Zero when no volume contains it.
pub fn models_volume_free_bytes(models_dir: &Path, volumes: &[VolumeSpace]) -> u64 {
    volumes
        .iter()
        .filter(|volume| models_dir.starts_with(&volume.mount_point))
        .max_by_key(|volume| volume.mount_point.components().count())
        .map_or(0, |volume| volume.free_bytes)
}

#[cfg(test)]
mod tests;
