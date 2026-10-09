//! Versioned course provenance, independent of optional learner activity.
//! Kept out of the schema-dependent aggregate table dump. Private assessment
//! findings remain backend-only and use the normal learner-safe projection.
use super::{db, invalid, json_string, parse, remap};
use crate::features::learning::{
    content_verification::LessonVerificationReport, dto::LearningProgramDto,
    outline_draft::OutlineDraft, pack::DecodedLearningPack,
};
use crate::shared::error::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection};
use std::collections::{HashMap, HashSet};

pub(super) const ENTRY: &str = "program/provenance.json";

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ProvenanceSnapshot {
    version: u32,
    outline: Option<OutlineDraft>,
    outline_checkpoints: Vec<OutlineCheckpoint>,
    lessons: Vec<LessonReceipt>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OutlineCheckpoint {
    revision: i64,
    created_at: i64,
    draft: OutlineDraft,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LessonReceipt {
    lesson_id: String,
    report: LessonVerificationReport,
}

fn validate_outline(draft: &OutlineDraft) -> Result<()> {
    let fingerprint = format!(
        "{:x}",
        Sha256::digest(draft.candidate.to_string().as_bytes())
    );
    if draft.review.content_hash != fingerprint {
        return Err(invalid(
            "Pack outline provenance has an invalid content fingerprint.",
        ));
    }
    Ok(())
}

impl ProvenanceSnapshot {
    fn validate(&self, program: &LearningProgramDto) -> Result<()> {
        if self.version != 1 {
            return Err(invalid("This course provenance version is not supported."));
        }
        let lesson_ids: HashSet<_> = program
            .modules
            .iter()
            .flat_map(|module| module.lessons.iter().map(|lesson| lesson.id.as_str()))
            .collect();
        let mut seen = HashSet::new();
        for lesson in &self.lessons {
            if !lesson_ids.contains(lesson.lesson_id.as_str()) || !seen.insert(&lesson.lesson_id) {
                return Err(invalid(
                    "Pack provenance refers to a missing or duplicate lesson.",
                ));
            }
            lesson.report.archived_metadata(&lesson.lesson_id)?;
        }
        if let Some(outline) = &self.outline {
            validate_outline(outline)?;
        }
        let mut revisions = HashSet::new();
        for checkpoint in &self.outline_checkpoints {
            if checkpoint.revision < 0 || !revisions.insert(checkpoint.revision) {
                return Err(invalid(
                    "Pack provenance has a duplicate or invalid outline revision.",
                ));
            }
            validate_outline(&checkpoint.draft)?;
        }
        Ok(())
    }
}

pub(super) fn decode(
    pack: &DecodedLearningPack,
    program: &LearningProgramDto,
) -> Result<Option<ProvenanceSnapshot>> {
    let snapshot: Option<ProvenanceSnapshot> = pack
        .entries
        .get(ENTRY)
        .map(|bytes| parse(bytes))
        .transpose()?;
    if snapshot.is_none() && pack.manifest.version >= 2 {
        return Err(invalid(
            "This v2 pack is missing its required course provenance entry.",
        ));
    }
    if let Some(snapshot) = &snapshot {
        snapshot.validate(program)?;
    }
    Ok(snapshot)
}

pub(super) async fn snapshot(
    connection: &mut SqliteConnection,
    program: &LearningProgramDto,
) -> Result<ProvenanceSnapshot> {
    let program_id = &program.summary.id;
    let outline: Option<String> =
        sqlx::query_scalar("SELECT state_json FROM learning_outline_drafts WHERE program_id=?")
            .bind(program_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(db)?;
    let outline = outline.map(|json| parse(json.as_bytes())).transpose()?;
    let rows = sqlx::query("SELECT revision,state_json,created_at FROM learning_outline_checkpoints WHERE program_id=? ORDER BY revision")
        .bind(program_id).fetch_all(&mut *connection).await.map_err(db)?;
    let outline_checkpoints = rows
        .into_iter()
        .map(|row| {
            Ok(OutlineCheckpoint {
                revision: row.get("revision"),
                created_at: row.get("created_at"),
                draft: parse(row.get::<String, _>("state_json").as_bytes())?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let rows = sqlx::query("SELECT lesson_id,policy,content_sha256,report_json,checked_at FROM learning_lesson_verifications WHERE program_id=? ORDER BY lesson_id")
        .bind(program_id).fetch_all(&mut *connection).await.map_err(db)?;
    let lessons = rows.into_iter().map(|row| {
        let lesson_id: String = row.get("lesson_id");
        let report: LessonVerificationReport = parse(row.get::<String, _>("report_json").as_bytes())?;
        let (policy, fingerprint, checked_at) = report.archived_metadata(&lesson_id)?;
        if policy != row.get::<String, _>("policy") || fingerprint != row.get::<String, _>("content_sha256") || checked_at != row.get::<i64, _>("checked_at") {
            return Err(invalid("Saved verification columns do not match their report; export was stopped without replacing the course."));
        }
        Ok(LessonReceipt { lesson_id, report })
    }).collect::<Result<Vec<_>>>()?;
    let snapshot = ProvenanceSnapshot {
        version: 1,
        outline,
        outline_checkpoints,
        lessons,
    };
    snapshot.validate(program)?;
    Ok(snapshot)
}

fn remap_outline(draft: &mut OutlineDraft, ids: &HashMap<String, String>) {
    for id in &mut draft.source_ids {
        *id = remap(ids, id);
    }
    for issue in &mut draft.review.issues {
        if let Some(id) = &mut issue.source_id {
            *id = remap(ids, id);
        }
    }
    if let Some(bindings) = &mut draft.bindings {
        for binding in bindings {
            binding.module_id = remap(ids, &binding.module_id);
            for lesson in &mut binding.lesson_ids {
                *lesson = remap(ids, lesson);
            }
        }
    }
    // Candidate prose, sourceIndex positions and historical review receipts
    // are untouched. Remapped sources cannot reuse the old context hash.
}

pub(super) async fn restore(
    connection: &mut SqliteConnection,
    program_id: &str,
    mut snapshot: ProvenanceSnapshot,
    ids: &HashMap<String, String>,
) -> Result<()> {
    if let Some(draft) = &mut snapshot.outline {
        remap_outline(draft, ids);
        sqlx::query("INSERT INTO learning_outline_drafts(program_id,state_json) VALUES(?,?)")
            .bind(program_id)
            .bind(json_string(draft)?)
            .execute(&mut *connection)
            .await
            .map_err(db)?;
    }
    for mut checkpoint in snapshot.outline_checkpoints {
        remap_outline(&mut checkpoint.draft, ids);
        sqlx::query("INSERT INTO learning_outline_checkpoints(program_id,revision,state_json,created_at) VALUES(?,?,?,?)")
            .bind(program_id).bind(checkpoint.revision).bind(json_string(&checkpoint.draft)?).bind(checkpoint.created_at)
            .execute(&mut *connection).await.map_err(db)?;
    }
    for mut lesson in snapshot.lessons {
        lesson.lesson_id = remap(ids, &lesson.lesson_id);
        lesson.report.remap_archived_ids(ids);
        let (policy, fingerprint, checked_at) =
            lesson.report.archived_metadata(&lesson.lesson_id)?;
        sqlx::query("INSERT INTO learning_lesson_verifications(lesson_id,program_id,policy,content_sha256,report_json,checked_at) VALUES(?,?,?,?,?,?)")
            .bind(&lesson.lesson_id).bind(program_id).bind(policy).bind(fingerprint).bind(json_string(&lesson.report)?).bind(checked_at)
            .execute(&mut *connection).await.map_err(db)?;
    }
    Ok(())
}
