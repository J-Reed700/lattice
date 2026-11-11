//! Reusable filesystem primitives and path handling.

pub mod atomic;
pub mod confinement;
pub mod path;
pub mod path_safety;
pub mod removal;
pub mod roots;
pub mod sweep_gate;

pub use atomic::AtomicFs;
pub use path::{path_to_string, validate_path};
pub use sweep_gate::SweepGate;
