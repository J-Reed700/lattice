//! lattice-core's provenance types as the trust commands' DTOs show them,
//! and the user's override input as the core override it states (§16, §19).
//! A site override carries a standing and never a level; a level, Primary
//! included, comes only with a source or a site for a subject class.
//!
//! The one mapping: the trust commands, the learner view and the review
//! card all show the gate's levels, decisions and bars through it.

use crate::features::learning::dto::{
    LearningBarBasis, LearningBarChangeDto, LearningClaimProfileDto, LearningClaimSubjectDto,
    LearningKindBarDto, LearningLevelBarDto, LearningSiteStanding, LearningSourceProvenanceDto,
    LearningSourceRelationDto, LearningSourceRelationKind, LearningSourceStanding,
    LearningSourceTrustDto, LearningSubjectClassDto, LearningSufficiencyDto, LearningTrustLevel,
    LearningTrustOverrideDto, LearningTrustOverrideInputDto, LearningTrustOverrideScopeDto,
    LearningTrustReasonDto,
};
use lattice_core::domain::provenance::{
    Available, BarBasis, BarChange, Calibration, ClaimProfile, Decision, EffectiveBar,
    OverrideScope, Policy, ProvenanceRecord, ProvenanceSignals, Reason, RelationKind,
    RelationTarget, SiteStanding, SourceRelation, Standing, SubjectClass, SubjectKey, Support,
    TrustLevel, TrustOverride,
};
use lattice_core::shared::error::{AppError, Result};

pub fn level(level: TrustLevel) -> LearningTrustLevel {
    match level {
        TrustLevel::Primary => LearningTrustLevel::Primary,
        TrustLevel::Authoritative => LearningTrustLevel::Authoritative,
        TrustLevel::Established => LearningTrustLevel::Established,
        TrustLevel::Community => LearningTrustLevel::Community,
        TrustLevel::Unknown => LearningTrustLevel::Unknown,
        TrustLevel::Distrusted => LearningTrustLevel::Distrusted,
    }
}

fn core_level(level: LearningTrustLevel) -> TrustLevel {
    match level {
        LearningTrustLevel::Primary => TrustLevel::Primary,
        LearningTrustLevel::Authoritative => TrustLevel::Authoritative,
        LearningTrustLevel::Established => TrustLevel::Established,
        LearningTrustLevel::Community => TrustLevel::Community,
        LearningTrustLevel::Unknown => TrustLevel::Unknown,
        LearningTrustLevel::Distrusted => TrustLevel::Distrusted,
    }
}

pub fn standing(standing: Standing) -> LearningSourceStanding {
    match standing {
        Standing::Institutional => LearningSourceStanding::Institutional,
        Standing::Identified => LearningSourceStanding::Identified,
        Standing::Community => LearningSourceStanding::Community,
        Standing::Unknown => LearningSourceStanding::Unknown,
    }
}

fn site_standing(standing: SiteStanding) -> LearningSiteStanding {
    match standing {
        SiteStanding::Institutional => LearningSiteStanding::Institutional,
        SiteStanding::Identified => LearningSiteStanding::Identified,
        SiteStanding::Community => LearningSiteStanding::Community,
        SiteStanding::Distrusted => LearningSiteStanding::Distrusted,
    }
}

fn core_site_standing(standing: LearningSiteStanding) -> SiteStanding {
    match standing {
        LearningSiteStanding::Institutional => SiteStanding::Institutional,
        LearningSiteStanding::Identified => SiteStanding::Identified,
        LearningSiteStanding::Community => SiteStanding::Community,
        LearningSiteStanding::Distrusted => SiteStanding::Distrusted,
    }
}

/// The reasons verbatim, each with its rule's wire name.
pub fn reasons(reasons: &[Reason]) -> Vec<LearningTrustReasonDto> {
    reasons.iter().map(reason).collect()
}

fn reason(reason: &Reason) -> LearningTrustReasonDto {
    LearningTrustReasonDto {
        code: wire_name(&reason.code, None).unwrap_or_default(),
        text: reason.text.clone(),
    }
}

fn class(class: &SubjectClass) -> LearningSubjectClassDto {
    LearningSubjectClassDto {
        label: class.label.clone(),
        key: class.key.0.clone(),
    }
}

pub fn scope(scope: &OverrideScope) -> LearningTrustOverrideScopeDto {
    match scope {
        OverrideScope::Source {
            program_id,
            source_id,
        } => LearningTrustOverrideScopeDto::Source {
            program_id: program_id.clone(),
            source_id: source_id.clone(),
        },
        OverrideScope::Site { registrable_domain } => LearningTrustOverrideScopeDto::Site {
            registrable_domain: registrable_domain.clone(),
        },
        OverrideScope::SiteSubject {
            registrable_domain,
            class: subject_class,
        } => LearningTrustOverrideScopeDto::SiteSubject {
            registrable_domain: registrable_domain.clone(),
            class: class(subject_class),
        },
    }
}

/// The scope a DTO names. A class is matched by its key, so the label the
/// user sees is kept but the key is recomputed from it.
pub fn core_scope(scope: &LearningTrustOverrideScopeDto) -> OverrideScope {
    match scope {
        LearningTrustOverrideScopeDto::Source {
            program_id,
            source_id,
        } => OverrideScope::Source {
            program_id: program_id.clone(),
            source_id: source_id.clone(),
        },
        LearningTrustOverrideScopeDto::Site { registrable_domain } => OverrideScope::Site {
            registrable_domain: registrable_domain.trim().to_lowercase(),
        },
        LearningTrustOverrideScopeDto::SiteSubject {
            registrable_domain,
            class,
        } => OverrideScope::SiteSubject {
            registrable_domain: registrable_domain.trim().to_lowercase(),
            class: subject_class(&class.label),
        },
    }
}

fn subject_class(label: &str) -> SubjectClass {
    SubjectClass {
        label: label.trim().to_owned(),
        key: SubjectKey::of(label),
        kinds: Vec::new(),
    }
}

pub fn trust_override(entry: &TrustOverride) -> LearningTrustOverrideDto {
    match entry {
        TrustOverride::Source {
            program_id,
            source_id,
            level: set,
            note,
            set_at,
        } => LearningTrustOverrideDto::Source {
            program_id: program_id.clone(),
            source_id: source_id.clone(),
            level: level(*set),
            note: note.clone(),
            set_at: *set_at,
        },
        TrustOverride::Site {
            registrable_domain,
            standing,
            note,
            set_at,
        } => LearningTrustOverrideDto::Site {
            registrable_domain: registrable_domain.clone(),
            standing: site_standing(*standing),
            note: note.clone(),
            set_at: *set_at,
        },
        TrustOverride::SiteSubject {
            registrable_domain,
            class: subject_class,
            level: set,
            note,
            set_at,
        } => LearningTrustOverrideDto::SiteSubject {
            registrable_domain: registrable_domain.clone(),
            class: class(subject_class),
            level: level(*set),
            note: note.clone(),
            set_at: *set_at,
        },
    }
}

fn note(note: &Option<String>) -> Option<String> {
    note.as_deref()
        .map(str::trim)
        .filter(|note| !note.is_empty())
        .map(str::to_owned)
}

/// The override the user stated, set at `set_at` (unix seconds). A site is
/// named by its registrable domain in lower case, as capture records it.
pub fn override_input(input: &LearningTrustOverrideInputDto, set_at: i64) -> Result<TrustOverride> {
    Ok(match input {
        LearningTrustOverrideInputDto::Source {
            program_id,
            source_id,
            level: set,
            note: text,
        } => TrustOverride::Source {
            program_id: program_id.clone(),
            source_id: source_id.clone(),
            level: core_level(*set),
            note: note(text),
            set_at,
        },
        LearningTrustOverrideInputDto::Site {
            registrable_domain,
            standing,
            note: text,
        } => TrustOverride::Site {
            registrable_domain: registrable_domain.trim().to_lowercase(),
            standing: core_site_standing(*standing),
            note: note(text),
            set_at,
        },
        LearningTrustOverrideInputDto::SiteSubject {
            registrable_domain,
            class_label,
            level: set,
            note: text,
        } => {
            if class_label.trim().is_empty() {
                return Err(AppError::InvalidInput(
                    "Name the subject class this site is trusted for.".into(),
                ));
            }
            TrustOverride::SiteSubject {
                registrable_domain: registrable_domain.trim().to_lowercase(),
                class: subject_class(class_label),
                level: core_level(*set),
                note: note(text),
                set_at,
            }
        }
    })
}

fn relation_kind(kind: RelationKind) -> LearningSourceRelationKind {
    match kind {
        RelationKind::CopyOf => LearningSourceRelationKind::CopyOf,
        RelationKind::ContainsCopyOf => LearningSourceRelationKind::ContainsCopyOf,
        RelationKind::ArchiveOf => LearningSourceRelationKind::ArchiveOf,
        RelationKind::CanonicalOf => LearningSourceRelationKind::CanonicalOf,
        RelationKind::Cites => LearningSourceRelationKind::Cites,
        RelationKind::CitedForClaim => LearningSourceRelationKind::CitedForClaim,
    }
}

pub fn relation(relation: &SourceRelation) -> LearningSourceRelationDto {
    let (to_version_id, to_url) = match &relation.to {
        RelationTarget::Version(id) => (Some(id.clone()), None),
        RelationTarget::Url(url) => (None, Some(url.clone())),
    };
    LearningSourceRelationDto {
        kind: relation_kind(relation.kind),
        from_version_id: relation.from.clone(),
        to_version_id,
        to_url,
    }
}

/// What capture recorded, with the relations that touch the version in
/// either direction ("mirror of …", "cited by …").
pub fn provenance(
    record: &ProvenanceRecord,
    relations: &[SourceRelation],
) -> LearningSourceProvenanceDto {
    let version = &record.source_version_id;
    let touching = relations
        .iter()
        .filter(|edge| {
            edge.from == *version
                || matches!(&edge.to, RelationTarget::Version(to) if to == version)
        })
        .map(relation)
        .collect();
    let (publisher, author, site_name, not_extracted) = match &record.signals {
        ProvenanceSignals::Web(page) => (
            page.publisher.as_ref().map(|entity| entity.name.clone()),
            page.author.as_ref().map(|entity| entity.name.clone()),
            page.site_name.clone(),
            None,
        ),
        ProvenanceSignals::Document(document) => (
            document
                .declared_origin
                .as_ref()
                .and_then(|origin| origin.description.clone()),
            None,
            None,
            None,
        ),
        ProvenanceSignals::Pasted(pasted) => (
            pasted
                .declared_origin
                .as_ref()
                .and_then(|origin| origin.description.clone()),
            None,
            None,
            None,
        ),
        ProvenanceSignals::NotExtracted { reason } => (None, None, None, Some(reason.clone())),
    };
    let dates = record.signals.dates();
    LearningSourceProvenanceDto {
        source_version_id: version.clone(),
        origin_key: record.origin.key.0.clone(),
        site: record.origin.site().map(str::to_owned),
        publisher,
        author,
        site_name,
        published_at: dates.and_then(|dates| dates.published),
        modified_at: dates.and_then(|dates| dates.modified),
        archived_from: record
            .origin
            .archived
            .as_ref()
            .map(|archived| archived.original_url.clone()),
        not_extracted,
        relations: touching,
    }
}

pub fn profile(profile: &ClaimProfile) -> LearningClaimProfileDto {
    LearningClaimProfileDto {
        claim_kind: profile.kind.as_str().to_owned(),
        subject: profile
            .subject
            .as_ref()
            .map(|subject| LearningClaimSubjectDto {
                name: subject.name.clone(),
                kind: subject.kind.as_str().to_owned(),
                aliases: subject.aliases.clone(),
                classes: subject.classes.clone(),
            }),
        measurement_origin: profile.measurement_origin.clone(),
        reasons: reasons(&profile.reasons),
    }
}

/// One kind's bar with each level's verdicts and the probability that its
/// claims hold at least `policy.accept_reliability` of the time.
pub fn kind_bar(
    bar: &EffectiveBar,
    calibration: &Calibration,
    policy: &Policy,
) -> LearningKindBarDto {
    LearningKindBarDto {
        claim_kind: bar.kind.as_str().to_owned(),
        static_lowest: level(bar.static_lowest),
        lowest_accepted: bar.lowest_accepted.map(level),
        levels: bar
            .levels
            .iter()
            .map(|entry| {
                let posterior = calibration
                    .cell(entry.level, bar.kind)
                    .map(|cell| cell.posterior)
                    .unwrap_or_default();
                let (basis, since) = match entry.basis {
                    BarBasis::Static => (LearningBarBasis::Static, None),
                    BarBasis::Relaxed { since, .. } => (LearningBarBasis::Relaxed, Some(since)),
                    BarBasis::Tightened { since, .. } => (LearningBarBasis::Tightened, Some(since)),
                };
                LearningLevelBarDto {
                    level: level(entry.level),
                    accepted_alone: entry.accepted_alone,
                    basis,
                    held: posterior.held,
                    failed: posterior.failed,
                    probability_reliable: posterior.probability_reliable(policy),
                    since,
                }
            })
            .collect(),
        reasons: reasons(&bar.reasons),
    }
}

/// A support as the gate levelled it for the claim's subject.
pub fn support_trust(profile: &ClaimProfile, support: &Support) -> LearningSourceTrustDto {
    LearningSourceTrustDto {
        source_version_id: support.source_version_id.clone(),
        level: level(support.trust.level),
        subject: profile.authority_subject().map(|subject| subject.name),
        reasons: reasons(&support.trust.reasons),
        override_scope: support.trust.override_.as_ref().map(|by| scope(&by.scope)),
    }
}

/// A serialised enum's tag or name, as the wire spells it.
fn wire_name<T: serde::Serialize>(value: &T, tag: Option<&str>) -> Option<String> {
    let value = serde_json::to_value(value).ok()?;
    let name = match tag {
        Some(tag) => value.get(tag)?.as_str()?.to_owned(),
        None => value.as_str()?.to_owned(),
    };
    Some(name)
}

/// The gate's decision on a claim (§18.9): whether its support met the bar
/// for its kind, the bar it failed, the label it is shown with, the rules
/// that decided, and the best level the course reaches on the claim's
/// subject (`available`, claim sources design §6.1) as its ceiling.
pub fn sufficiency(
    profile: &ClaimProfile,
    decision: &Decision,
    available: Option<Available>,
) -> LearningSufficiencyDto {
    let (sufficient, bar, label, missing, decided) = match decision {
        Decision::Sufficient { label, reasons, .. } => (
            true,
            None,
            label
                .as_ref()
                .and_then(|label| wire_name(label, Some("label"))),
            None,
            reasons,
        ),
        Decision::Insufficient {
            bar,
            missing,
            reasons,
            ..
        } => (
            false,
            wire_name(bar, None),
            None,
            wire_name(missing, Some("missing")),
            reasons,
        ),
    };
    LearningSufficiencyDto {
        claim_kind: profile.kind.as_str().to_owned(),
        subject: profile.subject.as_ref().map(|subject| subject.name.clone()),
        sufficient,
        bar,
        label,
        missing,
        ceiling: available.and_then(|available| available.ceiling).map(level),
        reasons: reasons(decided),
    }
}

/// One automatic bar change with the verdict that made it.
pub fn bar_change(change: &BarChange) -> LearningBarChangeDto {
    LearningBarChangeDto {
        claim_kind: change.kind.as_str().to_owned(),
        level: level(change.level),
        at: change.at,
        accepted_alone: change.accepted_alone,
        moves_bar: change.moves_bar(),
        label_source: change.label_source.as_str().to_owned(),
        lesson_id: change.lesson_id.clone(),
        claim_id: change.claim_id.clone(),
        reason: reason(&change.reason),
    }
}
