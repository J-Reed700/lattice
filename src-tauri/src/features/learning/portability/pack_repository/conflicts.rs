//! What stands between a pack and this computer: a different database
//! schema, the course already being here, a replacement that cannot go
//! ahead, and records another course owns. Every conflict says which
//! policies clear it, so the renderer never re-derives these rules.
use super::{
    db, entries::CheckedPack, guards::active_job_exists, guards::PREPARING, invalid, parse,
    records::PreviewRecord, replacement::unrecoverable_reason, validate_uuid,
};
use crate::features::learning::{
    dto::LearningProgramDto,
    pack::{sections::PROGRAM_ENTRY, DecodedLearningPack},
    portability_dto::{
        ExistingAction, LearningPackChangeDto, LearningPackConflictDto, LearningPackConflictKind,
        LearningPackConflictPolicy,
    },
};
use lattice_core::shared::error::Result;
use sqlx::{Row, SqliteConnection, SqlitePool};
use std::{borrow::Cow, collections::HashSet};

impl LearningPackConflictKind {
    /// The policies under which a conflict of this kind goes away, most
    /// preferred first.
    pub fn clears_with(self) -> Vec<LearningPackConflictPolicy> {
        LearningPackConflictPolicy::ALL
            .into_iter()
            .filter(|policy| match self {
                Self::PackSchema => false,
                Self::Program => policy.on_existing() != ExistingAction::Block,
                // Raised only against a course that is here, so a policy
                // that stops at that course does not get past them either.
                Self::ProgramHistory | Self::ProgramJobs => {
                    policy.on_existing() == ExistingAction::Keep
                }
                Self::Module | Self::Lesson | Self::Question | Self::Attempt | Self::Record => {
                    policy.remaps_ids()
                }
            })
            .collect()
    }
}

fn conflict(
    kind: LearningPackConflictKind,
    existing_id: Option<String>,
    existing_title: Option<String>,
    reason: Option<String>,
) -> LearningPackConflictDto {
    LearningPackConflictDto {
        kind,
        existing_id,
        existing_title,
        reason,
        clears_with: kind.clears_with(),
    }
}

/// True when any conflict stands under `policy`.
pub(super) fn blocks(
    conflicts: &[LearningPackConflictDto],
    policy: LearningPackConflictPolicy,
) -> bool {
    conflicts
        .iter()
        .any(|conflict| !conflict.clears_with.contains(&policy))
}

/// An identifier a section would write, and how to find the course that
/// owns it here. Sections state claims without touching the database; the
/// scan runs them.
pub(super) struct IdClaim {
    kind: LearningPackConflictKind,
    /// Claims in one namespace are checked once per identifier.
    namespace: &'static str,
    /// Selects the owning program's ID first, and may select its `title`.
    owner_sql: Cow<'static, str>,
    id: String,
    title: Option<String>,
}

impl IdClaim {
    /// A record whose table names its program in `program_id`. The table
    /// and column are constants from the section rows, never pack input.
    pub(super) fn column(
        table: &'static str,
        column: &'static str,
        id: &str,
        title: Option<&str>,
    ) -> Self {
        Self {
            kind: LearningPackConflictKind::Record,
            namespace: table,
            owner_sql: Cow::Owned(format!("SELECT program_id FROM {table} WHERE {column}=?")),
            id: id.into(),
            title: title.map(str::to_owned),
        }
    }

    /// A record owned through another table, or a curriculum record whose
    /// query also selects its local `title`.
    pub(super) fn owned_by(
        kind: LearningPackConflictKind,
        namespace: &'static str,
        owner_sql: &'static str,
        id: &str,
    ) -> Self {
        Self {
            kind,
            namespace,
            owner_sql: Cow::Borrowed(owner_sql),
            id: id.into(),
            title: None,
        }
    }

    async fn conflict(
        &self,
        pool: &SqlitePool,
        program_id: &str,
    ) -> Result<Option<LearningPackConflictDto>> {
        let Some(row) = sqlx::query(&self.owner_sql)
            .bind(&self.id)
            .fetch_optional(pool)
            .await
            .map_err(db)?
        else {
            return Ok(None);
        };
        let owner: String = row.try_get(0).map_err(db)?;
        if owner == program_id {
            return Ok(None);
        }
        let title = row
            .try_get::<String, _>("title")
            .ok()
            .or_else(|| self.title.clone());
        Ok(Some(conflict(
            self.kind,
            Some(self.id.clone()),
            title,
            None,
        )))
    }
}

/// Everything a preview reports about importing `program` under `policy`:
/// the course itself, then every record another course owns. A copy remaps
/// every identifier, so its claims are read for validation only.
pub(super) async fn scan(
    pool: &SqlitePool,
    program: &LearningProgramDto,
    policy: LearningPackConflictPolicy,
    checked: CheckedPack,
) -> Result<PreviewRecord> {
    let id = &program.summary.id;
    let local_title: Option<String> =
        sqlx::query_scalar("SELECT title FROM learning_programs WHERE id=?")
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(db)?;
    let mut conflicts = Vec::new();
    let mut action = "import";
    match (local_title, policy.on_existing()) {
        (None, ExistingAction::Replace) => action = "create",
        (None, _) => {}
        (Some(title), ExistingAction::Replace) => {
            let mut connection = pool.acquire().await.map_err(db)?;
            conflicts.extend(replace_conflicts(&mut connection, id, &title).await?);
        }
        (Some(title), _) => conflicts.push(conflict(
            LearningPackConflictKind::Program,
            Some(id.clone()),
            Some(title),
            None,
        )),
    }
    if !policy.remaps_ids() {
        let mut seen = HashSet::new();
        for claim in &checked.claims {
            if seen.insert((claim.namespace, claim.id.as_str())) {
                conflicts.extend(claim.conflict(pool, id).await?);
            }
        }
    }
    Ok(PreviewRecord {
        program_id: id.clone(),
        title: program.summary.title.clone(),
        conflicts,
        changes: vec![LearningPackChangeDto {
            entity_kind: "program".into(),
            entity_id: id.clone(),
            action: action.into(),
            description: format!("Import {}", program.summary.title),
        }],
        warnings: checked.warnings,
    })
}

/// Why a replacement of the course here cannot go ahead right now: data no
/// pack can restore, or a job still running.
async fn replace_conflicts(
    connection: &mut SqliteConnection,
    program_id: &str,
    title: &str,
) -> Result<Vec<LearningPackConflictDto>> {
    let blocked = |kind, reason: String| {
        conflict(
            kind,
            Some(program_id.into()),
            Some(title.into()),
            Some(reason),
        )
    };
    let mut conflicts = Vec::new();
    if let Some(reason) = unrecoverable_reason(connection, program_id).await? {
        conflicts.push(blocked(LearningPackConflictKind::ProgramHistory, reason));
    }
    if active_job_exists(connection, program_id).await? {
        conflicts.push(blocked(
            LearningPackConflictKind::ProgramJobs,
            PREPARING.into(),
        ));
    }
    Ok(conflicts)
}

/// A pack from another build of Lattice may describe its data differently,
/// so it is shown as blocked before anything else in it is trusted. The
/// course identity is read loosely: only enough to name it in the preview.
fn schema_conflict(pack: &DecodedLearningPack, local_schema: i64) -> Result<Option<PreviewRecord>> {
    if pack.manifest.schema_version == local_schema {
        return Ok(None);
    }
    let raw: serde_json::Value = parse(
        pack.entries
            .get(PROGRAM_ENTRY)
            .ok_or_else(|| invalid("Pack is missing its program."))?,
    )?;
    let id = raw
        .pointer("/summary/id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid("Pack program has no ID."))?;
    validate_uuid(id, "program")?;
    let title = raw
        .pointer("/summary/title")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(&pack.manifest.title);
    let reason = format!(
        "This pack was made by a different build of Lattice (database schema {}; this computer has {local_schema}). Install the same Lattice version on both computers, then export the course again.",
        pack.manifest.schema_version
    );
    Ok(Some(PreviewRecord {
        program_id: id.into(),
        title: title.into(),
        conflicts: vec![conflict(
            LearningPackConflictKind::PackSchema,
            None,
            None,
            Some(reason),
        )],
        changes: vec![],
        warnings: vec![],
    }))
}

/// The blocked preview of a pack written by another build, if it is one.
pub(super) async fn schema_mismatch(
    pool: &SqlitePool,
    pack: &DecodedLearningPack,
) -> Result<Option<PreviewRecord>> {
    let local = super::guards::schema_version(&mut *pool.acquire().await.map_err(db)?).await?;
    schema_conflict(pack, local)
}

/// Apply checks the stamp again: a preview never authorizes changed data.
pub(super) async fn require_same_schema(
    pool: &SqlitePool,
    pack: &DecodedLearningPack,
) -> Result<()> {
    match schema_mismatch(pool, pack).await? {
        Some(mut blocked) => Err(invalid(
            blocked
                .conflicts
                .pop()
                .and_then(|conflict| conflict.reason)
                .unwrap_or_default(),
        )),
        None => Ok(()),
    }
}
