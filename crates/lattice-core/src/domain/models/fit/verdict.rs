use super::footprint::ServerBuffers;
use super::placement::Placement;
use super::shape::ModelShape;

/// How well a model fits a placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitKind {
    /// Fits at the comfortable window (32,768 tokens).
    Fits,
    /// Fits only at the minimum window (8,192 tokens): it runs, with less context.
    Tight,
    TooLarge,
}

/// What kind of device a model is judged against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Dedicated,
    /// Apple Silicon's shared memory.
    Unified,
    /// System memory: no usable accelerator, or an integrated GPU, which draws on it. Slow.
    Processor,
}

/// The device a verdict was judged against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FitDevice {
    pub name: String,
    pub kind: DeviceKind,
    /// The device's total memory, which the budget is a share of.
    pub total_bytes: u64,
}

/// Whether a model fits a placement, what it needs and what it was judged against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FitVerdict {
    pub kind: FitKind,
    /// At the window the verdict was judged at: the comfortable one for `Fits`, the minimum for
    /// `Tight` and `TooLarge`.
    pub needs: u64,
    pub budget: u64,
    pub device: FitDevice,
}

/// The one rule: does a model holding `gpu_weight_bytes`, with `shape`, fit `placement`, priced by
/// `footprint` with `buffers`.
pub fn fit(
    _gpu_weight_bytes: u64,
    _shape: Option<&ModelShape>,
    _buffers: ServerBuffers,
    _placement: &dyn Placement,
) -> FitVerdict {
    todo!("stage 2")
}
