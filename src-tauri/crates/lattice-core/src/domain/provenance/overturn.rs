//! The overturn rule (§11): the only way a supported verdict is taken away.
//! A supported claim is overturned only by contrary evidence that outranks
//! its support, or when its support fails the bar for its kind.

use serde::{Deserialize, Serialize};

use super::reason::Reason;
use super::sufficiency::{Bar, CountedSupport};

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
