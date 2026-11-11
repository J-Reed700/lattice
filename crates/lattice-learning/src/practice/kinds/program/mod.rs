//! The Program kind: the learner writes code, and running it is the truth.

pub mod checker;
pub mod diff;
pub mod parts;
pub mod shape;
pub mod toolchain;
pub mod truth;

/// The id of the Program kind.
pub const KIND: &str = "program";
