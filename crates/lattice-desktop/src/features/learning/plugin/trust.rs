//! Source trust commands (source provenance §16, §19, decision 4): read a
//! course's sources' levels and reasons (and one claim's supports, levelled
//! for its subject), set, clear and list overrides, show the calibrated
//! bars, judge a publisher or a claim's profile again, and export the
//! user's overrides as provenance eval examples.
//!
//! The work is `references::trust`'s; these commands only assemble its
//! dependencies, pin the Learning model where a judgement is asked for, and
//! run the save dialog.

use super::*;

use crate::features::learning::lessons::claim_reviews::trust_overrides;
use crate::features::learning::references::trust::{
    calibration_view, eval_export, judgements, override_changes, source_view,
    SqliteSourceTrustStore,
};
use lattice_core::application::ports::llm_port::ModelRequirements;
use lattice_core::shared::error::AppError;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

/// The extension of an exported provenance eval file (§21).
const EXPORT_EXTENSION: &str = "jsonl";

fn store(container: &Container) -> SqliteSourceTrustStore {
    SqliteSourceTrustStore::new(container.db_pool().clone())
}

/// Each source's level with its reasons, standing, provenance, relations,
/// track record and the overrides that can touch it; with a claim, its
/// supports levelled for its subject and the bar for its kind. Reads
/// judgements already made and never calls a model.
#[tauri::command]
#[specta::specta]
pub async fn get_learning_source_trust(
    request: GetLearningSourceTrustRequestDto,
    container: State<'_, Container>,
) -> Result<LearningSourceTrustViewDto, ApiError> {
    Ok(trust_overrides::view(&store(&container), &request).await?)
}

/// Creates or replaces one override, re-decides the claims it touches with
/// no model call, and reports what that did.
#[tauri::command]
#[specta::specta]
pub async fn set_source_trust_override(
    request: LearningTrustOverrideInputDto,
    container: State<'_, Container>,
) -> Result<LearningTrustChangeResultDto, ApiError> {
    Ok(trust_overrides::set(&store(&container), &request).await?)
}

/// Removes the override at `scope` and re-decides the claims it touched;
/// `None` when there was none.
#[tauri::command]
#[specta::specta]
pub async fn clear_source_trust_override(
    scope: LearningTrustOverrideScopeDto,
    container: State<'_, Container>,
) -> Result<Option<LearningTrustChangeResultDto>, ApiError> {
    Ok(trust_overrides::clear(&store(&container), &scope).await?)
}

/// Every site and site-for-subject override, with the track record of the
/// claims its pages supported.
#[tauri::command]
#[specta::specta]
pub async fn list_source_trust_overrides(
    container: State<'_, Container>,
) -> Result<Vec<LearningSiteTrustOverrideDto>, ApiError> {
    Ok(override_changes::list(&store(&container)).await?)
}

/// The bars in force per claim kind and level, every automatic change and
/// the policy parameters with their derivations (§13).
#[tauri::command]
#[specta::specta]
pub async fn get_source_trust_calibration(
    container: State<'_, Container>,
) -> Result<LearningSourceTrustCalibrationDto, ApiError> {
    Ok(calibration_view::calibration(container.db_pool()).await?)
}

/// Judges a source's publisher again with the Learning model, then answers
/// with the source's trust.
#[tauri::command]
#[specta::specta]
pub async fn reassess_learning_source_trust(
    request: ReassessLearningSourceTrustRequestDto,
    container: State<'_, Container>,
) -> Result<LearningSourceTrustViewDto, ApiError> {
    // One standing call, its answer sized to the window it gets.
    let pin = learning_model(
        &container,
        ModelRequirements::default(),
        "Assessing a source's publisher",
    )
    .await?;
    let trust = store(&container);
    judgements::reassess_source(
        &trust,
        pin.llm.as_ref(),
        &request.program_id,
        &request.source_id,
    )
    .await?;
    Ok(source_view::view(
        &trust,
        &GetLearningSourceTrustRequestDto {
            program_id: request.program_id,
            source_ids: vec![request.source_id],
            claim: None,
            include_impact: false,
        },
    )
    .await?)
}

/// Profiles one claim of a lesson review again with the Learning model.
#[tauri::command]
#[specta::specta]
pub async fn reprofile_learning_claim(
    request: ReprofileLearningClaimRequestDto,
    container: State<'_, Container>,
) -> Result<LearningClaimProfileDto, ApiError> {
    // One profile call, its answer sized to the window it gets.
    let pin = learning_model(
        &container,
        ModelRequirements::default(),
        "Profiling a claim",
    )
    .await?;
    Ok(judgements::reprofile_claim(
        &store(&container),
        pin.llm.as_ref(),
        &request.program_id,
        &request.claim_id,
    )
    .await?)
}

/// Writes the user's overrides as `sources-v1` eval lines to a file the user
/// picks (§21). `None` when the user dismissed the dialog.
#[tauri::command]
#[specta::specta]
pub async fn export_provenance_eval_examples<R: Runtime>(
    app: AppHandle<R>,
    container: State<'_, Container>,
) -> Result<Option<ExportedProvenanceEvalExamplesDto>, ApiError> {
    let suggested =
        eval_export::export_file_name(&chrono::Local::now().format("%Y-%m-%d").to_string());
    let picked = tokio::task::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Export provenance eval examples")
            .set_file_name(suggested)
            .add_filter("JSON Lines", &[EXPORT_EXTENSION])
            .blocking_save_file()
            .and_then(|picked| picked.into_path().ok())
    })
    .await
    .map_err(|error| {
        AppError::InternalError(format!("The file dialog stopped unexpectedly: {error}"))
    })?;
    let Some(path) = picked else {
        return Ok(None);
    };
    let examples = eval_export::export(&store(&container), &path).await?;
    Ok(Some(ExportedProvenanceEvalExamplesDto {
        path: path.to_string_lossy().into_owned(),
        examples: u32::try_from(examples).unwrap_or(u32::MAX),
    }))
}
