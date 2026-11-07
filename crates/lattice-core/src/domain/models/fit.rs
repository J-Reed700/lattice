//! One authority on whether a chat model fits a machine, and the recommended list built from it.
//!
//! Pure: no I/O. A probe in the desktop crate builds the [`MachineProfile`]; every verdict, every
//! label and the first-run list come from here.

mod companions;
mod footprint;
mod machine;
mod placement;
mod recommender;
mod shape;
mod verdict;

pub use companions::{CompanionServer, Companions};
pub use footprint::{
    footprint, Footprint, ServerBuffers, COMFORTABLE_WINDOW_TOKENS, FIT_TARGET_BYTES,
    MINIMUM_WINDOW_TOKENS,
};
pub use machine::{Accelerator, AcceleratorKind, Backend, MachineProfile};
pub use placement::{
    choose_placement, DedicatedGpu, Placement, ProcessorOnly, UnifiedMemory, DISPLAY_RESERVE_BYTES,
};
pub use recommender::{Candidate, Evidence, Label, MeasuredSuite, Recommendation, Recommender};
pub use shape::{ModelShape, PerLayer, PinnedModel, MICRO_BATCH_TOKENS};
pub use verdict::{fit, DeviceKind, FitDevice, FitKind, FitVerdict};

#[cfg(test)]
mod tests;
