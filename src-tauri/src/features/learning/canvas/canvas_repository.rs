//! Durable persistence for editable Learning Studio canvases and checkpoints.
use crate::features::learning::dto::*;
use crate::features::learning::{
    operations::Operation,
    persistence::{db, hash, now},
};
use crate::shared::error::{AppError, Result};
use serde::Serialize;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

const MAX_SCENE_BYTES: usize = 5 * 1024 * 1024;
const MAX_ELEMENTS: usize = 5_000;

#[derive(Clone)]
pub struct LearningCanvasRepository {
    pool: SqlitePool,
}

#[derive(Clone)]
pub struct ValidatedCanvasScene {
    pub json: String,
    pub element_count: usize,
}

pub fn validate_scene(scene: &serde_json::Value) -> Result<ValidatedCanvasScene> {
    let encoded =
        serde_json::to_vec(scene).map_err(|error| AppError::Serialization(error.to_string()))?;
    if encoded.len() > MAX_SCENE_BYTES {
        return Err(AppError::InvalidInput(
            "Canvas scene exceeds the 5 MB limit.".into(),
        ));
    }
    let object = scene
        .as_object()
        .ok_or_else(|| invalid_scene("must be a JSON object"))?;
    if object.get("type").and_then(serde_json::Value::as_str) != Some("excalidraw") {
        return Err(invalid_scene("type must be 'excalidraw'"));
    }
    let version = object
        .get("version")
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| invalid_scene("version must be an integer"))?;
    if version <= 0 {
        return Err(invalid_scene("version must be a positive integer"));
    }
    let elements = object
        .get("elements")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| invalid_scene("elements must be an array"))?;
    if elements.len() > MAX_ELEMENTS {
        return Err(AppError::InvalidInput(
            "Canvas scene exceeds the 5,000 element limit.".into(),
        ));
    }
    let mut live_elements = 0;
    for element in elements {
        let element = element
            .as_object()
            .ok_or_else(|| invalid_scene("each element must be an object"))?;
        let id = element.get("id").and_then(serde_json::Value::as_str);
        if id.is_none_or(|value| value.trim().is_empty()) {
            return Err(invalid_scene(
                "each element must have a non-empty string id",
            ));
        }
        let element_type = element.get("type").and_then(serde_json::Value::as_str);
        if element_type.is_none_or(|value| value.trim().is_empty()) {
            return Err(invalid_scene(
                "each element must have a non-empty string type",
            ));
        }
        if element_type == Some("image") {
            return Err(AppError::InvalidInput(
                "Canvas image elements are not supported yet; remove images before saving.".into(),
            ));
        }
        if matches!(element_type, Some("embeddable" | "iframe")) {
            return Err(AppError::InvalidInput(
                "Web embeds and iframes are not supported in Learning canvases; remove embedded content before saving.".into(),
            ));
        }
        if element
            .get("isDeleted")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        {
            live_elements += 1;
        }
    }
    if !object
        .get("appState")
        .is_some_and(serde_json::Value::is_object)
    {
        return Err(invalid_scene("appState must be an object"));
    }
    let files = object
        .get("files")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| invalid_scene("files must be an object"))?;
    if !files.is_empty() {
        return Err(AppError::InvalidInput(
            "Canvas image files are not supported yet; remove embedded files before saving.".into(),
        ));
    }
    let json =
        String::from_utf8(encoded).map_err(|error| AppError::Serialization(error.to_string()))?;
    Ok(ValidatedCanvasScene {
        json,
        element_count: live_elements,
    })
}

fn invalid_scene(reason: &str) -> AppError {
    AppError::InvalidInput(format!("Invalid Excalidraw scene: {reason}."))
}

fn validate_uuid(id: &str, label: &str) -> Result<()> {
    uuid::Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| AppError::InvalidInput(format!("Invalid {label} UUID.")))
}

fn validate_text(value: &str, label: &str, max: usize, required: bool) -> Result<()> {
    let count = value.trim().chars().count();
    if count > max || (required && count == 0) {
        return Err(AppError::InvalidInput(format!(
            "{label} must contain {}–{max} characters.",
            if required { 1 } else { 0 }
        )));
    }
    Ok(())
}

impl LearningCanvasRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn workspace(&self, program_id: &str) -> Result<LearningCanvasWorkspaceDto> {
        validate_uuid(program_id, "program")?;
        let exists: i64 = sqlx::query_scalar("SELECT count(*) FROM learning_programs WHERE id=?")
            .bind(program_id)
            .fetch_one(&self.pool)
            .await
            .map_err(db)?;
        if exists == 0 {
            return Err(AppError::NotFound("Learning program not found.".into()));
        }
        let rows = sqlx::query(
            "SELECT * FROM learning_canvases WHERE program_id=? ORDER BY updated_at DESC,id",
        )
        .bind(program_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        let mut canvases = Vec::with_capacity(rows.len());
        for row in rows {
            let canvas_id: String = row.get("id");
            let snapshots = self.snapshots(program_id, &canvas_id).await?;
            canvases.push(canvas_from_row(&row, snapshots)?);
        }
        Ok(LearningCanvasWorkspaceDto {
            program_id: program_id.into(),
            canvases,
        })
    }

    async fn snapshots(
        &self,
        program_id: &str,
        canvas_id: &str,
    ) -> Result<Vec<LearningCanvasSnapshotDto>> {
        let rows = sqlx::query("SELECT * FROM learning_canvas_snapshots WHERE program_id=? AND canvas_id=? ORDER BY created_at,id")
            .bind(program_id).bind(canvas_id).fetch_all(&self.pool).await.map_err(db)?;
        rows.iter().map(snapshot_from_row).collect()
    }

    pub async fn create(
        &self,
        request: &CreateLearningCanvasRequestDto,
        scene: &ValidatedCanvasScene,
    ) -> Result<()> {
        validate_uuid(&request.operation_id, "operation")?;
        validate_uuid(&request.canvas_id, "canvas")?;
        validate_uuid(&request.program_id, "program")?;
        if let Some(lesson_id) = request.lesson_id.as_deref() {
            validate_uuid(lesson_id, "lesson")?;
        }
        validate_text(&request.title, "Canvas title", 120, true)?;
        validate_text(&request.description, "Canvas description", 2000, false)?;
        let payload_hash = hash(&(request, &scene.json))?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        lock_active_program(&mut tx, &request.program_id).await?;
        if operation(
            &request.operation_id,
            &request.program_id,
            &request.canvas_id,
            "create",
            &payload_hash,
        )
        .seen(&mut *tx)
        .await?
        {
            tx.commit().await.map_err(db)?;
            return Ok(());
        }
        validate_active_program(&mut tx, &request.program_id, request.lesson_id.as_deref()).await?;
        let canvas_exists: i64 =
            sqlx::query_scalar("SELECT count(*) FROM learning_canvases WHERE id=?")
                .bind(&request.canvas_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?;
        if canvas_exists != 0 {
            return Err(AppError::InvalidInput(
                "Canvas ID is already in use.".into(),
            ));
        }
        let created_at = now();
        sqlx::query("INSERT INTO learning_canvases(id,program_id,lesson_id,title,description,scene_json,element_count,revision,created_at,updated_at) VALUES(?,?,?,?,?,?,?,0,?,?)")
            .bind(&request.canvas_id).bind(&request.program_id).bind(&request.lesson_id).bind(request.title.trim()).bind(request.description.trim()).bind(&scene.json).bind(scene.element_count as i64).bind(created_at).bind(created_at)
            .execute(&mut *tx).await.map_err(db)?;
        operation(
            &request.operation_id,
            &request.program_id,
            &request.canvas_id,
            "create",
            &payload_hash,
        )
        .record(
            &mut *tx,
            &CanvasOperationResult {
                revision: 0,
                snapshot_id: None,
            },
        )
        .await?;
        tx.commit().await.map_err(db)
    }

    pub async fn save(
        &self,
        request: &SaveLearningCanvasRequestDto,
        scene: &ValidatedCanvasScene,
    ) -> Result<()> {
        validate_uuid(&request.operation_id, "operation")?;
        validate_uuid(&request.canvas_id, "canvas")?;
        validate_uuid(&request.program_id, "program")?;
        validate_text(&request.title, "Canvas title", 120, true)?;
        validate_text(&request.description, "Canvas description", 2000, false)?;
        if request.expected_revision < 0 {
            return Err(AppError::InvalidInput(
                "Expected canvas revision cannot be negative.".into(),
            ));
        }
        let payload_hash = hash(&(request, &scene.json))?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        lock_active_program(&mut tx, &request.program_id).await?;
        if operation(
            &request.operation_id,
            &request.program_id,
            &request.canvas_id,
            "save",
            &payload_hash,
        )
        .seen(&mut *tx)
        .await?
        {
            tx.commit().await.map_err(db)?;
            return Ok(());
        }
        validate_active_program(&mut tx, &request.program_id, None).await?;
        let next = request
            .expected_revision
            .checked_add(1)
            .ok_or_else(|| AppError::InvalidInput("Canvas revision is out of range.".into()))?;
        let result = sqlx::query("UPDATE learning_canvases SET title=?,description=?,scene_json=?,element_count=?,revision=?,updated_at=? WHERE program_id=? AND id=? AND revision=?")
            .bind(request.title.trim()).bind(request.description.trim()).bind(&scene.json).bind(scene.element_count as i64).bind(next).bind(now()).bind(&request.program_id).bind(&request.canvas_id).bind(request.expected_revision)
            .execute(&mut *tx).await.map_err(db)?;
        if result.rows_affected() != 1 {
            return Err(AppError::InvalidInput(
                "Canvas changed; reload and retry.".into(),
            ));
        }
        operation(
            &request.operation_id,
            &request.program_id,
            &request.canvas_id,
            "save",
            &payload_hash,
        )
        .record(
            &mut *tx,
            &CanvasOperationResult {
                revision: next,
                snapshot_id: None,
            },
        )
        .await?;
        tx.commit().await.map_err(db)
    }

    pub async fn create_snapshot(
        &self,
        request: &CreateLearningCanvasSnapshotRequestDto,
    ) -> Result<()> {
        validate_uuid(&request.operation_id, "operation")?;
        validate_uuid(&request.snapshot_id, "snapshot")?;
        validate_uuid(&request.program_id, "program")?;
        validate_uuid(&request.canvas_id, "canvas")?;
        if request.expected_revision < 0 {
            return Err(AppError::InvalidInput(
                "Expected canvas revision cannot be negative.".into(),
            ));
        }
        validate_text(&request.name, "Snapshot name", 120, true)?;
        let payload_hash = hash(request)?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        lock_active_program(&mut tx, &request.program_id).await?;
        if operation(
            &request.operation_id,
            &request.program_id,
            &request.canvas_id,
            "snapshot",
            &payload_hash,
        )
        .seen(&mut *tx)
        .await?
        {
            tx.commit().await.map_err(db)?;
            return Ok(());
        }
        validate_active_program(&mut tx, &request.program_id, None).await?;
        let row = canvas_for_update(&mut tx, &request.program_id, &request.canvas_id).await?;
        let revision: i64 = row.get("revision");
        if revision != request.expected_revision {
            return Err(AppError::InvalidInput(
                "Canvas changed; reload and retry.".into(),
            ));
        }
        let snapshot_exists: i64 =
            sqlx::query_scalar("SELECT count(*) FROM learning_canvas_snapshots WHERE id=?")
                .bind(&request.snapshot_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?;
        if snapshot_exists != 0 {
            return Err(AppError::InvalidInput(
                "Snapshot ID is already in use.".into(),
            ));
        }
        insert_snapshot(
            &mut tx,
            &request.snapshot_id,
            &request.program_id,
            &request.canvas_id,
            request.name.trim(),
            &row,
        )
        .await?;
        operation(
            &request.operation_id,
            &request.program_id,
            &request.canvas_id,
            "snapshot",
            &payload_hash,
        )
        .record(
            &mut *tx,
            &CanvasOperationResult {
                revision,
                snapshot_id: Some(&request.snapshot_id),
            },
        )
        .await?;
        tx.commit().await.map_err(db)
    }

    pub async fn restore(&self, request: &RestoreLearningCanvasSnapshotRequestDto) -> Result<()> {
        validate_uuid(&request.operation_id, "operation")?;
        validate_uuid(&request.pre_restore_snapshot_id, "pre-restore snapshot")?;
        validate_uuid(&request.snapshot_id, "snapshot")?;
        validate_uuid(&request.program_id, "program")?;
        validate_uuid(&request.canvas_id, "canvas")?;
        if request.pre_restore_snapshot_id == request.snapshot_id {
            return Err(AppError::InvalidInput(
                "Pre-restore and target snapshot IDs must differ.".into(),
            ));
        }
        if request.expected_revision < 0 {
            return Err(AppError::InvalidInput(
                "Expected canvas revision cannot be negative.".into(),
            ));
        }
        let payload_hash = hash(request)?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        lock_active_program(&mut tx, &request.program_id).await?;
        if operation(
            &request.operation_id,
            &request.program_id,
            &request.canvas_id,
            "restore",
            &payload_hash,
        )
        .seen(&mut *tx)
        .await?
        {
            tx.commit().await.map_err(db)?;
            return Ok(());
        }
        validate_active_program(&mut tx, &request.program_id, None).await?;
        let canvas = canvas_for_update(&mut tx, &request.program_id, &request.canvas_id).await?;
        let revision: i64 = canvas.get("revision");
        if revision != request.expected_revision {
            return Err(AppError::InvalidInput(
                "Canvas changed; reload and retry.".into(),
            ));
        }
        let snapshot = sqlx::query(
            "SELECT * FROM learning_canvas_snapshots WHERE program_id=? AND canvas_id=? AND id=?",
        )
        .bind(&request.program_id)
        .bind(&request.canvas_id)
        .bind(&request.snapshot_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Canvas snapshot not found.".into()))?;
        let pre_snapshot_exists: i64 =
            sqlx::query_scalar("SELECT count(*) FROM learning_canvas_snapshots WHERE id=?")
                .bind(&request.pre_restore_snapshot_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?;
        if pre_snapshot_exists != 0 {
            return Err(AppError::InvalidInput(
                "Pre-restore snapshot ID is already in use.".into(),
            ));
        }
        let name = format!("Before restore · {}", &request.operation_id[..8]);
        insert_snapshot(
            &mut tx,
            &request.pre_restore_snapshot_id,
            &request.program_id,
            &request.canvas_id,
            &name,
            &canvas,
        )
        .await?;
        let next = revision
            .checked_add(1)
            .ok_or_else(|| AppError::InvalidInput("Canvas revision is out of range.".into()))?;
        let updated = sqlx::query("UPDATE learning_canvases SET title=?,description=?,scene_json=?,element_count=?,revision=?,updated_at=? WHERE program_id=? AND id=? AND revision=?")
            .bind(snapshot.get::<String,_>("title")).bind(snapshot.get::<String,_>("description")).bind(snapshot.get::<String,_>("scene_json")).bind(snapshot.get::<i64,_>("element_count")).bind(next).bind(now()).bind(&request.program_id).bind(&request.canvas_id).bind(revision)
            .execute(&mut *tx).await.map_err(db)?;
        if updated.rows_affected() != 1 {
            return Err(AppError::InvalidInput(
                "Canvas changed; reload and retry.".into(),
            ));
        }
        operation(
            &request.operation_id,
            &request.program_id,
            &request.canvas_id,
            "restore",
            &payload_hash,
        )
        .record(
            &mut *tx,
            &CanvasOperationResult {
                revision: next,
                snapshot_id: Some(&request.pre_restore_snapshot_id),
            },
        )
        .await?;
        tx.commit().await.map_err(db)
    }
}

async fn validate_active_program(
    tx: &mut Transaction<'_, Sqlite>,
    program_id: &str,
    lesson_id: Option<&str>,
) -> Result<()> {
    let status: Option<String> =
        sqlx::query_scalar("SELECT status FROM learning_programs WHERE id=?")
            .bind(program_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(db)?;
    match status.as_deref() {
        None => return Err(AppError::NotFound("Learning program not found.".into())),
        Some("active") => {}
        Some(_) => {
            return Err(AppError::InvalidInput(
                "Accept the learning program before editing canvases.".into(),
            ))
        }
    }
    if let Some(lesson_id) = lesson_id {
        let exists: i64 =
            sqlx::query_scalar("SELECT count(*) FROM learning_lessons WHERE program_id=? AND id=?")
                .bind(program_id)
                .bind(lesson_id)
                .fetch_one(&mut **tx)
                .await
                .map_err(db)?;
        if exists == 0 {
            return Err(AppError::NotFound(
                "Lesson does not belong to this learning program.".into(),
            ));
        }
    }
    Ok(())
}

/// Upgrade SQLite's deferred transaction to a writer before checking operation IDs.
/// Concurrent retries then wait for the winning transaction and observe its receipt.
async fn lock_active_program(tx: &mut Transaction<'_, Sqlite>, program_id: &str) -> Result<()> {
    let result =
        sqlx::query("UPDATE learning_programs SET status=status WHERE id=? AND status='active'")
            .bind(program_id)
            .execute(&mut **tx)
            .await
            .map_err(db)?;
    if result.rows_affected() == 1 {
        return Ok(());
    }
    let status: Option<String> =
        sqlx::query_scalar("SELECT status FROM learning_programs WHERE id=?")
            .bind(program_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(db)?;
    match status.as_deref() {
        None => Err(AppError::NotFound("Learning program not found.".into())),
        Some(_) => Err(AppError::InvalidInput(
            "Accept the learning program before editing canvases.".into(),
        )),
    }
}

fn operation<'a>(
    operation_id: &'a str,
    program_id: &'a str,
    canvas_id: &'a str,
    kind: &'a str,
    payload_hash: &'a str,
) -> Operation<'a> {
    Operation {
        id: operation_id,
        scope: "canvas",
        kind,
        program_id: Some(program_id),
        subject_id: Some(canvas_id),
        payload_hash,
        conflict: "Canvas operation ID was already used with different request data.",
    }
}

/// What a canvas operation produced: the canvas revision it left, and the
/// snapshot it wrote when it wrote one.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CanvasOperationResult<'a> {
    revision: i64,
    snapshot_id: Option<&'a str>,
}

async fn canvas_for_update(
    tx: &mut Transaction<'_, Sqlite>,
    program_id: &str,
    canvas_id: &str,
) -> Result<sqlx::sqlite::SqliteRow> {
    sqlx::query("SELECT * FROM learning_canvases WHERE program_id=? AND id=?")
        .bind(program_id)
        .bind(canvas_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Canvas not found in this program.".into()))
}

async fn insert_snapshot(
    tx: &mut Transaction<'_, Sqlite>,
    snapshot_id: &str,
    program_id: &str,
    canvas_id: &str,
    name: &str,
    canvas: &sqlx::sqlite::SqliteRow,
) -> Result<()> {
    sqlx::query("INSERT INTO learning_canvas_snapshots(id,program_id,canvas_id,name,title,description,scene_json,element_count,canvas_revision,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)")
        .bind(snapshot_id).bind(program_id).bind(canvas_id).bind(name).bind(canvas.get::<String,_>("title")).bind(canvas.get::<String,_>("description")).bind(canvas.get::<String,_>("scene_json")).bind(canvas.get::<i64,_>("element_count")).bind(canvas.get::<i64,_>("revision")).bind(now()).execute(&mut **tx).await.map_err(db)?;
    Ok(())
}

fn canvas_from_row(
    row: &sqlx::sqlite::SqliteRow,
    snapshots: Vec<LearningCanvasSnapshotDto>,
) -> Result<LearningCanvasDto> {
    Ok(LearningCanvasDto {
        id: row.get("id"),
        program_id: row.get("program_id"),
        lesson_id: row.get("lesson_id"),
        title: row.get("title"),
        description: row.get("description"),
        scene_json: serde_json::from_str(row.get("scene_json"))
            .map_err(|error| AppError::Serialization(error.to_string()))?,
        element_count: usize::try_from(row.get::<i64, _>("element_count")).map_err(|_| {
            AppError::InvalidState("Stored canvas element count is invalid.".into())
        })?,
        revision: row.get("revision"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        snapshots,
    })
}

fn snapshot_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<LearningCanvasSnapshotDto> {
    Ok(LearningCanvasSnapshotDto {
        id: row.get("id"),
        canvas_id: row.get("canvas_id"),
        name: row.get("name"),
        title: row.get("title"),
        description: row.get("description"),
        scene_json: serde_json::from_str(row.get("scene_json"))
            .map_err(|error| AppError::Serialization(error.to_string()))?,
        element_count: usize::try_from(row.get::<i64, _>("element_count")).map_err(|_| {
            AppError::InvalidState("Stored snapshot element count is invalid.".into())
        })?,
        canvas_revision: row.get("canvas_revision"),
        created_at: row.get("created_at"),
    })
}
