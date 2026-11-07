//! Builds the recommender's [`MachineProfile`] from what the backend and the host report.

use super::backend_devices::BackendDevice;
use lattice_core::domain::models::fit::MachineProfile;
use std::path::{Path, PathBuf};

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
    _devices: &[BackendDevice],
    _system_ram_bytes: u64,
    _models_volume_free_bytes: u64,
    _os: &str,
) -> MachineProfile {
    todo!("stage 2")
}

/// Free space on the volume `models_dir` lives on: the volume with the longest mount point that
/// is a parent of it, not the sum of all disks. Zero when no volume contains it.
pub fn models_volume_free_bytes(_models_dir: &Path, _volumes: &[VolumeSpace]) -> u64 {
    todo!("stage 2")
}

#[cfg(test)]
mod tests;
