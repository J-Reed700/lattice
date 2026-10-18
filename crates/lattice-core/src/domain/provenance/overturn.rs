//! The overturn rule (§11): the only way a supported verdict is taken away.
//! A supported claim is overturned only by contrary evidence that outranks
//! its support, or when its support fails the bar for its kind.

use serde::{Deserialize, Serialize};

use super::contradiction::Resolution;
use super::reason::Reason;
use super::sufficiency::{Bar, CountedSupport, Decision};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "overturn", rename_all = "snake_case")]
pub enum Overturn {
    None,
    /// A contrary passage won (§10): the claim is contradicted.
    Contradicted {
        by: String,
        reasons: Vec<Reason>,
    },
    /// Neither side outranks the other: the claim waits for the user with
    /// both sides shown.
    Escalated {
        by: String,
        reasons: Vec<Reason>,
    },
    /// The supports fail the bar for the claim's kind.
    InsufficientStanding {
        bar: Bar,
        have: Vec<CountedSupport>,
        reasons: Vec<Reason>,
    },
}

/// The one rule that may take a supported verdict away (§11): contrary
/// evidence that wins (§10), then a contradiction nobody outranks (for the
/// user), then supports that fail the bar for the claim's kind (§6).
/// A dispute by a lower-standing source never overturns.
pub fn overturn(decision: &Decision, resolutions: &[(String, Resolution)]) -> Overturn {
    if let Some((by, reasons)) = resolutions
        .iter()
        .find_map(|(by, resolution)| match resolution {
            Resolution::ContraryWins { reasons } => Some((by, reasons)),
            _ => None,
        })
    {
        return Overturn::Contradicted {
            by: by.clone(),
            reasons: reasons.clone(),
        };
    }
    if let Some((by, reasons)) = resolutions
        .iter()
        .find_map(|(by, resolution)| match resolution {
            Resolution::Escalate { reasons } => Some((by, reasons)),
            _ => None,
        })
    {
        return Overturn::Escalated {
            by: by.clone(),
            reasons: reasons.clone(),
        };
    }
    match decision {
        Decision::Insufficient {
            bar, have, reasons, ..
        } => Overturn::InsufficientStanding {
            bar: *bar,
            have: have.clone(),
            reasons: reasons.clone(),
        },
        Decision::Sufficient { .. } => Overturn::None,
    }
}
