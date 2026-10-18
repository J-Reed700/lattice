//! Contradiction (§10): when a passage contradicts a supported claim, which
//! side wins. A contrary from a lower level than the support never
//! overturns it; it is recorded as disputed and handed to research.

use serde::{Deserialize, Serialize};

use super::authority::Trust;
use super::independence::ClusterId;
use super::reason::Reason;
use super::signals::SourceDates;
use super::temporal::TemporalValidity;

/// A passage the checker found contradicting the claim, levelled the same
/// way as a support.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contrary {
    pub source_version_id: String,
    pub origin_cluster: ClusterId,
    pub trust: Trust,
    pub dates: SourceDates,
    pub temporal: TemporalValidity,
    pub quote: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "resolution", rename_all = "snake_case")]
pub enum Resolution {
    /// The support stands; `note` records a dispute by a lower-standing
    /// source, shown and handed to research.
    SupportStands {
        #[serde(default)]
        note: Option<Reason>,
    },
    ContraryWins {
        reasons: Vec<Reason>,
    },
    /// Neither side outranks the other: the user decides, with both shown.
    Escalate {
        reasons: Vec<Reason>,
    },
}
