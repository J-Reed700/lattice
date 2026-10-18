//! Contradiction (§10): when a passage contradicts a supported claim, which
//! side wins. A contrary from a lower level than the support never
//! overturns it; it is recorded as disputed and handed to research.

use serde::{Deserialize, Serialize};

use super::authority::{Trust, TrustLevel};
use super::claim_profile::{ClaimKind, ClaimProfile, TemporalScope};
use super::independence::ClusterId;
use super::reason::{Reason, ReasonCode};
use super::signals::SourceDates;
use super::sufficiency::Support;
use super::temporal::effective_date;
use super::temporal::{day, TemporalValidity};

/// A passage the checker found contradicting the claim, levelled the same
/// way as a support.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// Which side wins when `contrary` contradicts a claim its `supports`
/// support (§10), weighing every contrary passage in `all_contrary` (which
/// includes `contrary`) for the independence step:
///
/// 1. A distrusted side is ignored.
/// 2. When the claim's scope places one side in time and not the other, the
///    placed side wins.
/// 3. A higher level wins; a lower-level contrary never overturns and is
///    recorded as a dispute.
/// 4. For current and since claims (not historical ones), the newer
///    effective date wins.
/// 5. One community or unknown origin loses to two or more independent
///    origins.
/// 6. Otherwise the user decides.
pub fn resolve(
    profile: &ClaimProfile,
    supports: &[Support],
    contrary: &Contrary,
    all_contrary: &[Contrary],
) -> Resolution {
    let id = &contrary.source_version_id;
    if contrary.trust.level == TrustLevel::Distrusted {
        return Resolution::SupportStands {
            note: Some(Reason::new(
                ReasonCode::IgnoredDistrusted,
                format!("{id} contradicts it but is distrusted"),
            )),
        };
    }
    let live: Vec<&Support> = supports
        .iter()
        .filter(|s| s.trust.level != TrustLevel::Distrusted)
        .filter(|s| !matches!(s.temporal, TemporalValidity::Invalid { .. }))
        .collect();
    let Some(best) = live.iter().copied().max_by(|a, b| {
        a.trust.level.cmp(&b.trust.level).then_with(|| {
            effective_date(&a.dates)
                .at
                .cmp(&effective_date(&b.dates).at)
        })
    }) else {
        return Resolution::ContraryWins {
            reasons: vec![Reason::new(
                ReasonCode::IgnoredDistrusted,
                format!("{id} contradicts it and no usable support remains"),
            )],
        };
    };
    let (support_level, contrary_level) = (best.trust.level, contrary.trust.level);
    let subject = profile
        .subject
        .as_ref()
        .map_or("the subject".to_owned(), |subject| subject.name.clone());

    let placing = profile.kind == ClaimKind::VersionBound
        || matches!(
            profile.temporal,
            TemporalScope::Since { .. } | TemporalScope::AsOf { .. }
        );
    if placing {
        let support_placed = live.iter().any(|s| s.temporal == TemporalValidity::Valid);
        let contrary_placed = contrary.temporal == TemporalValidity::Valid;
        if support_placed != contrary_placed {
            let text = if support_placed {
                format!("{id} cannot be placed at the claim's version or date; the support can")
            } else {
                format!("{id} speaks to the claim's version or date; the support cannot")
            };
            let reason = Reason::new(ReasonCode::TemporallyPlaced, text);
            return if support_placed {
                Resolution::SupportStands { note: Some(reason) }
            } else {
                Resolution::ContraryWins {
                    reasons: vec![reason],
                }
            };
        }
    }

    if contrary_level > support_level {
        return Resolution::ContraryWins {
            reasons: vec![Reason::new(
                ReasonCode::OutranksByLevel,
                format!(
                    "{id} is {} for {subject} and outranks the support ({})",
                    contrary_level.as_str(),
                    support_level.as_str()
                ),
            )],
        };
    }
    if contrary_level < support_level {
        return Resolution::SupportStands {
            note: Some(Reason::new(
                ReasonCode::DisputedByLowerStanding,
                format!(
                    "Disputed by {id} ({}), which does not outrank the support ({}); research checks the owner's source at this point",
                    contrary_level.as_str(),
                    support_level.as_str()
                ),
            )),
        };
    }

    let dated = matches!(
        profile.temporal,
        TemporalScope::Current | TemporalScope::Since { .. }
    ) && profile.kind != ClaimKind::Historical;
    if dated {
        let support_at = effective_date(&best.dates).at;
        let contrary_at = effective_date(&contrary.dates).at;
        if let (Some(support_at), Some(contrary_at)) = (support_at, contrary_at) {
            if contrary_at > support_at {
                return Resolution::ContraryWins {
                    reasons: vec![Reason::new(
                        ReasonCode::NewerEvidence,
                        format!(
                            "{id} ({}) is newer than the support ({}), and the claim is about a subject that changes",
                            day(contrary_at),
                            day(support_at)
                        ),
                    )],
                };
            }
            if support_at > contrary_at {
                return Resolution::SupportStands {
                    note: Some(Reason::new(
                        ReasonCode::NewerEvidence,
                        format!(
                            "The support ({}) is newer than {id} ({})",
                            day(support_at),
                            day(contrary_at)
                        ),
                    )),
                };
            }
        }
    }

    let support_clusters = distinct(live.iter().map(|s| &s.origin_cluster));
    let contrary_clusters = distinct(
        all_contrary
            .iter()
            .filter(|c| c.trust.level != TrustLevel::Distrusted)
            .map(|c| &c.origin_cluster),
    );
    let weak = |level: TrustLevel| level <= TrustLevel::Community;
    if support_clusters == 1 && weak(support_level) && contrary_clusters >= 2 {
        return Resolution::ContraryWins {
            reasons: vec![Reason::new(
                ReasonCode::MoreIndependentOrigins,
                format!(
                    "{contrary_clusters} independent origins contradict a single {} origin",
                    support_level.as_str()
                ),
            )],
        };
    }
    if contrary_clusters == 1 && weak(contrary_level) && support_clusters >= 2 {
        return Resolution::SupportStands {
            note: Some(Reason::new(
                ReasonCode::MoreIndependentOrigins,
                format!(
                    "{support_clusters} independent origins support it against a single {} origin",
                    contrary_level.as_str()
                ),
            )),
        };
    }
    Resolution::Escalate {
        reasons: vec![Reason::new(
            ReasonCode::Unresolved,
            format!(
                "{id} and the support are both {} for {subject} and neither is newer or more independent; you decide",
                support_level.as_str()
            ),
        )],
    }
}

/// [`resolve`] for every contrary passage, by its version id.
pub fn resolve_all(
    profile: &ClaimProfile,
    supports: &[Support],
    contrary: &[Contrary],
) -> Vec<(String, Resolution)> {
    contrary
        .iter()
        .map(|c| {
            (
                c.source_version_id.clone(),
                resolve(profile, supports, c, contrary),
            )
        })
        .collect()
}

fn distinct<'a>(clusters: impl Iterator<Item = &'a ClusterId>) -> usize {
    let mut seen: Vec<&ClusterId> = Vec::new();
    for cluster in clusters {
        if !seen.contains(&cluster) {
            seen.push(cluster);
        }
    }
    seen.len()
}

#[cfg(test)]
#[path = "contradiction_tests.rs"]
mod tests;
