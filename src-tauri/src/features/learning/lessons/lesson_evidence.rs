//! Learner-safe verification report. Assessment claims, keys and explanations
//! stay on the backend; only teaching claims and original source passages leave.
use crate::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Row, SqlitePool};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningEvidencePassageDto {
    pub source_version_id: String,
    pub title: String,
    pub url: Option<String>,
    pub text: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub retrieval_kind: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningClaimEvidenceDto {
    pub section_index: usize,
    pub claim: String,
    pub reason: String,
    pub supporting_quote: Option<String>,
    pub passages: Vec<LearningEvidencePassageDto>,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningLessonEvidenceDto {
    pub policy: String,
    pub checked_at: i64,
    pub checker_model: String,
    pub retrieval_mode: String,
    pub embedding_model: Option<String>,
    pub content_sha256: String,
    pub claim_count: usize,
    pub executed_examples: usize,
    pub unexecuted_languages: Vec<String>,
    pub sources_current: bool,
    pub teaching_claims: Vec<LearningClaimEvidenceDto>,
}
fn db(error: sqlx::Error) -> AppError {
    AppError::Database(error.to_string())
}
fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap_or(&Value::Null)
}
fn text(value: &Value, key: &str) -> String {
    field(value, key).as_str().unwrap_or_default().to_owned()
}
fn optional(value: &Value, key: &str) -> Option<String> {
    field(value, key).as_str().map(str::to_owned)
}
fn number(value: &Value, key: &str) -> usize {
    field(value, key).as_u64().unwrap_or(0) as usize
}

pub async fn get(
    pool: &SqlitePool,
    program_id: &str,
    lesson_id: &str,
) -> Result<Option<LearningLessonEvidenceDto>> {
    let program = crate::features::learning::repository::LearningRepository::new(pool.clone())
        .get(program_id)
        .await?;
    let lesson = program
        .modules
        .iter()
        .flat_map(|m| &m.lessons)
        .find(|l| l.id == lesson_id)
        .ok_or_else(|| AppError::NotFound("Lesson not found in this course.".into()))?;
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT report_json FROM learning_lesson_verifications WHERE program_id=? AND lesson_id=?",
    )
    .bind(program_id)
    .bind(lesson_id)
    .fetch_optional(pool)
    .await
    .map_err(db)?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let report: Value = serde_json::from_str(&raw)?;
    let versions = sqlx::query("SELECT v.id,v.title,v.resolved_url,v.requested_url,v.content_sha256,s.active_version_id,s.deleted_at FROM learning_source_versions v JOIN learning_source_library s ON s.program_id=v.program_id AND s.id=v.source_id WHERE v.program_id=?").bind(program_id).fetch_all(pool).await.map_err(db)?;
    let active: HashSet<_> = versions
        .iter()
        .filter(|v| v.get::<Option<i64>, _>("deleted_at").is_none())
        .filter_map(|v| v.get::<Option<String>, _>("active_version_id"))
        .collect();
    let bindings = field(&report, "sources")
        .as_array()
        .cloned()
        .unwrap_or_default();
    let expected: HashSet<_> = bindings.iter().map(|s| text(s, "id")).collect();
    let sources_current = active == expected
        && bindings.iter().all(|s| {
            versions.iter().any(|v| {
                v.get::<String, _>("id") == text(s, "id")
                    && v.get::<String, _>("content_sha256") == text(s, "sha256")
            })
        });
    let findings = field(&report, "findings")
        .as_array()
        .cloned()
        .unwrap_or_default();
    let teaching_claims = findings
        .iter()
        .filter(|finding| number(finding, "unit") < lesson.blocks.len())
        .map(|finding| {
            let passages = field(finding, "evidence")
                .as_array()
                .into_iter()
                .flatten()
                .map(|passage| {
                    let id = text(passage, "source_id");
                    let version = versions.iter().find(|v| v.get::<String, _>("id") == id);
                    LearningEvidencePassageDto {
                        source_version_id: id,
                        title: version
                            .map(|v| v.get("title"))
                            .unwrap_or_else(|| "Executed example".into()),
                        url: version.and_then(|v| {
                            v.get::<Option<String>, _>("resolved_url")
                                .or_else(|| v.get("requested_url"))
                        }),
                        text: text(passage, "text"),
                        start_byte: number(passage, "start_byte"),
                        end_byte: number(passage, "end_byte"),
                        retrieval_kind: text(passage, "retrieval_kind"),
                    }
                })
                .collect();
            LearningClaimEvidenceDto {
                section_index: number(finding, "unit"),
                claim: text(finding, "statement"),
                reason: text(finding, "reason"),
                supporting_quote: optional(finding, "supporting_quote"),
                passages,
            }
        })
        .collect();
    Ok(Some(LearningLessonEvidenceDto {
        policy: text(&report, "policy"),
        checked_at: field(&report, "checked_at").as_i64().unwrap_or_default(),
        checker_model: text(&report, "checker_model"),
        retrieval_mode: optional(&report, "retrieval_mode").unwrap_or_else(|| "legacy".into()),
        embedding_model: optional(&report, "embedding_model"),
        content_sha256: text(&report, "content_sha256"),
        claim_count: findings.len(),
        executed_examples: field(&report, "executions").as_array().map_or(0, Vec::len),
        unexecuted_languages: field(&report, "unexecuted_languages")
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        sources_current,
        teaching_claims,
    }))
}
