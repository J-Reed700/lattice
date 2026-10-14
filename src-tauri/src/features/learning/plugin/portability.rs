use super::*;
use crate::features::learning::pack_repository::{pack_file_name, LearningPackRepository};
use std::path::PathBuf;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

/// The pack workspace lives under the app data directory; the container is
/// only the construction site, never threaded through the repository.
fn pack_repository(container: &Container) -> LearningPackRepository {
    let data_dir = container.core.data_dir();
    LearningPackRepository::new(
        container.db_pool().clone(),
        data_dir.join("learning-packs"),
        data_dir.clone(),
    )
}

// Destinations outside the app's folder are chosen in native dialogs opened
// here, never passed in by the webview, so export cannot become an arbitrary
// write. Same rule as the backup archive's destination.

/// Native save dialog for one pack. `None` means cancelled.
async fn pick_save_file<R: Runtime>(app: AppHandle<R>, suggested: String) -> Option<PathBuf> {
    tokio::task::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Save course")
            .set_file_name(suggested)
            .add_filter("Lattice course", &["lattice-learning"])
            .blocking_save_file()
            .and_then(|picked| picked.into_path().ok())
    })
    .await
    .unwrap_or_else(|error| {
        tracing::warn!(%error, "save dialog task failed");
        None
    })
}

/// Native folder picker for exporting every course. `None` means cancelled.
async fn pick_folder<R: Runtime>(app: AppHandle<R>) -> Option<PathBuf> {
    tokio::task::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Choose a folder for your courses")
            .blocking_pick_folder()
            .and_then(|picked| picked.into_path().ok())
    })
    .await
    .unwrap_or_else(|error| {
        tracing::warn!(%error, "folder dialog task failed");
        None
    })
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_portability_workspace(
    program_id: String,
    container: State<'_, Container>,
) -> Result<LearningPortabilityWorkspaceDto, ApiError> {
    pack_repository(&container)
        .workspace(&program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn export_learning_pack(
    request: ExportLearningPackRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPortabilityWorkspaceDto, ApiError> {
    pack_repository(&container)
        .export(&request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn preview_learning_pack_import(
    request: PreviewLearningPackImportRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPortabilityWorkspaceDto, ApiError> {
    pack_repository(&container)
        .preview(&request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn apply_learning_pack_import(
    request: ApplyLearningPackImportRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPortabilityWorkspaceDto, ApiError> {
    pack_repository(&container)
        .apply(&request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_learning_pack_import_preview(
    request: CancelLearningPackImportPreviewRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPortabilityWorkspaceDto, ApiError> {
    pack_repository(&container)
        .cancel(&request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn save_learning_pack_as<R: Runtime>(
    app: AppHandle<R>,
    request: ExportLearningPackRequestDto,
    container: State<'_, Container>,
) -> Result<SaveLearningPackAsResultDto, ApiError> {
    let packs = pack_repository(&container);
    // A retried request answers with what it already wrote, without a dialog.
    if let Some(export) = packs.replayed_export(&request).await? {
        return Ok(SaveLearningPackAsResultDto {
            cancelled: false,
            export: Some(export),
        });
    }
    packs.check_exportable(&request.program_id).await?;
    let suggested = pack_file_name(
        request
            .file_name
            .trim()
            .trim_end_matches(".lattice-learning"),
    );
    let Some(chosen) = pick_save_file(app, suggested).await else {
        return Ok(SaveLearningPackAsResultDto {
            cancelled: true,
            export: None,
        });
    };
    let export = packs.save_as(&request, &chosen).await?;
    Ok(SaveLearningPackAsResultDto {
        cancelled: false,
        export: Some(export),
    })
}

#[tauri::command]
#[specta::specta]
pub async fn export_all_learning_packs<R: Runtime>(
    app: AppHandle<R>,
    request: ExportAllLearningPacksRequestDto,
    container: State<'_, Container>,
) -> Result<ExportAllLearningPacksResultDto, ApiError> {
    let packs = pack_repository(&container);
    if let Some(done) = packs.replayed_export_all(&request).await? {
        return Ok(done);
    }
    let Some(folder) = pick_folder(app).await else {
        return Ok(ExportAllLearningPacksResultDto {
            cancelled: true,
            destination_dir: None,
            exported: Vec::new(),
            failed: Vec::new(),
        });
    };
    packs
        .export_all(&request, &folder)
        .await
        .map_err(ApiError::from)
}
