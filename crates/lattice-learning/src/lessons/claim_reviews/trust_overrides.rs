//! The trust commands' work that reaches the claim ledger (source provenance
//! §18.7, §19): the trust view with what each override scope would re-decide,
//! and setting or clearing an override. Which claims a change re-decides is
//! `trust_changed`'s rule, so the count the menu shows is the claims
//! re-decided.

use std::collections::BTreeSet;

use lattice_core::domain::learning::claim_review::TrustCause;
use lattice_core::domain::provenance::OverrideScope;
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::Database;

use super::count;
use super::trust_changed::{override_changed, Redecidable, Redecided};
use crate::dto::{
    GetLearningSourceTrustRequestDto, LearningSourceTrustEntryDto, LearningSourceTrustViewDto,
    LearningTrustChangeDto, LearningTrustChangeResultDto, LearningTrustImpactDto,
    LearningTrustImpactsDto, LearningTrustOverrideInputDto, LearningTrustOverrideScopeDto,
    LearningTrustRedecidedDto,
};
use crate::references::trust::overrides::{self, preview, CoveredVersion, TrustChange};
use crate::references::trust::repository::unix_seconds;
use crate::references::trust::{dto_map, source_view, SqliteSourceTrustStore};

/// The trust view a request asks for; with `include_impact`, each source
/// (and each of the claim's supports) says what an override of the source,
/// and of its site, would re-decide.
pub async fn view(
    database: &Database,
    request: &GetLearningSourceTrustRequestDto,
) -> Result<LearningSourceTrustViewDto> {
    let store = SqliteSourceTrustStore::new(database.clone());
    let mut view = source_view::view(database, &store, request).await?;
    if !request.include_impact {
        return Ok(view);
    }
    let ledger = Redecidable::load(database).await?;
    let supports = view
        .claim
        .iter_mut()
        .flat_map(|claim| claim.supports.iter_mut());
    for entry in view.sources.iter_mut().chain(supports) {
        entry.impact = Some(impacts(database, &ledger, &request.program_id, entry).await?);
    }
    Ok(view)
}

async fn impacts(
    database: &Database,
    ledger: &Redecidable,
    program_id: &str,
    entry: &LearningSourceTrustEntryDto,
) -> Result<LearningTrustImpactsDto> {
    let source = OverrideScope::Source {
        program_id: program_id.to_owned(),
        source_id: entry.source_id.clone(),
    };
    let source = reach(ledger, &preview(database, &source).await?);
    let site = match entry
        .provenance
        .as_ref()
        .and_then(|provenance| provenance.site.clone())
    {
        Some(registrable_domain) => {
            let scope = OverrideScope::Site { registrable_domain };
            Some(reach(ledger, &preview(database, &scope).await?))
        }
        None => None,
    };
    Ok(LearningTrustImpactsDto { source, site })
}

/// How many claims, in how many lessons, a change of `versions`' trust
/// re-decides.
fn reach(ledger: &Redecidable, versions: &[CoveredVersion]) -> LearningTrustImpactDto {
    let cause = TrustCause::Override {
        source_version_ids: versions
            .iter()
            .map(|version| version.source_version_id.clone())
            .collect(),
    };
    let claims: Vec<_> = ledger.affected(&cause).collect();
    let lessons: BTreeSet<&str> = claims
        .iter()
        .map(|claim| claim.lesson_id.as_str())
        .collect();
    LearningTrustImpactDto {
        versions: count(versions.len()),
        claims: count(claims.len()),
        lessons: count(lessons.len()),
    }
}

/// Creates or replaces the override at its scope, then re-decides the
/// claims it touches.
pub async fn set(
    database: &Database,
    input: &LearningTrustOverrideInputDto,
) -> Result<LearningTrustChangeResultDto> {
    let entry = dto_map::override_input(input, unix_seconds())?;
    let change = overrides::set(database, &entry).await?;
    changed(database, change).await
}

/// Removes the override at `scope`, then re-decides the claims it touched;
/// `None` when there was none.
pub async fn clear(
    database: &Database,
    scope: &LearningTrustOverrideScopeDto,
) -> Result<Option<LearningTrustChangeResultDto>> {
    match overrides::clear(database, &dto_map::core_scope(scope)).await? {
        Some(change) => Ok(Some(changed(database, change).await?)),
        None => Ok(None),
    }
}

async fn changed(database: &Database, change: TrustChange) -> Result<LearningTrustChangeResultDto> {
    let redecided = override_changed(database, &change).await?;
    let impact = reach(&Redecidable::load(database).await?, &change.versions);
    Ok(LearningTrustChangeResultDto {
        change: LearningTrustChangeDto {
            scope: dto_map::scope(&change.scope),
            source_version_ids: change
                .versions
                .into_iter()
                .map(|version| version.source_version_id)
                .collect(),
        },
        impact,
        redecided: redecided_dto(&redecided),
    })
}

fn redecided_dto(redecided: &Redecided) -> LearningTrustRedecidedDto {
    LearningTrustRedecidedDto {
        verified: count(redecided.verified),
        reopened: count(redecided.reopened),
        still_failing: count(redecided.still_failing),
        not_levelled: count(redecided.not_levelled),
        lessons: count(redecided.lessons.len()),
    }
}
