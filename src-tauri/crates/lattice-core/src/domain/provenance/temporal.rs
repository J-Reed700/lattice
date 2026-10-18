//! Temporal validity (§9): whether a source's date can place the claim's
//! temporal scope. Staleness is relative to the newest primary evidence the
//! course holds for the subject; no absolute age cut-off exists.

use serde::{Deserialize, Serialize};

use super::reason::Reason;

/// Whether a support can speak to the claim's scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "validity", rename_all = "snake_case")]
pub enum TemporalValidity {
    Valid,
    /// Older than the newest primary for the subject: counts, and fetching
    /// the newer primary is the first next step.
    Stale {
        newer_primary: String,
    },
    /// The source neither names the version nor is dated after it.
    CannotPlace {
        reason: String,
    },
    /// No date at all, for a scope that needs one.
    Undated {
        reason: String,
    },
    /// Dated before the claim's as-of or since date.
    Invalid {
        reason: String,
    },
}

/// The date a source is taken to speak from (§9), with its basis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveDate {
    /// Unix seconds; `None` when the source is undated.
    #[serde(default)]
    pub at: Option<i64>,
    pub reasons: Vec<Reason>,
}
