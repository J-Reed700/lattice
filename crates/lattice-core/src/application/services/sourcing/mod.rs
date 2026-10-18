//! Source-finding services that are pure policy: routing a need into waves
//! and deciding what the caches key on and when they may answer. The runner
//! that executes a plan lives in the desktop crate's web feature.

pub mod cache_policy;
pub mod routing;

pub use cache_policy::SourcingCache;
pub use routing::{plan, routed_kind, FinderGate, Plan, Step, StepScope, Wave};
