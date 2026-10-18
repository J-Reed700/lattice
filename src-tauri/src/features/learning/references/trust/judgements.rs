//! The two judgements the user can ask for again (§12, §19): "Assess" a
//! source's publisher, and "Re-profile" a claim. Each runs lattice-core's
//! judgement service with the Learning model and replaces the cached answer
//! for its key, so the next read, and the gate's next decision, use it.
//! Ownership of a subject is judged when a claim needs it, by the gate.

use super::dto_map;
use super::repository::{unix_seconds, SqliteSourceTrustStore};
use crate::features::learning::dto::LearningClaimProfileDto;
use crate::features::learning::persistence::db;
use lattice_core::application::ports::{LLMPort, SourceTrustStore, StoredJudgement};
use lattice_core::application::services::source_trust::{
    claim_profile_key, judge_standing, profile_claims, standing_key, ClaimToProfile, ProfileRequest,
};
use lattice_core::domain::provenance::{known_classes, Judged, JudgementKey};
use lattice_core::shared::error::{AppError, Result};
use sqlx::Row;

fn stored<T>(key: JudgementKey, judged: Judged<T>) -> StoredJudgement<T> {
    StoredJudgement {
        key,
        value: judged.value,
        input_tokens: judged.input_tokens,
        output_tokens: judged.output_tokens,
        assessed_at: unix_seconds(),
    }
}

/// Judges the standing of a source's active version again, with the
/// origin's current track record (§12.1: the track record is an input, not
/// part of the key, so only this re-judges after new verdicts).
pub async fn reassess_source(
    store: &SqliteSourceTrustStore,
    llm: &dyn LLMPort,
    program_id: &str,
    source_id: &str,
) -> Result<()> {
    let version: Option<String> = sqlx::query_scalar(
        "SELECT active_version_id FROM learning_source_library WHERE program_id=? AND id=? AND deleted_at IS NULL",
    )
    .bind(program_id)
    .bind(source_id)
    .fetch_optional(store.pool())
    .await
    .map_err(db)?
    .flatten();
    let version = version
        .ok_or_else(|| AppError::NotFound("This source has no saved version to assess.".into()))?;
    let record = store.provenance(&version).await?.ok_or_else(|| {
        AppError::InvalidState(
            "No provenance was recorded when this source was saved. Refresh it to record its publisher, then assess it."
                .into(),
        )
    })?;
    let track_record = store.track_record(&record.origin.key).await?;
    let judged = judge_standing(llm, &record.signals, track_record).await?;
    store
        .save_standing(&stored(
            standing_key(&record.signals, llm.model_name()),
            judged,
        ))
        .await
}

/// Profiles one claim of a course's lesson review again, under the subject classes
/// the user's overrides name now (§12.3), and saves it over the cached one.
/// Answers with the new profile.
pub async fn reprofile_claim(
    store: &SqliteSourceTrustStore,
    llm: &dyn LLMPort,
    program_id: &str,
    claim_id: &str,
) -> Result<LearningClaimProfileDto> {
    let row = sqlx::query(
        "SELECT c.statement, c.unit_index, l.title AS lesson_title, p.goal \
         FROM learning_lesson_claims c \
         JOIN learning_lesson_reviews r ON r.lesson_id = c.lesson_id \
         JOIN learning_lessons l ON l.id = c.lesson_id AND l.program_id = r.program_id \
         JOIN learning_programs p ON p.id = r.program_id \
         WHERE c.claim_id=? AND r.program_id=?",
    )
    .bind(claim_id)
    .bind(program_id)
    .fetch_optional(store.pool())
    .await
    .map_err(db)?
    .ok_or_else(|| {
        AppError::NotFound("This claim is not in any of the course's lesson reviews.".into())
    })?;
    let statement: String = row.try_get("statement").map_err(db)?;
    let unit_index: i64 = row.try_get("unit_index").map_err(db)?;
    let lesson_title: String = row.try_get("lesson_title").map_err(db)?;
    let goal: String = row.try_get("goal").map_err(db)?;
    let classes = known_classes(&store.site_overrides().await?);
    // The same unit title and course topic the preparation job gives the
    // profile call (`provenance_gate::context`, `generation_jobs`).
    let unit_title = format!("Section {}", unit_index.saturating_add(1));
    let topic = format!("{goal}: {lesson_title}");
    let claims = [ClaimToProfile {
        id: "claim-0".into(),
        statement: statement.clone(),
    }];
    let mut judged = profile_claims(
        llm,
        &ProfileRequest {
            unit_title: &unit_title,
            course_topic: &topic,
            known_classes: &classes,
            claims: &claims,
        },
    )
    .await?;
    if judged.len() != claims.len() {
        return Err(AppError::InternalError(
            "The claim profile call answered for a different number of claims.".into(),
        ));
    }
    let judged = judged.remove(0);
    let profile = dto_map::profile(&judged.value);
    store
        .save_claim_profiles(&[stored(
            claim_profile_key(&statement, &classes, llm.model_name()),
            judged,
        )])
        .await?;
    Ok(profile)
}
