//! The support a claim card leads with (UI design §5.1): the strongest
//! support the gate counted and the others it weighed, or, with none, the
//! passage that contradicts the claim; each with its source's kind, site
//! and level for the claim's subject.

use std::collections::{BTreeSet, HashMap};

use lattice_core::application::ports::SourceTrustStore;
use lattice_core::application::services::source_trust::STANDING_POLICY;
use lattice_core::domain::learning::claim_review::{
    ClaimLedgerEntry, ClaimState, Ledger, TrustAssessment,
};
use lattice_core::domain::provenance::{Decision, StandingAssessment, TrustLevel};
use lattice_core::shared::error::Result;
use sqlx::SqlitePool;

use super::super::dto::{source_kind, ClaimSupportCountsDto, ClaimSupportDto, ClaimSupportRoleDto};
use super::super::repository::ShownPassage;
use super::super::service::ClaimReviewService;
use super::{evidence_passage, latest_check, PassageKey, PassageText, SourceFacts};
use crate::features::learning::dto::LearningTrustLevel;
use crate::features::learning::persistence::db;
use crate::features::learning::references::trust::dto_map;
use crate::features::learning::references::trust::repository::SqliteSourceTrustStore;

/// What the review knows of a version beyond the library: the site it was
/// served from and its publisher's judged standing.
#[derive(Default)]
pub(super) struct VersionStanding {
    site: Option<String>,
    assessment: Option<StandingAssessment>,
}

/// What a card's supports are read from.
pub(super) struct SupportContext<'a> {
    pub shown: &'a [ShownPassage],
    pub texts: &'a HashMap<PassageKey, PassageText>,
    pub sources: &'a HashMap<String, SourceFacts>,
    pub standings: &'a HashMap<String, VersionStanding>,
}

impl ClaimReviewService {
    /// The site and standing of every version a claim's support or
    /// contrary passage comes from.
    pub(super) async fn standings(
        &self,
        ledger: &Ledger,
    ) -> Result<HashMap<String, VersionStanding>> {
        let store = SqliteSourceTrustStore::new(self.pool().clone());
        let mut standings = HashMap::new();
        for version in support_versions(ledger) {
            let standing = match store.provenance(&version).await? {
                Some(record) => VersionStanding {
                    site: record.origin.site().map(str::to_owned),
                    assessment: latest_standing(self.pool(), &record.signals_sha256).await?,
                },
                None => VersionStanding::default(),
            };
            standings.insert(version, standing);
        }
        Ok(standings)
    }
}

/// The versions the cards' supports can name.
fn support_versions(ledger: &Ledger) -> BTreeSet<String> {
    let mut versions = BTreeSet::new();
    for entry in &ledger.claims {
        if let Some(trust) = entry.trust() {
            versions.extend(trust.supports.iter().map(|s| s.source_version_id.clone()));
            versions.extend(trust.contrary.iter().map(|c| c.source_version_id.clone()));
        }
        for verifier in [false, true] {
            if let Some((version, _)) =
                latest_check(entry, verifier).and_then(|(check, _)| check.result.support())
            {
                versions.insert(version.to_owned());
            }
        }
    }
    versions
}

/// The latest standing judgement of a publisher's signals, by any model. A
/// judgement this build cannot read is left out: the source shows as a
/// page.
async fn latest_standing(
    pool: &SqlitePool,
    signals_sha256: &str,
) -> Result<Option<StandingAssessment>> {
    let judgement: Option<String> = sqlx::query_scalar(
        "SELECT judgement_json FROM source_standing_assessments WHERE signals_sha256=? AND policy=? ORDER BY assessed_at DESC LIMIT 1",
    )
    .bind(signals_sha256)
    .bind(STANDING_POLICY)
    .fetch_optional(pool)
    .await
    .map_err(db)?;
    Ok(judgement.and_then(|json| serde_json::from_str(&json).ok()))
}

/// The card's `support` and `also_supported`.
pub(super) fn supports(
    entry: &ClaimLedgerEntry,
    context: &SupportContext<'_>,
) -> (Option<ClaimSupportDto>, Vec<ClaimSupportDto>) {
    let quotes = quotes(entry);
    let quote = |version: &str| quotes.get(version).cloned().unwrap_or_default();
    if let Some(trust) = entry.trust().filter(|trust| !trust.supports.is_empty()) {
        let mut found: Vec<ClaimSupportDto> = counted(trust)
            .into_iter()
            .filter_map(|(version, level, counts)| {
                let quote = quote(version);
                support(
                    entry,
                    context,
                    version,
                    ClaimSupportRoleDto::Supports,
                    Some(level),
                    counts,
                    quote,
                )
            })
            .collect();
        let first = (!found.is_empty()).then(|| found.remove(0));
        return (first, found);
    }
    let checked = [false, true].into_iter().find_map(|verifier| {
        latest_check(entry, verifier).and_then(|(check, _)| check.result.support())
    });
    if let Some((version, quote)) = checked {
        let counts = match entry.state() {
            ClaimState::ResolvedByEvidence { .. } => ClaimSupportCountsDto::Enough,
            _ => ClaimSupportCountsDto::NotEnough,
        };
        let found = support(
            entry,
            context,
            version,
            ClaimSupportRoleDto::Supports,
            None,
            counts,
            quote.to_owned(),
        );
        return (found, Vec::new());
    }
    let contrary = entry
        .trust()
        .and_then(|trust| trust.contrary.iter().max_by_key(|c| c.trust.level))
        .and_then(|contrary| {
            support(
                entry,
                context,
                &contrary.source_version_id,
                ClaimSupportRoleDto::Contradicts,
                Some(dto_map::level(contrary.trust.level)),
                ClaimSupportCountsDto::NotCounted {
                    reason: "it contradicts the claim".into(),
                },
                contrary.quote.clone(),
            )
        });
    (contrary, Vec::new())
}

/// The quotation the checks cite from each version: the one the gate
/// weighed first, then the checkers' latest.
fn quotes(entry: &ClaimLedgerEntry) -> HashMap<String, String> {
    let mut quotes = HashMap::new();
    if let Some(trust) = entry.trust() {
        quotes.insert(
            trust.supported.source_version_id.clone(),
            trust.supported.supporting_quote.clone(),
        );
    }
    for verifier in [false, true] {
        if let Some((version, quote)) =
            latest_check(entry, verifier).and_then(|(check, _)| check.result.support())
        {
            quotes
                .entry(version.to_owned())
                .or_insert_with(|| quote.to_owned());
        }
    }
    quotes
}

/// The gate's supports, strongest counted first, each with its level for
/// the claim's subject and whether it counted.
fn counted(trust: &TrustAssessment) -> Vec<(&str, LearningTrustLevel, ClaimSupportCountsDto)> {
    let mut ranked: Vec<(usize, Option<TrustLevel>, &str, ClaimSupportCountsDto)> = trust
        .supports
        .iter()
        .map(|support| {
            let version = support.source_version_id.as_str();
            let (rank, level, counts) = match &trust.decision {
                Decision::Sufficient { met_by, .. } => {
                    match met_by.iter().position(|v| v == version) {
                        Some(at) => (at, Some(support.trust.level), ClaimSupportCountsDto::Enough),
                        None => (
                            usize::MAX,
                            counted_level(support.trust.level),
                            not_enough(support.trust.level),
                        ),
                    }
                }
                Decision::Insufficient { have, .. } => {
                    let level = have
                        .iter()
                        .find(|have| have.source_version_id == version)
                        .and_then(|have| have.counted);
                    let counts = match level {
                        Some(_) => ClaimSupportCountsDto::NotEnough,
                        None => excluded(support.trust.level),
                    };
                    (usize::MAX, level, counts)
                }
            };
            (rank, level, version, counts)
        })
        .collect();
    ranked.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
    ranked
        .into_iter()
        .map(|(_, _, version, counts)| {
            let level = trust
                .supports
                .iter()
                .find(|support| support.source_version_id == version)
                .map_or(LearningTrustLevel::Unknown, |support| {
                    dto_map::level(support.trust.level)
                });
            (version, level, counts)
        })
        .collect()
}

/// A support outside the ones that met the bar: below it, or excluded.
fn not_enough(level: TrustLevel) -> ClaimSupportCountsDto {
    match counted_level(level) {
        Some(_) => ClaimSupportCountsDto::NotEnough,
        None => excluded(level),
    }
}

fn counted_level(level: TrustLevel) -> Option<TrustLevel> {
    (level != TrustLevel::Distrusted).then_some(level)
}

/// Why the gate left a support out: the user distrusts it, else its date
/// rules it out for this claim.
fn excluded(level: TrustLevel) -> ClaimSupportCountsDto {
    let reason = if level == TrustLevel::Distrusted {
        "the source is distrusted"
    } else {
        "its date rules it out for this claim"
    };
    ClaimSupportCountsDto::NotCounted {
        reason: reason.into(),
    }
}

/// One support of a card, on the passage the checks compared from its
/// version; `None` when no compared passage of it is known.
fn support(
    entry: &ClaimLedgerEntry,
    context: &SupportContext<'_>,
    version: &str,
    role: ClaimSupportRoleDto,
    standing: Option<LearningTrustLevel>,
    counts: ClaimSupportCountsDto,
    quote: String,
) -> Option<ClaimSupportDto> {
    let facts = context.sources.get(version)?;
    let (text, span, retrieval_kind) = passage(context, version, &quote)?;
    let passage = evidence_passage(
        version,
        &text,
        span,
        &retrieval_kind,
        context.sources,
        entry.trust(),
    )
    .passage;
    let standing_of = context.standings.get(version);
    let judged = standing_of
        .and_then(|standing| standing.assessment.as_ref())
        .map(|assessment| (assessment.nature, assessment.publisher_kind));
    let quote_span = (!quote.is_empty())
        .then(|| passage.text.find(&quote))
        .flatten()
        .map(|at| (at, at + quote.len()));
    Some(ClaimSupportDto {
        role,
        source: facts.reference(version),
        site: standing_of
            .and_then(|standing| standing.site.clone())
            .or_else(|| facts.host()),
        kind: source_kind(judged, &facts.library_kind),
        standing,
        counts,
        quote,
        quote_span,
        passage,
    })
}

/// The compared passage of `version` a support shows: the card's own
/// passages first, then those the latest checks compared; of them, one
/// holding the quote, else the earliest.
fn passage(
    context: &SupportContext<'_>,
    version: &str,
    quote: &str,
) -> Option<(String, (usize, usize), String)> {
    let mut candidates: Vec<(String, (usize, usize), String)> = context
        .shown
        .iter()
        .filter(|passage| passage.source_version_id == version)
        .map(|passage| {
            (
                passage.text.clone(),
                (passage.start_byte, passage.end_byte),
                passage.retrieval_kind.clone(),
            )
        })
        .collect();
    if candidates.is_empty() {
        candidates = context
            .texts
            .iter()
            .filter(|((id, _, _), _)| id == version)
            .map(|((_, start, end), found)| {
                (
                    found.text.clone(),
                    (*start, *end),
                    found.retrieval_kind.clone(),
                )
            })
            .collect();
        candidates.sort_by_key(|(_, span, _)| *span);
    }
    let holding = candidates
        .iter()
        .position(|(text, _, _)| !quote.is_empty() && text.contains(quote));
    match holding {
        Some(at) => Some(candidates.swap_remove(at)),
        None => candidates.into_iter().next(),
    }
}
