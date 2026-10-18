//! Canvases, one pack entry each holding the canvas row and its snapshots,
//! so no single entry has to hold every drawing in a course. Canvases are
//! learner work: they travel with the learner's evidence.
use super::{
    conflicts::IdClaim,
    db,
    entries::{fresh, ExportScope, ImportScope, SectionRows},
    importers, invalid, json, parse, value_str,
};
use crate::features::learning::{
    dto::LearningProgramDto,
    pack::{
        sections::{PackSection, CANVAS_PREFIX},
        DecodedLearningPack, LearningPackEntry, LearningPackEntryKind,
    },
};
use async_trait::async_trait;
use lattice_core::shared::error::Result;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqliteConnection};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CanvasPackSnapshot {
    pub(super) canvases: Vec<serde_json::Value>,
    pub(super) snapshots: Vec<serde_json::Value>,
}

fn canvas_path(canvas_id: &str) -> Result<String> {
    // The ID becomes a file name: keep it to the characters our IDs use.
    if canvas_id.is_empty()
        || canvas_id.len() > 200
        || !canvas_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(invalid("A canvas ID cannot be used as a pack entry name."));
    }
    Ok(format!("{CANVAS_PREFIX}{canvas_id}.json"))
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CanvasEntry {
    canvas: serde_json::Value,
    snapshots: Vec<serde_json::Value>,
}

/// One entry per canvas holding the canvas row and its snapshots.
fn encode_canvases(snapshot: &CanvasPackSnapshot) -> Result<Vec<LearningPackEntry>> {
    let mut grouped: BTreeMap<&str, CanvasEntry> = BTreeMap::new();
    for canvas in &snapshot.canvases {
        let id = value_str(canvas, "id")?;
        let entry = CanvasEntry {
            canvas: canvas.clone(),
            snapshots: Vec::new(),
        };
        if grouped.insert(id, entry).is_some() {
            return Err(invalid("Pack canvases repeat an ID."));
        }
    }
    for row in &snapshot.snapshots {
        grouped
            .get_mut(value_str(row, "canvasId")?)
            .ok_or_else(|| invalid("A canvas snapshot has no canvas in this program."))?
            .snapshots
            .push(row.clone());
    }
    grouped
        .into_iter()
        .map(|(id, entry)| {
            Ok(LearningPackEntry {
                path: canvas_path(id)?,
                kind: LearningPackEntryKind::Canvas,
                bytes: json(&entry)?,
            })
        })
        .collect()
}

/// `None` when the pack has no canvases.
fn decode_canvases(pack: &DecodedLearningPack) -> Result<Option<CanvasPackSnapshot>> {
    let mut snapshot = CanvasPackSnapshot {
        canvases: Vec::new(),
        snapshots: Vec::new(),
    };
    for (path, bytes) in &pack.entries {
        if !path.starts_with(CANVAS_PREFIX) {
            continue;
        }
        let entry: CanvasEntry = parse(bytes)?;
        let id = value_str(&entry.canvas, "id")?;
        if canvas_path(id)? != *path {
            return Err(invalid(
                "Pack canvas entry names a different canvas than its path.",
            ));
        }
        for row in &entry.snapshots {
            if value_str(row, "canvasId")? != id {
                return Err(invalid("Pack canvas snapshot belongs to another canvas."));
            }
        }
        snapshot.canvases.push(entry.canvas);
        snapshot.snapshots.extend(entry.snapshots);
    }
    Ok((!snapshot.canvases.is_empty()).then_some(snapshot))
}

/// Every canvas and canvas snapshot row of one program.
async fn read_canvases(tx: &mut SqliteConnection, program_id: &str) -> Result<CanvasPackSnapshot> {
    let canvas_rows=sqlx::query("SELECT id,lesson_id,title,description,scene_json,element_count,revision,created_at,updated_at FROM learning_canvases WHERE program_id=? ORDER BY created_at,id")
            .bind(program_id).fetch_all(&mut *tx).await.map_err(db)?;
    let canvases:Vec<serde_json::Value>=canvas_rows.into_iter().map(|r|serde_json::json!({"id":r.get::<String,_>("id"),"lessonId":r.get::<Option<String>,_>("lesson_id"),"title":r.get::<String,_>("title"),"description":r.get::<String,_>("description"),"sceneJson":r.get::<String,_>("scene_json"),"elementCount":r.get::<i64,_>("element_count"),"revision":r.get::<i64,_>("revision"),"createdAt":r.get::<i64,_>("created_at"),"updatedAt":r.get::<i64,_>("updated_at")})).collect();
    let snapshot_rows=sqlx::query("SELECT id,canvas_id,name,title,description,scene_json,element_count,canvas_revision,created_at FROM learning_canvas_snapshots WHERE program_id=? ORDER BY created_at,id")
            .bind(program_id).fetch_all(&mut *tx).await.map_err(db)?;
    let snapshots:Vec<serde_json::Value>=snapshot_rows.into_iter().map(|r|serde_json::json!({"id":r.get::<String,_>("id"),"canvasId":r.get::<String,_>("canvas_id"),"name":r.get::<String,_>("name"),"title":r.get::<String,_>("title"),"description":r.get::<String,_>("description"),"sceneJson":r.get::<String,_>("scene_json"),"elementCount":r.get::<i64,_>("element_count"),"canvasRevision":r.get::<i64,_>("canvas_revision"),"createdAt":r.get::<i64,_>("created_at")})).collect();
    Ok(CanvasPackSnapshot {
        canvases,
        snapshots,
    })
}

pub(super) struct CanvasRows;
pub(super) const CANVASES: CanvasRows = CanvasRows;

#[async_trait]
impl SectionRows for CanvasRows {
    fn section(&self) -> PackSection {
        PackSection::Canvases
    }
    async fn encode(
        &self,
        connection: &mut SqliteConnection,
        scope: &ExportScope<'_>,
    ) -> Result<Vec<LearningPackEntry>> {
        if !scope.options.include_evidence {
            return Ok(Vec::new());
        }
        encode_canvases(&read_canvases(connection, &scope.program.summary.id).await?)
    }
    fn claims(
        &self,
        pack: &DecodedLearningPack,
        _program: &LearningProgramDto,
    ) -> Result<Vec<IdClaim>> {
        let Some(snapshot) = decode_canvases(pack)? else {
            return Ok(Vec::new());
        };
        let mut claims = Vec::new();
        for (table, rows) in [
            ("learning_canvases", &snapshot.canvases),
            ("learning_canvas_snapshots", &snapshot.snapshots),
        ] {
            for row in rows {
                claims.push(IdClaim::column(table, "id", value_str(row, "id")?, None));
            }
        }
        Ok(claims)
    }
    fn map_ids(
        &self,
        pack: &DecodedLearningPack,
        _program: &LearningProgramDto,
        ids: &mut HashMap<String, String>,
    ) -> Result<()> {
        if let Some(snapshot) = decode_canvases(pack)? {
            for row in snapshot.canvases.iter().chain(&snapshot.snapshots) {
                fresh(ids, value_str(row, "id")?);
            }
        }
        Ok(())
    }
    async fn insert(
        &self,
        connection: &mut SqliteConnection,
        scope: &ImportScope<'_>,
    ) -> Result<()> {
        let Some(mut snapshot) = decode_canvases(scope.pack)? else {
            return Ok(());
        };
        importers::insert_canvas_snapshot(
            connection,
            &scope.program.summary.id,
            &mut snapshot,
            scope.ids,
        )
        .await
    }
}
