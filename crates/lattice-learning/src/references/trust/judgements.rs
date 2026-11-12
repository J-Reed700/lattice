//! The two judgements the user can ask for again (§12, §19): "Assess" a
//! source's publisher, and "Re-profile" a claim. Each runs lattice-core's
//! judgement service with the Learning model and replaces the cached answer
//! for its key, so the next read, and the gate's next decision, use it.
//! Ownership of a subject is judged when a claim needs it, by the gate.

use super::dto_map;
use super::repository::judgements::{active_version_id, claim_context, ClaimContext};
use super::repository::{unix_seconds, SqliteSourceTrustStore};
use crate::dto::LearningClaimProfileDto;
use crate::lessons::review_findings::FindingSite;
use crate::source_library::LearningSourceLibraryRepository;
use lattice_core::application::ports::{LLMPort, SourceTrustStore, StoredJudgement};
use lattice_core::application::services::source_trust::{
    claim_profile_key, judge_standing, profile_claims, standing_key, ClaimToProfile, ProfileRequest,
};
use lattice_core::domain::provenance::{
    known_classes, AddedBy, Chosen, ContentEvidence, Judged, JudgementKey, ProvenanceRecord,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Database;

fn stored<T>(key: JudgementKey, judged: Judged<T>) -> StoredJudgement<T> {
    StoredJudgement {
        key,
        value: judged.value,
        input_tokens: judged.input_tokens,
        output_tokens: judged.output_tokens,
        assessed_at: unix_seconds(),
    }
}

/// The content evidence a version's `source-standing-v2` judgement is keyed
/// by (claim sources design §4.3): its saved text, its recorded links and
/// origin, and how the course got it. `None` when the version is not in the
/// program.
pub(crate) async fn content_evidence(
    database: &Database,
    program_id: &str,
    record: &ProvenanceRecord,
) -> Result<Option<ContentEvidence>> {
    let Some(version) = LearningSourceLibraryRepository::new(database.clone())
        .chosen_version(program_id, &record.source_version_id)
        .await?
    else {
        return Ok(None);
    };
    let by = AddedBy::from_column(version.added_by.as_str()).ok_or_else(|| {
        AppError::InternalError(format!(
            "Unknown source origin \"{}\".",
            version.added_by.as_str()
        ))
    })?;
    Ok(Some(ContentEvidence::of(
        &version.text,
        &record.links,
        &record.origin,
        Chosen {
            by,
            note: version.selection_note,
        },
    )))
}

/// Judges the standing of a source's active version again, with the
/// origin's current track record (§12.1: the track record is an input, not
/// part of the key, so only this re-judges after new verdicts).
pub async fn reassess_source(
    database: &Database,
    store: &SqliteSourceTrustStore,
    llm: &dyn LLMPort,
    program_id: &str,
    source_id: &str,
) -> Result<()> {
    let version = active_version_id(database, program_id, source_id).await?;
    let version = version
        .ok_or_else(|| AppError::NotFound("This source has no saved version to assess.".into()))?;
    let record = store.provenance(&version).await?.ok_or_else(|| {
        AppError::InvalidState(
            "No provenance was recorded when this source was saved. Refresh it to record its publisher, then assess it."
                .into(),
        )
    })?;
    let content = content_evidence(database, program_id, &record)
        .await?
        .ok_or_else(|| AppError::NotFound("This source has no saved version to assess.".into()))?;
    let track_record = store.track_record(&record.origin.key).await?;
    let judged = judge_standing(llm, &record.signals, &content, track_record).await?;
    store
        .save_standing(&stored(
            standing_key(&record.signals, &content, llm.model_name()),
            judged,
        ))
        .await
}

/// Profiles one claim of a course's lesson review again, under the subject classes
/// the user's overrides name now (§12.3), and saves it over the cached one.
/// Answers with the new profile.
pub async fn reprofile_claim(
    database: &Database,
    store: &SqliteSourceTrustStore,
    llm: &dyn LLMPort,
    program_id: &str,
    claim_id: &str,
) -> Result<LearningClaimProfileDto> {
    let context = claim_context(database, program_id, claim_id)
        .await?
        .ok_or_else(|| {
            AppError::NotFound(
                "This statement is not in any of the course's lesson reviews.".into(),
            )
        })?;
    let ClaimContext {
        statement,
        unit_index,
        sections,
        lesson_title,
        goal,
    } = context;
    let classes = known_classes(&store.site_overrides().await?);
    // The same unit title and course topic the preparation job gives the
    // profile call (`provenance_gate::context`, `generation_jobs`).
    let unit_title = FindingSite::of_unit(
        usize::try_from(unit_index).map_err(|_| AppError::InternalError("A statement's unit is out of range.".into()))?,
        usize::try_from(sections).map_err(|_| AppError::InternalError("A lesson's section count is out of range.".into()))?,
    )
    .label();
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
            "The model profiled a different number of statements than it was asked for.".into(),
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
