//! Setting and clearing a trust override (§5 steps 1-3, §18.7, §19). An
//! override moves the level of every version it covers, so each change
//! answers with those versions: the claims that cite them and wait on
//! `InsufficientStanding` are what the ledger reopens (`TrustChanged`, P5),
//! and the gate re-decides them without a model call. The override menu
//! counts the same versions to say what will change.

use super::repository::SqliteSourceTrustStore;
use super::rows::{delete_override, write_override};
use crate::features::learning::persistence::db;
use lattice_core::domain::provenance::{OverrideScope, TrustOverride};
use lattice_core::shared::error::{AppError, Result};
use sqlx::{Row, SqliteConnection};

/// One source version an override covers.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CoveredVersion {
    pub program_id: String,
    pub source_version_id: String,
}

/// What an override change can move: every version under its scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustChange {
    pub scope: OverrideScope,
    pub versions: Vec<CoveredVersion>,
}

/// Creates or replaces the override at its scope, in one transaction with
/// reading the versions it covers.
pub async fn set(
    store: &SqliteSourceTrustStore,
    trust_override: &TrustOverride,
) -> Result<TrustChange> {
    let scope = trust_override.scope();
    validate(&scope)?;
    let mut tx = store.pool().begin().await.map_err(db)?;
    write_override(&mut tx, trust_override).await?;
    let versions = covered(&mut tx, &scope).await?;
    tx.commit().await.map_err(db)?;
    Ok(TrustChange { scope, versions })
}

/// Removes the override at `scope`; `None` when there was none, so nothing
/// moves.
pub async fn clear(
    store: &SqliteSourceTrustStore,
    scope: &OverrideScope,
) -> Result<Option<TrustChange>> {
    let mut tx = store.pool().begin().await.map_err(db)?;
    if !delete_override(&mut tx, scope).await? {
        return Ok(None);
    }
    let versions = covered(&mut tx, scope).await?;
    tx.commit().await.map_err(db)?;
    Ok(Some(TrustChange {
        scope: scope.clone(),
        versions,
    }))
}

/// The versions an override at `scope` would cover, without changing it:
/// what the menu counts before the user commits.
pub async fn preview(
    store: &SqliteSourceTrustStore,
    scope: &OverrideScope,
) -> Result<Vec<CoveredVersion>> {
    let mut conn = store.pool().acquire().await.map_err(db)?;
    covered(&mut conn, scope).await
}

fn validate(scope: &OverrideScope) -> Result<()> {
    let blank = match scope {
        OverrideScope::Source {
            program_id,
            source_id,
        } => program_id.trim().is_empty() || source_id.trim().is_empty(),
        OverrideScope::Site { registrable_domain } => registrable_domain.trim().is_empty(),
        OverrideScope::SiteSubject {
            registrable_domain,
            class,
        } => registrable_domain.trim().is_empty() || class.key.0.is_empty(),
    };
    if blank {
        return Err(AppError::InvalidInput(
            "A trust override needs the source, or the site (and subject class), it applies to."
                .into(),
        ));
    }
    Ok(())
}

/// A source scope covers every version of that source in its course; a
/// site scope (with or without a subject class) covers every captured
/// version whose origin is that site, in every course: the class narrows
/// which claims the level applies to, which the gate decides.
async fn covered(
    conn: &mut SqliteConnection,
    scope: &OverrideScope,
) -> Result<Vec<CoveredVersion>> {
    let rows = match scope {
        OverrideScope::Source {
            program_id,
            source_id,
        } => sqlx::query(
            "SELECT program_id, id AS source_version_id FROM learning_source_versions WHERE program_id=? AND source_id=? ORDER BY version_number",
        )
        .bind(program_id)
        .bind(source_id)
        .fetch_all(&mut *conn)
        .await,
        OverrideScope::Site { registrable_domain }
        | OverrideScope::SiteSubject {
            registrable_domain, ..
        } => sqlx::query(
            "SELECT program_id, source_version_id FROM source_provenance WHERE site=? ORDER BY program_id, source_version_id",
        )
        .bind(registrable_domain)
        .fetch_all(&mut *conn)
        .await,
    }
    .map_err(db)?;
    Ok(rows
        .iter()
        .map(|row| CoveredVersion {
            program_id: row.get("program_id"),
            source_version_id: row.get("source_version_id"),
        })
        .collect())
}
