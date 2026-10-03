//! The `explorer` Tauri plugin. Filesystem work runs on the blocking pool:
//! a search may take its full two seconds and must not stall the runtime.

use super::index::dto::FolderIndexStatusDto;
use super::index::manager::{self, EmbedderSource, FolderIndexManager, ManagerConfig};
use super::{dto::*, fs, repository, scope::Scope};
use crate::{
    application::ports::EmbeddingPort,
    interfaces::di::Container,
    shared::{api_result::ApiError, AppError},
};
use async_trait::async_trait;
use std::sync::Arc;
use tauri::{
    plugin::{Builder, TauriPlugin},
    AppHandle, Emitter, Manager, Runtime, State,
};

/// Default and ceiling for a viewer search; the model's tool asks for fewer.
const DEFAULT_SEARCH_RESULTS: u32 = 500;
const MAX_SEARCH_RESULTS: u32 = 2_000;

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> crate::shared::Result<T> + Send + 'static,
) -> Result<T, ApiError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| {
            ApiError::from(AppError::InternalError(format!(
                "Explorer task failed: {e}"
            )))
        })?
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn explorer_resolve_root(path: String) -> Result<ExplorerRootDto, ApiError> {
    blocking(move || {
        let scope = Scope::open(&path)?;
        Ok(ExplorerRootDto {
            root: scope.root_string(),
            name: scope.name(),
        })
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn explorer_list_dir(root: String, path: String) -> Result<ExplorerListingDto, ApiError> {
    blocking(move || fs::list_dir(&Scope::open(&root)?, &path)).await
}

#[tauri::command]
#[specta::specta]
pub async fn explorer_read_file(root: String, path: String) -> Result<ExplorerFileDto, ApiError> {
    blocking(move || fs::read_file(&Scope::open(&root)?, &path)).await
}

#[tauri::command]
#[specta::specta]
pub async fn explorer_search(
    root: String,
    query: String,
    regex: bool,
    path_prefix: Option<String>,
    max_results: Option<u32>,
) -> Result<ExplorerSearchResultDto, ApiError> {
    blocking(move || {
        let scope = Scope::open(&root)?;
        fs::search(
            &scope,
            &fs::SearchRequest {
                query: &query,
                regex,
                path_prefix: path_prefix.as_deref(),
                max_results: max_results
                    .unwrap_or(DEFAULT_SEARCH_RESULTS)
                    .clamp(1, MAX_SEARCH_RESULTS) as usize,
            },
        )
    })
    .await
}

/// Binds a conversation to a folder (stored canonical) or, with `None`,
/// unbinds it.
#[tauri::command]
#[specta::specta]
pub async fn set_conversation_explorer_root(
    conversation_id: String,
    root: Option<String>,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    let canonical = match root {
        Some(root) => Some(blocking(move || Scope::open(&root).map(|s| s.root_string())).await?),
        None => None,
    };
    repository::set_conversation_root(container.db_pool(), &conversation_id, canonical.as_deref())
        .await
        .map_err(ApiError::from)
}

/// The embedder the folder index uses: the container's, so the model is
/// loaded once and shared with the chat path.
struct ContainerEmbedders<R: Runtime> {
    app: AppHandle<R>,
}

#[async_trait]
impl<R: Runtime> EmbedderSource for ContainerEmbedders<R> {
    async fn embedder(&self) -> Option<Arc<dyn EmbeddingPort>> {
        let container = self.app.try_state::<Container>()?;
        // No identity means no active embedding model: nothing to load.
        container.search.embedding_identity()?;
        container
            .get_or_load_embedding()
            .await
            .map_err(|error| tracing::warn!(%error, "Folder index has no embedder"))
            .ok()
    }
}

/// The app's folder index manager, installed on first use.
fn index_manager<R: Runtime>(app: &AppHandle<R>, container: &Container) -> Arc<FolderIndexManager> {
    manager::install(|| {
        let emitter = app.clone();
        FolderIndexManager::new(
            ManagerConfig::for_data_dir(container.core.data_dir()),
            Arc::new(ContainerEmbedders { app: app.clone() }),
            Arc::new(move |status: &FolderIndexStatusDto| {
                if let Err(error) = emitter.emit(super::index::STATUS_EVENT, status) {
                    tracing::debug!(%error, "Could not emit folder index status");
                }
            }),
        )
    })
}

/// Opens the folder's index and starts or resumes indexing it. Any other
/// open folder is closed first.
#[tauri::command]
#[specta::specta]
pub async fn explorer_index_open<R: Runtime>(
    root: String,
    app: AppHandle<R>,
    container: State<'_, Container>,
) -> Result<FolderIndexStatusDto, ApiError> {
    index_manager(&app, &container)
        .open(&root)
        .await
        .map_err(ApiError::from)
}

/// Closes the open folder's index: its watcher, task and database.
#[tauri::command]
#[specta::specta]
pub async fn explorer_index_close<R: Runtime>(
    app: AppHandle<R>,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    index_manager(&app, &container).close().await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn explorer_index_status<R: Runtime>(
    root: String,
    app: AppHandle<R>,
    container: State<'_, Container>,
) -> Result<FolderIndexStatusDto, ApiError> {
    index_manager(&app, &container)
        .status(&root)
        .await
        .map_err(ApiError::from)
}

/// Wipes the folder's index and starts over.
#[tauri::command]
#[specta::specta]
pub async fn explorer_index_rebuild<R: Runtime>(
    root: String,
    app: AppHandle<R>,
    container: State<'_, Container>,
) -> Result<FolderIndexStatusDto, ApiError> {
    index_manager(&app, &container)
        .rebuild(&root)
        .await
        .map_err(ApiError::from)
}

/// Deletes the folder's index directory, closing it first if it is open.
#[tauri::command]
#[specta::specta]
pub async fn explorer_index_forget<R: Runtime>(
    root: String,
    app: AppHandle<R>,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    index_manager(&app, &container)
        .forget(&root)
        .await
        .map_err(ApiError::from)
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("explorer")
        .invoke_handler(tauri::generate_handler![
            explorer_resolve_root,
            explorer_list_dir,
            explorer_read_file,
            explorer_search,
            set_conversation_explorer_root,
            explorer_index_open,
            explorer_index_close,
            explorer_index_status,
            explorer_index_rebuild,
            explorer_index_forget
        ])
        .build()
}
