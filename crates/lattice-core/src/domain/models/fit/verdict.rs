use super::footprint::{Footprint, COMFORTABLE_WINDOW_TOKENS, MINIMUM_WINDOW_TOKENS};
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
///
/// `Fits` and `Tight` report what the model needs at the window they were judged at; `TooLarge`
/// reports the need at the minimum window.
pub fn fit(footprint: &Footprint, placement: &dyn Placement) -> FitVerdict {
    let budget = placement.budget_bytes();
    let device = placement.device_name();
    let total_bytes = placement.device_total_bytes();
    let comfortable = footprint.at(COMFORTABLE_WINDOW_TOKENS);
    let minimum = footprint.at(MINIMUM_WINDOW_TOKENS);
    if comfortable <= budget {
        FitVerdict::Fits {
            needs: comfortable,
            budget,
            device,
            total_bytes,
        }
    } else if minimum <= budget {
        FitVerdict::Tight {
            needs: minimum,
            budget,
            device,
            total_bytes,
        }
    } else {
        FitVerdict::TooLarge {
            needs: minimum,
            budget,
            device,
            total_bytes,
        }
    }
}
