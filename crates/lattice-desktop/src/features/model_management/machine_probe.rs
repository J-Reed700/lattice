//! The real machine profile probe: the devices the runtime already probed, the system's RAM and the
//! models volume's free space.

use super::recommend::MachineProfileProbe;
use crate::features::llm::engine::system::machine_profile::{
    machine_profile, models_volume_free_bytes, VolumeSpace,
};
use crate::features::llm::engine::system::{BackendDevice, BackendDevices};
use async_trait::async_trait;
use lattice_core::domain::models::fit::MachineProfile;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Builds the profile from the app's one cached `--list-devices` answer. A probe that finds no
/// accelerator, or cannot run, answers with a processor-only profile and says why in the log.
pub struct SidecarMachineProfileProbe {
    devices: Arc<BackendDevices>,
    app: Option<tauri::AppHandle>,
    models_dir: PathBuf,
}

impl SidecarMachineProfileProbe {
    pub fn new(
        devices: Arc<BackendDevices>,
        app: Option<tauri::AppHandle>,
        models_dir: PathBuf,
    ) -> Self {
        Self {
            devices,
            app,
            models_dir,
        }
    }

    async fn listed_devices(&self) -> Vec<BackendDevice> {
        match &self.app {
            Some(app) => self.devices.detect(app).await.to_vec(),
            None => {
                let cached = self.devices.cached().map(<[BackendDevice]>::to_vec);
                if cached.is_none() {
                    tracing::warn!("machine profile asked before the device probe ran");
                }
                cached.unwrap_or_default()
            }
        }
    }
}

#[async_trait]
impl MachineProfileProbe for SidecarMachineProfileProbe {
    async fn profile(&self) -> MachineProfile {
        let devices = self.listed_devices().await;
        if !devices.iter().any(BackendDevice::is_accelerator) {
            tracing::warn!(
                listed = devices.len(),
                "no usable accelerator was listed; recommending for the processor"
            );
        }
        let models_dir = self.models_dir.clone();
        let (system_ram_bytes, free_bytes) =
            tokio::task::spawn_blocking(move || host_memory_and_disk(&models_dir))
                .await
                .unwrap_or_else(|error| {
                    tracing::warn!(%error, "host facts could not be read");
                    (0, 0)
                });
        machine_profile(&devices, system_ram_bytes, free_bytes, std::env::consts::OS)
    }
}

fn host_memory_and_disk(models_dir: &Path) -> (u64, u64) {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    let volumes: Vec<VolumeSpace> = sysinfo::Disks::new_with_refreshed_list()
        .iter()
        .map(|disk| VolumeSpace {
            mount_point: disk.mount_point().to_path_buf(),
            free_bytes: disk.available_space(),
        })
        .collect();
    (
        system.total_memory(),
        models_volume_free_bytes(models_dir, &volumes),
    )
}
