//! Learning canvases and their snapshots.
use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningCanvasSnapshotDto {
    pub id: String,
    pub canvas_id: String,
    pub name: String,
    pub title: String,
    pub description: String,
    pub scene_json: serde_json::Value,
    pub element_count: usize,
    pub canvas_revision: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningCanvasDto {
    pub id: String,
    pub program_id: String,
    pub lesson_id: Option<String>,
    pub title: String,
    pub description: String,
    pub scene_json: serde_json::Value,
    pub element_count: usize,
    pub revision: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub snapshots: Vec<LearningCanvasSnapshotDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningCanvasWorkspaceDto {
    pub program_id: String,
    pub canvases: Vec<LearningCanvasDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateLearningCanvasRequestDto {
    pub operation_id: String,
    pub canvas_id: String,
    pub program_id: String,
    pub lesson_id: Option<String>,
    pub title: String,
    pub description: String,
    pub scene_json: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveLearningCanvasRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub canvas_id: String,
    pub expected_revision: i64,
    pub title: String,
    pub description: String,
    pub scene_json: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateLearningCanvasSnapshotRequestDto {
    pub operation_id: String,
    pub snapshot_id: String,
    pub program_id: String,
    pub canvas_id: String,
    pub expected_revision: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RestoreLearningCanvasSnapshotRequestDto {
    pub operation_id: String,
    pub pre_restore_snapshot_id: String,
    pub program_id: String,
    pub canvas_id: String,
    pub snapshot_id: String,
    pub expected_revision: i64,
}
