use super::footprint::Footprint;
use super::placement::Placement;

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
        /// The device's total memory, which the budget is a share of.
        total_bytes: u64,
    },
    TooLarge {
        needs: u64,
        budget: u64,
        device: String,
        /// The device's total memory, which the budget is a share of.
        total_bytes: u64,
    },
}

/// The one rule: does `footprint` fit `placement`.
pub fn fit(_footprint: &Footprint, _placement: &dyn Placement) -> FitVerdict {
    todo!("stage 2")
}
