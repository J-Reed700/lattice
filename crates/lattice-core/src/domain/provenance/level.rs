//! The level procedure (§5): standing, ownership, relations, flags and
//! overrides combined into one level for one subject, with every rule that
//! fired as a reason.

use super::authority::{AuthorityAssessment, InheritedTrust, SubjectRelation, Trust, TrustLevel};
use super::claim_profile::ClaimSubject;
use super::origin::Origin;
use super::overrides::{Overrides, SiteStanding};
use super::policy::Policy;
use super::reason::{AddedBy, Reason, ReasonCode};
use super::signals::ProvenanceSignals;
use super::standing::{ContentNature, EditorialProcess, RiskFlag, Standing, StandingAssessment};

/// Everything the level of one source version for one subject depends on.
#[derive(Debug, Clone, Copy)]
pub struct LevelInputs<'a> {
    pub program_id: &'a str,
    /// The `learning_source_library` id: source overrides are per source,
    /// across its versions.
    pub source_id: &'a str,
    pub origin: &'a Origin,
    pub signals: &'a ProvenanceSignals,
    pub standing: &'a StandingAssessment,
    /// The claim's subject (the measurement's origin for a measurement);
    /// `None` when the claim names none.
    pub subject: Option<&'a ClaimSubject>,
    /// The ownership judgement for (origin, subject).
    pub relation: Option<&'a AuthorityAssessment>,
    /// Levels this version may inherit through copy, archive or canonical
    /// relations, each the related version's level for the same subject.
    pub inherited: &'a [InheritedTrust],
    pub overrides: &'a Overrides,
    pub added_by: AddedBy,
    pub policy: &'a Policy,
}

/// Natures an owner publishes as its own account of its subject (§5 step 5).
const OWNER_PRIMARY_NATURES: [ContentNature; 4] = [
    ContentNature::PrimaryDocument,
    ContentNature::Documentation,
    ContentNature::Reference,
    ContentNature::Reporting,
];

/// Natures that cap a level at Established whoever publishes them (§5
/// step 7): an evaluation, a sales page or a digest of other pages is not
/// an authority on the facts it mentions.
const CAPPED_NATURES: [ContentNature; 3] = [
    ContentNature::Opinion,
    ContentNature::Marketing,
    ContentNature::Aggregation,
];

/// The level of one source version for one subject (§5).
///
/// 1. A source override for this source in this course fixes the level.
/// 2. A site standing override replaces the judged standing (it says who
///    the publisher is, never what it owns, so it cannot make Primary); a
///    site the user distrusts is Distrusted.
/// 3. Metadata addressed to a reader or a model makes it Distrusted.
/// 4. A site-for-subject override whose class takes in the subject fixes
///    the level, Primary included.
/// 5. The subject's owner publishing documentation, a primary document, a
///    reference or a report is Primary; anything else the owner hosts is
///    Established.
/// 6. Otherwise standing and editorial process decide: Institutional with
///    peer review or editing is Authoritative, other institutional and
///    identified publishing Established, community-edited content
///    Community, an unknown publisher Unknown.
/// 7. Opinion, marketing and aggregation cap at Established.
/// 8. Self-declared officiality without ownership, content-farm and
///    generated-content patterns cap at Community.
/// 9. A verified copy, archive or canonical source of a version with a
///    higher level inherits that level (only upward: a mirror never lowers
///    the original it copies).
/// 10. User material without a stated origin keeps what is known and says
///     so.
pub fn trust_level(inputs: &LevelInputs<'_>) -> Trust {
    let mut reasons = Vec::new();
    if let Some((level, entry)) = inputs.overrides.source(inputs.program_id, inputs.source_id) {
        reasons.push(Reason::new(
            ReasonCode::SourceOverride,
            format!(
                "You set this source to {} for this course{}",
                level.as_str(),
                note_suffix(entry.note())
            ),
        ));
        return Trust {
            level,
            reasons,
            override_: Some(entry.reference()),
        };
    }

    let site = inputs.origin.site();
    let mut standing = inputs.standing.standing;
    let mut override_ = None;
    if let Some((site_standing, entry)) = site.and_then(|site| inputs.overrides.site(site)) {
        let domain = site.unwrap_or_default();
        if site_standing == SiteStanding::Distrusted {
            reasons.push(Reason::new(
                ReasonCode::DistrustedByYou,
                format!("You distrust {domain}{}", note_suffix(entry.note())),
            ));
            return Trust {
                level: TrustLevel::Distrusted,
                reasons,
                override_: Some(entry.reference()),
            };
        }
        standing = match site_standing {
            SiteStanding::Institutional => Standing::Institutional,
            SiteStanding::Identified => Standing::Identified,
            SiteStanding::Community | SiteStanding::Distrusted => Standing::Community,
        };
        reasons.push(Reason::new(
            ReasonCode::SiteStandingOverride,
            format!(
                "You set {domain} as {} everywhere{}",
                site_standing.as_str(),
                note_suffix(entry.note())
            ),
        ));
        override_ = Some(entry.reference());
    }

    let injected = inputs.signals.reader_addressed_fields();
    if inputs.standing.flags.contains(&RiskFlag::InjectionAttempt) || !injected.is_empty() {
        let fields = if injected.is_empty() {
            "its metadata".to_owned()
        } else {
            injected.join(", ")
        };
        reasons.push(Reason::new(
            ReasonCode::InjectionDistrusted,
            format!("Text in {fields} addresses a reader or a model, so the source is not used"),
        ));
        return Trust {
            level: TrustLevel::Distrusted,
            reasons,
            override_,
        };
    }

    if let (Some(site), Some(subject)) = (site, inputs.subject) {
        if let Some((level, class, entry)) = inputs.overrides.site_subject(site, subject) {
            reasons.push(Reason::new(
                ReasonCode::SiteSubjectOverride,
                format!(
                    "You set {site} as {} for {}{}",
                    level.as_str(),
                    class.label,
                    note_suffix(entry.note())
                ),
            ));
            return Trust {
                level,
                reasons,
                override_: Some(entry.reference()),
            };
        }
    }

    let own = own_level(inputs, standing, &mut reasons);
    let level = inherit(inputs, own, &mut reasons);
    if inputs.added_by == AddedBy::User && inputs.signals.is_unattributed_user_material() {
        reasons.push(Reason::new(
            ReasonCode::AddedByYou,
            "Added by you · origin not stated",
        ));
    }
    Trust {
        level,
        reasons,
        override_,
    }
}

/// Steps 5-8: the level from this version's own signals and judgements.
fn own_level(
    inputs: &LevelInputs<'_>,
    standing: Standing,
    reasons: &mut Vec<Reason>,
) -> TrustLevel {
    let assessment = inputs.standing;
    let publisher = publisher_name(inputs);
    let relation = inputs
        .relation
        .map_or(SubjectRelation::Unknown, |judged| judged.relation);
    let subject = inputs.subject.map(|subject| subject.name.as_str());
    if subject.is_none() {
        reasons.push(Reason::new(
            ReasonCode::NoSubject,
            "The claim names no subject, so ownership was not judged",
        ));
    }

    let mut level = if relation == SubjectRelation::Owner {
        let subject = subject.unwrap_or("the subject");
        if OWNER_PRIMARY_NATURES.contains(&assessment.nature)
            && assessment.editorial != EditorialProcess::CommunityEdited
        {
            let basis = inputs
                .relation
                .map(|judged| {
                    judged
                        .basis
                        .iter()
                        .map(|reason| reason.text.as_str())
                        .collect::<Vec<_>>()
                        .join("; ")
                })
                .unwrap_or_default();
            reasons.push(Reason::new(
                ReasonCode::OwnerOfSubject,
                format!("Primary for {subject}: {publisher} owns it ({basis})"),
            ));
            TrustLevel::Primary
        } else if assessment.nature == ContentNature::Marketing {
            reasons.push(Reason::new(
                ReasonCode::OwnerMarketing,
                format!("Published by {subject}'s owner, but as marketing"),
            ));
            TrustLevel::Established
        } else {
            reasons.push(Reason::new(
                ReasonCode::OwnerHostedCommunity,
                format!(
                    "Hosted by {subject}'s owner, but {} {} content",
                    assessment.editorial.as_str().replace('_', " "),
                    assessment.nature.as_str().replace('_', " ")
                ),
            ));
            TrustLevel::Established
        }
    } else {
        standing_level(standing, assessment.editorial, &publisher, reasons)
    };

    if CAPPED_NATURES.contains(&assessment.nature) && level > TrustLevel::Established {
        level = TrustLevel::Established;
        reasons.push(Reason::new(
            ReasonCode::ContentNatureCap,
            format!(
                "{} content counts at most as established",
                capitalised(assessment.nature.as_str())
            ),
        ));
    }

    let self_declared = assessment.flags.contains(&RiskFlag::SelfDeclaredOfficial)
        || matches!(inputs.signals, ProvenanceSignals::Web(page) if !page.self_description.is_empty());
    let caps = [
        (
            self_declared && relation != SubjectRelation::Owner,
            ReasonCode::SelfDeclaredOfficialCap,
            "It calls itself official without being the subject's owner",
        ),
        (
            assessment.flags.contains(&RiskFlag::ContentFarmPattern),
            ReasonCode::ContentFarmCap,
            "It follows a content-farm pattern",
        ),
        (
            assessment
                .flags
                .contains(&RiskFlag::GeneratedContentPattern),
            ReasonCode::GeneratedContentCap,
            "It follows a generated-content pattern",
        ),
    ];
    for (applies, code, text) in caps {
        if applies && level > TrustLevel::Community {
            level = TrustLevel::Community;
            reasons.push(Reason::new(code, format!("{text}: at most community")));
        }
    }
    level
}

/// Step 6: the level standing and editorial process give a non-owner.
fn standing_level(
    standing: Standing,
    editorial: EditorialProcess,
    publisher: &str,
    reasons: &mut Vec<Reason>,
) -> TrustLevel {
    let (level, code, text) = match (standing, editorial) {
        (_, EditorialProcess::CommunityEdited) | (Standing::Community, _) => (
            TrustLevel::Community,
            ReasonCode::CommunityPublisher,
            format!("Community content on {publisher}"),
        ),
        (Standing::Institutional, EditorialProcess::PeerReviewed | EditorialProcess::Editorial) => {
            (
                TrustLevel::Authoritative,
                ReasonCode::InstitutionalEditorial,
                format!(
                    "{publisher} publishes it through a {} process",
                    if editorial == EditorialProcess::PeerReviewed {
                        "peer-review"
                    } else {
                        "editorial"
                    }
                ),
            )
        }
        (Standing::Institutional, _) => (
            TrustLevel::Established,
            ReasonCode::InstitutionalSelfPublished,
            format!("{publisher} publishes it without an editorial process"),
        ),
        (Standing::Identified, _) => (
            TrustLevel::Established,
            ReasonCode::IdentifiedPublisher,
            format!("An identified author or publisher: {publisher}"),
        ),
        (Standing::Unknown, _) => (
            TrustLevel::Unknown,
            ReasonCode::UnknownPublisher,
            "No identifiable publisher".to_owned(),
        ),
    };
    reasons.push(Reason::new(code, text));
    level
}

/// Step 9: inherit a higher level through a verified copy relation.
fn inherit(inputs: &LevelInputs<'_>, own: TrustLevel, reasons: &mut Vec<Reason>) -> TrustLevel {
    let best = inputs
        .inherited
        .iter()
        .filter(|candidate| {
            candidate.containment >= inputs.policy.containment_copy
                && candidate.level != TrustLevel::Distrusted
                && candidate.level > own
        })
        .max_by(|a, b| {
            a.level
                .cmp(&b.level)
                .then_with(|| a.containment.total_cmp(&b.containment))
        });
    let Some(best) = best else {
        return own;
    };
    reasons.push(Reason::new(
        ReasonCode::Mirror,
        format!(
            "{} of {} ({:.0}% of its text, verified): {}",
            match best.relation {
                super::independence::RelationKind::ArchiveOf => "Archived copy",
                super::independence::RelationKind::CanonicalOf => "Syndicated copy",
                _ => "Mirror",
            },
            best.of_origin.0.trim_start_matches("web:"),
            best.containment * 100.0,
            best.level.as_str()
        ),
    ));
    best.level
}

/// The publisher as the judgement named it, else the site.
fn publisher_name(inputs: &LevelInputs<'_>) -> String {
    inputs
        .standing
        .publisher
        .as_ref()
        .map(|publisher| publisher.name.clone())
        .or_else(|| inputs.origin.site().map(str::to_owned))
        .unwrap_or_else(|| "an unnamed publisher".to_owned())
}

fn note_suffix(note: Option<&str>) -> String {
    note.filter(|note| !note.trim().is_empty())
        .map(|note| format!(" (\"{}\")", note.trim()))
        .unwrap_or_default()
}

fn capitalised(text: &str) -> String {
    let text = text.replace('_', " ");
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "level_tests.rs"]
mod tests;
