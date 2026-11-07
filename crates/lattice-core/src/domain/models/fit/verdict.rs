use super::footprint::ServerBuffers;
use super::placement::Placement;
use super::shape::ModelShape;

/// Whether a model fits a placement. Each carries the device it was judged against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FitVerdict {
    /// Fits at the comfortable window (32,768 tokens).
    Fits {
        needs: u64,
        budget: u64,
        device: String,
        /// The device's total memory, which the budget is a share of.
        total_bytes: u64,
    },
    /// Fits only at the minimum window (8,192 tokens): it runs, with less context.
    Tight {
        needs: u64,
        budget: u64,
        device: String,
        total_bytes: u64,
    },
    TooLarge {
        needs: u64,
        budget: u64,
        device: String,
        total_bytes: u64,
    },
}

/// The one rule: does a model holding `gpu_weight_bytes`, with `shape`, fit `placement`, priced by
/// `footprint` with `buffers`. `Fits` and `Tight` report the need at the window they were judged
/// at; `TooLarge` the need at the minimum window.
pub fn fit(
    _gpu_weight_bytes: u64,
    _shape: Option<&ModelShape>,
    _buffers: ServerBuffers,
    _placement: &dyn Placement,
) -> FitVerdict {
    todo!("stage 2")
}
