//! Study guide tangents: focused conversations seeded from course material.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateLearningTangentRequestDto {
    pub program_id: String,
    pub module_id: String,
    pub lesson_id: String,
    #[serde(default)]
    pub section_index: Option<i64>,
    #[serde(default)]
    pub section_title: Option<String>,
    pub selected_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListLearningTangentsRequestDto {
    #[serde(default)]
    pub program_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningTangentDto {
    pub conversation_id: String,
    pub program_id: String,
    pub program_title: String,
    pub space_id: String,
    pub module_id: String,
    pub module_title: String,
    pub lesson_id: String,
    pub lesson_title: String,
    pub section_index: Option<i64>,
    pub section_title: Option<String>,
    pub selected_text: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: String,
    pub message_count: i64,
    pub context_message_count: i64,
}
