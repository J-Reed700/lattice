//! System capabilities detection module.
//!
//! Detects hardware capabilities (RAM, GPU, CPU) to enable intelligent
//! model selection and performance optimization.

pub mod backend_devices;
pub mod capabilities;
pub mod gpu;
pub mod platform;

pub use backend_devices::{cached_backend_devices, detect_backend_devices, BackendDevice};
pub use capabilities::{detect_capabilities, detect_capabilities_with_app, SystemCapabilities};
pub use gpu::{detect_gpu, GPUInfo, GPUVendor};
pub use platform::Platform;
