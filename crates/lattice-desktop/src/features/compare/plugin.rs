//! Compare Plugin — comparison tables across documents.

use crate::features::compare::commands as compare;
use std::sync::Arc;

use crate::features::compare::di::{CompareDeps, LlmLoader};
use crate::features::compare::dto::{CompareDocumentsRequestDto, CompareTableDto};
use crate::features::compare::scoped_search::HybridScopedSearch;
use crate::interfaces::di::Container;
use lattice_core::shared::ipc::ApiError;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime, State,
};

#[tauri::command]
#[specta::specta]
pub async fn compare_documents(
    request: CompareDocumentsRequestDto,
    container: State<'_, Container>,
    load_llm: State<'_, LlmLoader>,
) -> Result<CompareTableDto, ApiError> {
    let deps = CompareDeps {
        documents: container.document_repository(),
        search: Arc::new(HybridScopedSearch::new(container.hybrid_search_use_case())),
        chunks: container.chunk_repository(),
        load_llm: load_llm.inner().clone(),
    };
    compare::compare_documents_impl(&deps, request)
        .await
        .map_err(ApiError::from)
}

/// Bind the chat model behind a loader so the pipeline never sees the
/// container. Built once at plugin setup: each load shares one container and
/// resolves the model through it at call time, so the loader never holds a
/// stale model.
fn build_llm_loader(container: Container) -> LlmLoader {
    let container = Arc::new(container);
    LlmLoader::new(move || {
        let container = Arc::clone(&container);
        Box::pin(async move { container.default_chat_llm().await })
    })
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("compare")
        .setup(|app, _api| {
            let container = app.state::<Container>().inner().clone();
            app.manage(build_llm_loader(container));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![compare_documents])
        .build()
}
