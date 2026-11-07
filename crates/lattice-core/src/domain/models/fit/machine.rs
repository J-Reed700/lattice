/// Which family of device an accelerator is, which decides how much of its memory a model may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceleratorKind {
    /// A discrete card with its own memory.
    Dedicated,
    /// A GPU that draws on system RAM.
    Integrated,
    /// Apple Silicon: one memory pool shared by the processor and the GPU.
    Unified,
}

/// The graphics API llama.cpp drives a device through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Cuda,
    Rocm,
    Vulkan,
    Metal,
    Sycl,
}

/// One device llama-server can offload to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accelerator {
    pub name: String,
    /// Memory the device lists: the card's, or for Metal the working-set size.
    pub total_bytes: u64,
    pub kind: AcceleratorKind,
    pub backend: Backend,
}

/// What a computer offers a model, as a value the probe hands to the recommender.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineProfile {
    pub accelerators: Vec<Accelerator>,
    pub system_ram_bytes: u64,
    /// Free space on the volume the models are stored on.
    pub models_volume_free_bytes: u64,
    pub os: String,
}
