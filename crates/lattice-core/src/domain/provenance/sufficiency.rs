//! Sufficiency (§6): whether a claim's independent supports meet the bar
//! for its kind. A pure function over cached judgements, so it costs
//! nothing to recompute when an override or a relation changes.

use serde::{Deserialize, Serialize};

use super::authority::{Trust, TrustLevel};
use super::calibration::EffectiveBars;
use super::claim_profile::{ClaimKind, ClaimProfile, SubjectKey, TemporalScope};
use super::contradiction::Contrary;
use super::independence::ClusterId;
use super::reason::{AddedBy, Reason, ReasonCode};
use super::signals::SourceDates;
use super::temporal::TemporalValidity;

/// How a passage bears on the claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "passage", rename_all = "snake_case")]
pub enum PassageKind {
    /// The source states the fact itself.
    Direct,
    /// The source quotes another source.
    Quotation {
        #[serde(default)]
        of: Option<String>,
    },
    /// The source reports what someone else says ("X reports that…").
    /// `borne_out` is set once the chain to X was followed (§8).
    Testimonial {
        attributed_to: String,
        #[serde(default)]
        borne_out: Option<bool>,
    },
}

/// One passage the checker used as support, levelled for the claim's
/// subject.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Support {
    pub source_version_id: String,
    pub origin_cluster: ClusterId,
    pub trust: Trust,
    pub dates: SourceDates,
    pub passage: PassageKind,
    pub temporal: TemporalValidity,
    pub added_by: AddedBy,
    /// Citation candidates in the passage, best first (`citation_candidates`):
    /// what [`NextStep::FollowCitation`] offers.
    #[serde(default)]
    pub cited_urls: Vec<String>,
    /// Owner hints from this support's ownership judgement: what
    /// [`NextStep::SearchOwner`] offers.
    #[serde(default)]
    pub owner_hints: Vec<String>,
}

/// The rule a claim's supports must meet, named for the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Bar {
    /// The subject's owner (or an authoritative, uncontradicted source).
    Primary,
    /// The measurement's origin.
    Origin,
    Authoritative,
    Established,
    /// An identified source the claim can be attributed to.
    Attributed,
    /// Any identified support.
    Any,
}

/// How an accepted claim must be worded or shown when its support is below
/// Primary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "label", rename_all = "snake_case")]
pub enum Label {
    Practice,
    Attributed,
    CommunityConsensus,
    ReportedBy { attributing_to: String },
    Secondary,
    Mirror,
}

/// A support as the gate counted it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CountedSupport {
    pub source_version_id: String,
    pub origin_cluster: ClusterId,
    /// The level it counted at after the passage, date and exclusion rules.
    /// `None` when it was excluded.
    #[serde(default)]
    pub counted: Option<TrustLevel>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum Decision {
    Sufficient {
        /// The supports that met the bar, best first.
        met_by: Vec<String>,
        #[serde(default)]
        label: Option<Label>,
        reasons: Vec<Reason>,
    },
    Insufficient {
        bar: Bar,
        have: Vec<CountedSupport>,
        missing: Missing,
        next: Vec<NextStep>,
        reasons: Vec<Reason>,
    },
}

/// What the supports lack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "missing", rename_all = "snake_case")]
pub enum Missing {
    NoPrimaryForSubject,
    NoMeasurementOrigin,
    NoIndependentCorroboration {
        have: usize,
    },
    NoIdentifiedSource,
    /// Every support that would meet the bar only reports someone else.
    OnlyTestimonial,
    OnlyStale {
        newer_primary: String,
    },
}

/// What research may do next, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum NextStep {
    FollowCitation { url: String, from: String },
    SearchOwner { origin_hints: Vec<String> },
    SearchIndependent,
    FetchNewer { newer_primary: String },
    AskUser,
}

/// The §6.1 table's "accepted alone" column: per claim kind, the lowest
/// level accepted as the only support. The prior calibration moves (§13).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticBars {
    pub entries: Vec<(ClaimKind, TrustLevel)>,
}

/// Independent origins a corroboration rule asks for (§6.1, "two
/// independent …"): the smallest number that is corroboration at all.
const CORROBORATING_ORIGINS: usize = 2;

impl StaticBars {
    /// The "accepted alone" column of §6.1: the lowest level accepted as a
    /// claim's only support, per kind.
    pub fn table() -> Self {
        Self {
            entries: vec![
                (ClaimKind::Specification, TrustLevel::Authoritative),
                (ClaimKind::Measurement, TrustLevel::Primary),
                (ClaimKind::VersionBound, TrustLevel::Authoritative),
                (ClaimKind::Definition, TrustLevel::Established),
                (ClaimKind::Practice, TrustLevel::Established),
                (ClaimKind::Opinion, TrustLevel::Established),
                (ClaimKind::Historical, TrustLevel::Authoritative),
                (ClaimKind::General, TrustLevel::Established),
            ],
        }
    }

    pub fn lowest(&self, kind: ClaimKind) -> Option<TrustLevel> {
        self.entries
            .iter()
            .find(|(entry, _)| *entry == kind)
            .map(|(_, level)| *level)
    }
}

impl Bar {
    /// The bar a kind of claim is held to, as the user is told it.
    pub fn for_kind(kind: ClaimKind) -> Self {
        match kind {
            ClaimKind::Specification | ClaimKind::VersionBound => Self::Primary,
            ClaimKind::Measurement => Self::Origin,
            ClaimKind::Definition | ClaimKind::Practice => Self::Established,
            ClaimKind::Opinion => Self::Attributed,
            ClaimKind::Historical => Self::Authoritative,
            ClaimKind::General => Self::Any,
        }
    }
}

/// A support after the exclusion, date and passage rules.
struct Counted<'a> {
    support: &'a Support,
    /// `None` when excluded.
    level: Option<TrustLevel>,
    /// The page's level before the passage rule capped it.
    page: TrustLevel,
    reported: bool,
}

/// Whether the claim's supports meet the bar for its kind (§6.1). Pure: the
/// levels, dates and passage kinds are already judged, so this costs
/// nothing to recompute when an override or a relation changes.
///
/// Order: exclusions (Distrusted, a date that rules the source out); the
/// user's own evidence; one support at or above the kind's effective bar
/// (calibrated, §13); independent corroboration; otherwise what is missing
/// and what research may do next.
pub fn decide(
    profile: &ClaimProfile,
    supports: &[Support],
    contrary: &[Contrary],
    bars: &EffectiveBars,
) -> Decision {
    let kind = profile.kind;
    let mut reasons = Vec::new();
    let counted: Vec<Counted<'_>> = supports
        .iter()
        .map(|support| count(profile, support, &mut reasons))
        .collect();
    let usable: Vec<&Counted<'_>> = counted.iter().filter(|c| c.level.is_some()).collect();

    let user: Vec<String> = usable
        .iter()
        .filter(|c| c.support.added_by == AddedBy::User)
        .map(|c| c.support.source_version_id.clone())
        .collect();
    if !user.is_empty() {
        reasons.push(Reason::new(
            ReasonCode::AddedByYou,
            "Supported by material you added; its standing is shown, not weighed",
        ));
        return Decision::Sufficient {
            met_by: user,
            label: None,
            reasons,
        };
    }

    // A contrary the claim's scope rules out (misdated, or unplaceable for
    // a version claim) blocks nothing, as it would lose in `resolve`.
    let needs_placing =
        kind == ClaimKind::VersionBound || matches!(profile.temporal, TemporalScope::Since { .. });
    let contrary_levels: Vec<TrustLevel> = contrary
        .iter()
        .filter(|c| c.trust.level != TrustLevel::Distrusted)
        .filter(|c| match c.temporal {
            super::temporal::TemporalValidity::Invalid { .. } => false,
            super::temporal::TemporalValidity::CannotPlace { .. } => !needs_placing,
            _ => true,
        })
        .map(|c| c.trust.level)
        .collect();
    let lowest = bars.lowest_accepted(kind);
    if let Some(lowest) = lowest {
        if let Some(decision) = alone(
            profile,
            &usable,
            lowest,
            &contrary_levels,
            bars,
            &mut reasons,
        ) {
            return decision;
        }
    }
    if let Some(decision) = corroborated(profile, &usable, &contrary_levels, &mut reasons) {
        return decision;
    }
    insufficient(profile, &counted, lowest, bars, reasons)
}

fn count<'a>(
    profile: &ClaimProfile,
    support: &'a Support,
    reasons: &mut Vec<Reason>,
) -> Counted<'a> {
    use super::temporal::TemporalValidity as T;
    let page = support.trust.level;
    let id = &support.source_version_id;
    let excluded = |reasons: &mut Vec<Reason>, code, text: String| {
        reasons.push(Reason::new(code, text));
        Counted {
            support,
            level: None,
            page,
            reported: false,
        }
    };
    if page == TrustLevel::Distrusted {
        return excluded(
            reasons,
            ReasonCode::ExcludedDistrusted,
            format!("{id} is distrusted and not counted"),
        );
    }
    let needs_placing = profile.kind == ClaimKind::VersionBound
        || matches!(profile.temporal, TemporalScope::Since { .. });
    let mut level = page;
    match &support.temporal {
        T::Invalid { reason } => {
            return excluded(
                reasons,
                ReasonCode::ExcludedInvalidDate,
                format!("{id}: {reason}"),
            );
        }
        T::CannotPlace { reason } if needs_placing => {
            return excluded(
                reasons,
                ReasonCode::ExcludedCannotPlace,
                format!("{id}: {reason}"),
            );
        }
        T::Undated { reason }
            if profile.kind == ClaimKind::VersionBound
                || matches!(profile.temporal, TemporalScope::Current) =>
        {
            level = one_lower(level);
            reasons.push(Reason::new(
                ReasonCode::UndatedCountedLower,
                format!("{id} counts one level lower ({}): {reason}", level.as_str()),
            ));
        }
        T::Stale { newer_primary } => reasons.push(Reason::new(
            ReasonCode::StaleSupport,
            format!("{id} is older than the newest primary source ({newer_primary})"),
        )),
        _ => {}
    }
    let reported = !matches!(support.passage, PassageKind::Direct);
    if reported && level > TrustLevel::Community {
        level = TrustLevel::Community;
        let whose = match &support.passage {
            PassageKind::Testimonial { attributed_to, .. } => {
                format!("reports what {attributed_to} says")
            }
            PassageKind::Quotation { of: Some(of) } => format!("quotes {of}"),
            _ => "quotes another source".to_owned(),
        };
        reasons.push(Reason::new(
            ReasonCode::TestimonialCounted,
            format!("{id} {whose}, so it counts as community until that source is checked"),
        ));
    }
    Counted {
        support,
        level: Some(level),
        page,
        reported,
    }
}

/// One support at or above the effective bar.
fn alone(
    profile: &ClaimProfile,
    usable: &[&Counted<'_>],
    lowest: TrustLevel,
    contrary_levels: &[TrustLevel],
    bars: &EffectiveBars,
    reasons: &mut Vec<Reason>,
) -> Option<Decision> {
    let primary_contradicts = contrary_levels.contains(&TrustLevel::Primary);
    let spec_like = matches!(
        profile.kind,
        ClaimKind::Specification | ClaimKind::VersionBound
    );
    let mut meeting: Vec<&&Counted<'_>> = usable
        .iter()
        .filter(|c| c.level.is_some_and(|level| level >= lowest))
        .filter(|c| !(spec_like && primary_contradicts && c.level < Some(TrustLevel::Primary)))
        .collect();
    if meeting.is_empty() {
        if spec_like && primary_contradicts && usable.iter().any(|c| c.level >= Some(lowest)) {
            reasons.push(Reason::new(
                ReasonCode::PrimaryContradicts,
                "A primary source contradicts it, so no secondary source meets the bar alone",
            ));
        }
        return None;
    }
    meeting.sort_by(|a, b| {
        b.level.cmp(&a.level).then_with(|| {
            a.support
                .source_version_id
                .cmp(&b.support.source_version_id)
        })
    });
    let best = meeting.first()?;
    let best_level = best.level?;
    let label = if best_level == TrustLevel::Primary {
        best.support
            .trust
            .reasons
            .iter()
            .any(|reason| reason.code == ReasonCode::Mirror)
            .then_some(Label::Mirror)
    } else {
        match profile.kind {
            ClaimKind::Specification | ClaimKind::VersionBound => Some(Label::Secondary),
            ClaimKind::Measurement => Some(profile.measurement_origin.clone().map_or(
                Label::Secondary,
                |origin| Label::ReportedBy {
                    attributing_to: origin,
                },
            )),
            ClaimKind::Practice => Some(Label::Practice),
            ClaimKind::Opinion => Some(Label::Attributed),
            ClaimKind::Definition | ClaimKind::Historical | ClaimKind::General => None,
        }
    };
    reasons.push(Reason::new(
        ReasonCode::MetAlone,
        format!(
            "{} ({}) meets the {} bar alone",
            best.support.source_version_id,
            best_level.as_str(),
            kind_name(profile.kind)
        ),
    ));
    reasons.extend(bars.moved_reason(profile.kind));
    Some(Decision::Sufficient {
        met_by: meeting
            .iter()
            .map(|c| c.support.source_version_id.clone())
            .collect(),
        label,
        reasons: std::mem::take(reasons),
    })
}

/// Independent origins that together meet the kind's corroboration rule.
fn corroborated(
    profile: &ClaimProfile,
    usable: &[&Counted<'_>],
    contrary_levels: &[TrustLevel],
    reasons: &mut Vec<Reason>,
) -> Option<Decision> {
    let (floor, label) = match profile.kind {
        ClaimKind::Specification | ClaimKind::VersionBound => {
            (TrustLevel::Established, Some(Label::Secondary))
        }
        ClaimKind::Historical => (TrustLevel::Established, None),
        ClaimKind::Definition | ClaimKind::Practice => {
            (TrustLevel::Community, Some(Label::CommunityConsensus))
        }
        ClaimKind::Measurement => {
            return reported_measurement(profile, usable, contrary_levels, reasons)
        }
        ClaimKind::Opinion | ClaimKind::General => return None,
    };
    let best = best_per_cluster(usable.iter().filter_map(|c| Some((c, c.level?))));
    let qualifying: Vec<(&&Counted<'_>, TrustLevel)> = best
        .into_iter()
        .filter(|(_, level)| *level >= floor)
        .collect();
    finish_corroboration(profile, qualifying, label, contrary_levels, reasons)
}

/// The reports attributing a measurement to one origin: its key, its name
/// as first written, and each report with its page level.
type ReportsOf<'a, 'b> = (SubjectKey, String, Vec<(&'b &'b Counted<'a>, TrustLevel)>);

/// Measurement corroboration: independent reports, each at Established or
/// above as a page, attributing the figure to the same origin.
fn reported_measurement(
    profile: &ClaimProfile,
    usable: &[&Counted<'_>],
    contrary_levels: &[TrustLevel],
    reasons: &mut Vec<Reason>,
) -> Option<Decision> {
    let wanted = profile.measurement_origin.as_deref().map(SubjectKey::of);
    let mut by_origin: Vec<ReportsOf<'_, '_>> = Vec::new();
    for counted in usable {
        let PassageKind::Testimonial { attributed_to, .. } = &counted.support.passage else {
            continue;
        };
        let key = SubjectKey::of(attributed_to);
        if counted.page < TrustLevel::Established
            || wanted.as_ref().is_some_and(|wanted| *wanted != key)
        {
            continue;
        }
        match by_origin.iter_mut().find(|(seen, _, _)| *seen == key) {
            Some((_, _, members)) => members.push((counted, counted.page)),
            None => by_origin.push((key, attributed_to.clone(), vec![(counted, counted.page)])),
        }
    }
    by_origin.into_iter().find_map(|(_, name, members)| {
        let qualifying = best_per_cluster(members.into_iter());
        finish_corroboration(
            profile,
            qualifying,
            Some(Label::ReportedBy {
                attributing_to: name,
            }),
            contrary_levels,
            reasons,
        )
    })
}

fn finish_corroboration(
    profile: &ClaimProfile,
    qualifying: Vec<(&&Counted<'_>, TrustLevel)>,
    label: Option<Label>,
    contrary_levels: &[TrustLevel],
    reasons: &mut Vec<Reason>,
) -> Option<Decision> {
    if qualifying.len() < CORROBORATING_ORIGINS {
        return None;
    }
    let weakest = qualifying.iter().map(|(_, level)| *level).min()?;
    if contrary_levels.iter().any(|level| *level >= weakest) {
        reasons.push(Reason::new(
            ReasonCode::ContradictionBlocksCorroboration,
            format!(
                "A source at {} or above contradicts it, so corroboration does not settle it",
                weakest.as_str()
            ),
        ));
        return None;
    }
    reasons.push(Reason::new(
        ReasonCode::MetByCorroboration,
        format!(
            "{} independent origins support the {} claim",
            qualifying.len(),
            kind_name(profile.kind)
        ),
    ));
    Some(Decision::Sufficient {
        met_by: qualifying
            .iter()
            .map(|(c, _)| c.support.source_version_id.clone())
            .collect(),
        label,
        reasons: std::mem::take(reasons),
    })
}

/// The best-levelled member of each origin cluster, clusters in id order.
fn best_per_cluster<'a, 'b>(
    members: impl Iterator<Item = (&'b &'b Counted<'a>, TrustLevel)>,
) -> Vec<(&'b &'b Counted<'a>, TrustLevel)> {
    let mut best: Vec<(&&Counted<'_>, TrustLevel)> = Vec::new();
    for (counted, level) in members {
        let cluster = &counted.support.origin_cluster;
        match best
            .iter_mut()
            .find(|(seen, _)| seen.support.origin_cluster == *cluster)
        {
            Some(slot) if slot.1 < level => *slot = (counted, level),
            Some(_) => {}
            None => best.push((counted, level)),
        }
    }
    best.sort_by(|a, b| a.0.support.origin_cluster.cmp(&b.0.support.origin_cluster));
    best
}

fn insufficient(
    profile: &ClaimProfile,
    counted: &[Counted<'_>],
    lowest: Option<TrustLevel>,
    bars: &EffectiveBars,
    mut reasons: Vec<Reason>,
) -> Decision {
    let kind = profile.kind;
    let usable: Vec<&Counted<'_>> = counted.iter().filter(|c| c.level.is_some()).collect();
    let clusters_at = |floor: TrustLevel| {
        let mut seen: Vec<&ClusterId> = Vec::new();
        for c in usable.iter().filter(|c| c.level >= Some(floor)) {
            if !seen.contains(&&c.support.origin_cluster) {
                seen.push(&c.support.origin_cluster);
            }
        }
        seen.len()
    };
    let stale: Vec<&str> = usable
        .iter()
        .filter_map(|c| match &c.support.temporal {
            super::temporal::TemporalValidity::Stale { newer_primary } => {
                Some(newer_primary.as_str())
            }
            _ => None,
        })
        .collect();
    let testimonial_would_meet =
        lowest.is_some_and(|lowest| usable.iter().any(|c| c.reported && c.page >= lowest));
    let missing = if !usable.is_empty() && stale.len() == usable.len() {
        Missing::OnlyStale {
            newer_primary: stale.first().map(|s| (*s).to_owned()).unwrap_or_default(),
        }
    } else if testimonial_would_meet {
        Missing::OnlyTestimonial
    } else {
        match kind {
            ClaimKind::Specification | ClaimKind::VersionBound => Missing::NoPrimaryForSubject,
            ClaimKind::Measurement => Missing::NoMeasurementOrigin,
            ClaimKind::Historical => Missing::NoIndependentCorroboration {
                have: clusters_at(TrustLevel::Established),
            },
            ClaimKind::Definition | ClaimKind::Practice
                if clusters_at(TrustLevel::Community) == 1 =>
            {
                Missing::NoIndependentCorroboration { have: 1 }
            }
            _ => Missing::NoIdentifiedSource,
        }
    };

    let mut next = Vec::new();
    let mut push = |step: NextStep| {
        if !next.contains(&step) {
            next.push(step);
        }
    };
    for newer in &stale {
        push(NextStep::FetchNewer {
            newer_primary: (*newer).to_owned(),
        });
    }
    let mut by_level: Vec<&&Counted<'_>> = usable.iter().collect();
    by_level.sort_by_key(|c| std::cmp::Reverse(c.page));
    for c in &by_level {
        for url in &c.support.cited_urls {
            push(NextStep::FollowCitation {
                url: url.clone(),
                from: c.support.source_version_id.clone(),
            });
        }
    }
    if matches!(
        kind,
        ClaimKind::Specification | ClaimKind::VersionBound | ClaimKind::Measurement
    ) {
        let mut hints: Vec<String> = Vec::new();
        for c in counted {
            for hint in &c.support.owner_hints {
                if !hints.contains(hint) {
                    hints.push(hint.clone());
                }
            }
        }
        if !hints.is_empty() {
            push(NextStep::SearchOwner {
                origin_hints: hints,
            });
        }
    }
    push(NextStep::SearchIndependent);
    push(NextStep::AskUser);

    let have_levels: Vec<&str> = usable
        .iter()
        .filter_map(|c| c.level.map(TrustLevel::as_str))
        .collect();
    let about = profile
        .subject
        .as_ref()
        .map(|subject| format!(" about {}", subject.name))
        .unwrap_or_default();
    reasons.push(Reason::new(
        ReasonCode::BelowBar,
        format!(
            "{} claim{about}: {}; needs {}",
            capitalised(kind_name(kind)),
            if have_levels.is_empty() {
                "no support that can be counted".to_owned()
            } else {
                format!("supported only by {} sources", have_levels.join(", "))
            },
            requirement(kind, lowest)
        ),
    ));
    reasons.extend(bars.moved_reason(kind));
    Decision::Insufficient {
        bar: Bar::for_kind(kind),
        have: counted
            .iter()
            .map(|c| CountedSupport {
                source_version_id: c.support.source_version_id.clone(),
                origin_cluster: c.support.origin_cluster.clone(),
                counted: c.level,
            })
            .collect(),
        missing,
        next,
        reasons,
    }
}

/// What meets the bar, in words.
fn requirement(kind: ClaimKind, lowest: Option<TrustLevel>) -> String {
    let alone = match lowest {
        Some(TrustLevel::Primary) if kind == ClaimKind::Measurement => {
            "the measurement's own source".to_owned()
        }
        Some(TrustLevel::Primary) => "the owner's own source".to_owned(),
        Some(level) => format!("one {} source or better", level.as_str()),
        None => "independent corroboration (calibration accepts no single source)".to_owned(),
    };
    let corroboration = match kind {
        ClaimKind::Specification | ClaimKind::VersionBound | ClaimKind::Historical => {
            " or two independent established sources"
        }
        ClaimKind::Measurement => " or two independent reports naming the same origin",
        ClaimKind::Definition | ClaimKind::Practice => " or two independent community sources",
        ClaimKind::Opinion | ClaimKind::General => "",
    };
    format!("{alone}{corroboration}")
}

fn one_lower(level: TrustLevel) -> TrustLevel {
    match level {
        TrustLevel::Primary => TrustLevel::Authoritative,
        TrustLevel::Authoritative => TrustLevel::Established,
        TrustLevel::Established => TrustLevel::Community,
        TrustLevel::Community | TrustLevel::Unknown => TrustLevel::Unknown,
        TrustLevel::Distrusted => TrustLevel::Distrusted,
    }
}

fn kind_name(kind: ClaimKind) -> &'static str {
    match kind {
        ClaimKind::VersionBound => "version-bound",
        other => other.as_str(),
    }
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "sufficiency_tests.rs"]
mod tests;
