use super::*;
use crate::features::learning::pack_repository::{
    DestinationPicker, LearningPackRepository, PACK_EXTENSION,
};
use crate::shared::error::{AppError, Result as AppResult};
use async_trait::async_trait;
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

/// The native dialogs behind `DestinationPicker`. Destinations outside the
/// app's folder are chosen here, never passed in by the webview, so export
/// cannot become an arbitrary write. Same rule as the backup archive's
/// destination.
struct NativeDialogs<R: Runtime> {
    app: AppHandle<R>,
}

fn dialog_failed(error: tokio::task::JoinError) -> AppError {
    AppError::InternalError(format!("The file dialog stopped unexpectedly: {error}"))
}

#[async_trait]
impl<R: Runtime> DestinationPicker for NativeDialogs<R> {
    async fn pick_file(&self, suggested_file_name: &str) -> AppResult<Option<PathBuf>> {
        let (app, suggested) = (self.app.clone(), suggested_file_name.to_owned());
        tokio::task::spawn_blocking(move || {
            app.dialog()
                .file()
                .set_title("Save course")
                .set_file_name(suggested)
                .add_filter("Lattice course", &[PACK_EXTENSION])
                .blocking_save_file()
                .and_then(|picked| picked.into_path().ok())
        })
        .await
        .map_err(dialog_failed)
    }

    async fn pick_folder(&self) -> AppResult<Option<PathBuf>> {
        let app = self.app.clone();
        tokio::task::spawn_blocking(move || {
            app.dialog()
                .file()
                .set_title("Choose a folder for your courses")
                .blocking_pick_folder()
                .and_then(|picked| picked.into_path().ok())
        })
        .await
        .map_err(dialog_failed)
    }
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
    pack_repository(&container)
        .save_as(&request, &NativeDialogs { app })
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn export_all_learning_packs<R: Runtime>(
    app: AppHandle<R>,
    request: ExportAllLearningPacksRequestDto,
    container: State<'_, Container>,
) -> Result<ExportAllLearningPacksResultDto, ApiError> {
    pack_repository(&container)
        .export_all(&request, &NativeDialogs { app })
        .await
        .map_err(ApiError::from)
}
