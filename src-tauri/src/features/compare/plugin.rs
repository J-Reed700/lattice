//! Compare Plugin — comparison tables across documents.

use crate::features::compare::commands as compare;
use crate::features::compare::dto::{CompareDocumentsRequestDto, CompareTableDto};
use crate::interfaces::di::Container;
use crate::shared::api_result::ApiError;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, State,
};

#[tauri::command]
#[specta::specta]
pub async fn compare_documents(
    request: CompareDocumentsRequestDto,
    container: State<'_, Container>,
) -> Result<CompareTableDto, ApiError> {
    compare::compare_documents_impl(container.inner(), request)
        .await
        .map_err(ApiError::from)
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("compare")
        .invoke_handler(tauri::generate_handler![compare_documents])
        .build()
}
