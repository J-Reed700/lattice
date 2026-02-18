//! System capabilities detection module.
//!
//! Detects hardware capabilities (RAM, GPU, CPU) to enable intelligent
//! model selection and performance optimization.

pub mod capabilities;
pub mod gpu;
pub mod platform;

pub use capabilities::{detect_capabilities, SystemCapabilities};
pub use gpu::{detect_gpu, GPUInfo, GPUVendor};
pub use platform::Platform;
