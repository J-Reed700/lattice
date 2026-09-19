//! Shared accelerator selection for local ML inference.

use candle_core::Device;

const FORCE_CPU_ENV: &str = "LATTICE_FORCE_CPU";

/// Prefer the platform accelerator and fall back to CPU without taking down the
/// application when a backend is unavailable or its constructor panics.
pub fn best_available_compute_device(workload: &str) -> Device {
    if force_cpu_requested(std::env::var(FORCE_CPU_ENV).ok().as_deref()) {
        tracing::info!(workload, "Local inference forced to CPU");
        return Device::Cpu;
    }

    #[cfg(target_os = "macos")]
    {
        // Candle's Metal constructor can panic in constrained processes before
        // it has a chance to return an error. Treat that like any unavailable
        // accelerator so model loading remains recoverable.
        match std::panic::catch_unwind(|| Device::new_metal(0)) {
            Ok(Ok(device)) => return device,
            Ok(Err(error)) => {
                tracing::warn!(workload, error = %error, "Metal unavailable; using CPU fallback")
            }
            Err(_) => tracing::warn!(
                workload,
                "Metal initialization panicked; using CPU fallback"
            ),
        }
    }

    Device::cuda_if_available(0).unwrap_or_else(|error| {
        tracing::debug!(workload, error = %error, "CUDA unavailable; using CPU fallback");
        Device::Cpu
    })
}

/// Whether local inference will actually get an accelerator on this machine.
///
/// Asks the same question [`best_available_compute_device`] answers, because
/// choosing a default model on "the machine has a GPU" rather than "this build
/// can use it" would hand a CPU-only install a model it cannot run at a usable
/// speed.
pub fn gpu_acceleration_available() -> bool {
    !best_available_compute_device("accelerator-probe").is_cpu()
}

fn force_cpu_requested(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::force_cpu_requested;

    #[test]
    fn force_cpu_values_are_explicit() {
        for value in ["1", "true", "TRUE", " yes ", "on"] {
            assert!(force_cpu_requested(Some(value)));
        }
        for value in ["", "0", "false", "metal", "cpu"] {
            assert!(!force_cpu_requested(Some(value)));
        }
        assert!(!force_cpu_requested(None));
    }
}
