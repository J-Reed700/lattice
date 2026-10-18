//! Temporal validity (§9): whether a source's date can place the claim's
//! temporal scope. Staleness is relative to the newest primary evidence the
//! course holds for the subject; no absolute age cut-off exists.

use chrono::DateTime;
use serde::{Deserialize, Serialize};

use super::claim_profile::TemporalScope;
use super::reason::{Reason, ReasonCode};
use super::signals::SourceDates;

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

/// The date a source speaks from (§9): the latest of its published and
/// modified dates that is not after it was acquired (a future date is
/// ignored, with the reason); else the server's `Last-Modified` (weak, said
/// so); else its archive capture time (an upper bound: the content existed
/// by then); else undated.
pub fn effective_date(dates: &SourceDates) -> EffectiveDate {
    let mut reasons = Vec::new();
    let mut stated = Vec::new();
    for (name, date) in [("published", dates.published), ("modified", dates.modified)] {
        let Some(date) = date else { continue };
        if date > dates.acquired_at {
            reasons.push(Reason::new(
                ReasonCode::FutureDateIgnored,
                format!(
                    "Its {name} date {} is after it was read; ignored",
                    day(date)
                ),
            ));
        } else {
            stated.push(date);
        }
    }
    if let Some(at) = stated.into_iter().max() {
        return EffectiveDate {
            at: Some(at),
            reasons,
        };
    }
    let weak = [
        (
            dates.http_last_modified,
            "Dated only by the server's Last-Modified header",
        ),
        (
            dates.archived_at,
            "Dated by its archive capture: the content existed by then",
        ),
    ];
    for (date, text) in weak {
        if let Some(date) = date.filter(|date| *date <= dates.acquired_at) {
            reasons.push(Reason::new(ReasonCode::WeakDateBasis, text));
            return EffectiveDate {
                at: Some(date),
                reasons,
            };
        }
    }
    EffectiveDate { at: None, reasons }
}

/// Whether a source dated `dates` can speak to `scope` (§9).
/// `mentions_version` says whether the source names the claim's version or
/// a later one in the same series; `newest_primary` is the newest Primary
/// support for the subject in the course, by version id and effective date.
pub fn validity(
    scope: &TemporalScope,
    dates: &SourceDates,
    mentions_version: bool,
    newest_primary: Option<(&str, i64)>,
) -> TemporalValidity {
    let effective = effective_date(dates).at;
    match scope {
        TemporalScope::Timeless => TemporalValidity::Valid,
        TemporalScope::AsOf { date } => match effective {
            None => TemporalValidity::Undated {
                reason: format!("Undated, and the claim is as of {}", day(*date)),
            },
            Some(at) if at >= *date => TemporalValidity::Valid,
            Some(at) => TemporalValidity::Invalid {
                reason: format!(
                    "Dated {}, before the claim's as-of date {}",
                    day(at),
                    day(*date)
                ),
            },
        },
        TemporalScope::Since { version, date } => {
            if version.is_some() && mentions_version {
                return TemporalValidity::Valid;
            }
            let Some(at) = effective else {
                return match (version, date) {
                    (None, None) => TemporalValidity::Valid,
                    (Some(version), _) => TemporalValidity::CannotPlace {
                        reason: format!("It does not name version {version} and is undated"),
                    },
                    (None, Some(date)) => TemporalValidity::CannotPlace {
                        reason: format!("Undated, and the claim holds since {}", day(*date)),
                    },
                };
            };
            if let Some(date) = date {
                return if at >= *date {
                    TemporalValidity::Valid
                } else {
                    TemporalValidity::Invalid {
                        reason: format!(
                            "Dated {}, before the claim's since-date {}",
                            day(at),
                            day(*date)
                        ),
                    }
                };
            }
            match (version, newest_primary) {
                (None, _) => TemporalValidity::Valid,
                (Some(_), Some((_, newest))) if at >= newest => TemporalValidity::Valid,
                (Some(version), _) => TemporalValidity::CannotPlace {
                    reason: format!(
                        "It does not name version {version} and is not dated after the newest primary source"
                    ),
                },
            }
        }
        TemporalScope::Current => match (effective, newest_primary) {
            (None, _) => TemporalValidity::Undated {
                reason: "Undated, and the claim is about the current state".into(),
            },
            (Some(at), Some((newer, newest))) if at < newest => TemporalValidity::Stale {
                newer_primary: newer.to_owned(),
            },
            (Some(_), _) => TemporalValidity::Valid,
        },
    }
}

/// `YYYY-MM-DD` for unix seconds.
pub(super) fn day(at: i64) -> String {
    DateTime::from_timestamp(at, 0)
        .map(|date| date.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| at.to_string())
}

#[cfg(test)]
#[path = "temporal_tests.rs"]
mod tests;
