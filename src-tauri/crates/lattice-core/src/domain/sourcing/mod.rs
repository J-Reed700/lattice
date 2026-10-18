//! Source finding: routed, fused, budgeted search over many finders
//! (`docs/design/2026-10-18-source-finding-toolbelt.md`).
//!
//! Pure: needs, leads, finder descriptors, outcomes, the policy, and the
//! functions over them (fusion, canonical keys, the breaker). No I/O; the
//! adapters and the runner live in the desktop crate's web feature.

pub mod descriptor;
pub mod health;
pub mod lead;
pub mod need;
pub mod outcome;
pub mod policy;

pub use descriptor::{
    Activation, Auth, Cost, Destination, FinderDescriptor, FinderId, Identity, InstanceSetting,
    KeyPlan, KeyService, Privacy, Quota, RatePolicy, Reach, Role, WaveSlot,
};
pub use health::{Admission, Breaker, BreakerState, FinderHealth, HealthState, QuotaWindow};
pub use lead::{Lead, LeadKind, LinkPlace, Near, OwnerResolution, OwnerSource, Terms};
pub use need::{
    Ecosystem, Freshness, Identifier, NeedKind, Origin, SourceNeed, SubjectHint, SubjectKind,
};
pub use outcome::{FailureCause, FinderOutcome, FinderResult, Findings, SkipReason};
pub use policy::{BreakerTrip, SourcingPolicy};
