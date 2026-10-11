//! The `explorer` Tauri plugin. Filesystem work runs on the blocking pool:
//! a search may take its full two seconds and must not stall the runtime.

use super::index::dto::FolderIndexStatusDto;
use super::index::manager::{EmbedderSource, FolderIndexManager};
use super::{dto::*, folders, fs, scope::Scope};
use crate::{
    application::ports::EmbeddingPort,
    interfaces::di::Container,
    shared::{ipc::ApiError, AppError},
};
use async_trait::async_trait;
use std::sync::Arc;
use tauri::{
    plugin::{Builder, TauriPlugin},
    AppHandle, Manager, Runtime, State,
};

/// Carries the open folder's `FolderIndexStatusDto` when the watcher's
/// updates move it. Builds report on `jobs://status`.
pub const FOLDER_CHANGED_EVENT: &str = "explorer://folder-changed";

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

/// The files in the folder a path from an answer most likely means: the path
/// itself when it exists, else files whose path ends with it, else files of
/// the same name. Empty when none match.
#[tauri::command]
#[specta::specta]
pub async fn explorer_locate_file(root: String, path: String) -> Result<Vec<String>, ApiError> {
    blocking(move || fs::locate_file(&Scope::open(&root)?, &path)).await
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
    folders::bind_thread(container.db_pool(), &conversation_id, canonical.as_deref())
        .await
        .map_err(ApiError::from)
}

/// The embedder the folder index uses: the container's, so the model is
/// loaded once and shared with the chat path.
pub(crate) struct AppEmbedders<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> AppEmbedders<R> {
    pub(crate) fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }
}

#[async_trait]
impl<R: Runtime> EmbedderSource for AppEmbedders<R> {
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

/// The app's folder index manager, which the composition root owns.
fn index_manager(container: &Container) -> Result<Arc<FolderIndexManager>, ApiError> {
    container.folder_index().cloned().ok_or_else(|| {
        ApiError::from(AppError::ServiceNotAvailable(
            "The folder index is not available.".into(),
        ))
    })
}

/// Opens the folder's index and starts or resumes indexing it. Any other
/// open folder is closed first. The folder joins the folders list, or moves
/// up it.
#[tauri::command]
#[specta::specta]
pub async fn explorer_index_open(
    root: String,
    container: State<'_, Container>,
) -> Result<FolderIndexStatusDto, ApiError> {
    let status = index_manager(&container)?
        .open(&root)
        .await
        .map_err(ApiError::from)?;
    // The list is a convenience; the index is what was asked for.
    if let Err(error) = folders::record_open(container.db_pool(), &status.root).await {
        tracing::warn!(%error, "Could not record the Explorer folder");
    }
    Ok(status)
}

/// Closes the open folder's index: its watcher, task and database.
#[tauri::command]
#[specta::specta]
pub async fn explorer_index_close(container: State<'_, Container>) -> Result<(), ApiError> {
    index_manager(&container)?.close().await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn explorer_index_status(
    root: String,
    container: State<'_, Container>,
) -> Result<FolderIndexStatusDto, ApiError> {
    index_manager(&container)?
        .status(&root)
        .await
        .map_err(ApiError::from)
}

/// Wipes the folder's index and starts over.
#[tauri::command]
#[specta::specta]
pub async fn explorer_index_rebuild(
    root: String,
    container: State<'_, Container>,
) -> Result<FolderIndexStatusDto, ApiError> {
    index_manager(&container)?
        .rebuild(&root)
        .await
        .map_err(ApiError::from)
}

/// The folders list: pinned first, then the most recently opened, each with
/// its threads and what its index holds.
#[tauri::command]
#[specta::specta]
pub async fn explorer_folders_list(
    container: State<'_, Container>,
) -> Result<ExplorerFolderListDto, ApiError> {
    let manager = index_manager(&container)?;
    folders::list(container.db_pool(), &manager)
        .await
        .map_err(ApiError::from)
}

/// Renames a folder in the list; an empty name goes back to its own.
#[tauri::command]
#[specta::specta]
pub async fn explorer_folder_rename(
    root: String,
    name: String,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    folders::rename(container.db_pool(), &root, &name)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn explorer_folder_set_pinned(
    root: String,
    pinned: bool,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    folders::set_pinned(container.db_pool(), &root, pinned)
        .await
        .map_err(ApiError::from)
}

/// Remembers the thread the folder's chat is showing, so reopening the
/// folder lands on it. The thread must be bound to the folder.
#[tauri::command]
#[specta::specta]
pub async fn explorer_folder_set_last_thread(
    root: String,
    conversation_id: String,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    folders::remember_thread(container.db_pool(), &root, &conversation_id)
        .await
        .map_err(ApiError::from)
}

/// Sets a folder's system prompt and the space its threads belong to; an
/// empty prompt is none, and General is the default space. The folder's
/// threads move to the space. Returns how many threads moved.
#[tauri::command]
#[specta::specta]
pub async fn explorer_folder_set_settings(
    root: String,
    instructions: String,
    space_id: String,
    container: State<'_, Container>,
) -> Result<u32, ApiError> {
    let moved = folders::set_settings(container.db_pool(), &root, &instructions, &space_id)
        .await
        .map_err(ApiError::from)?;
    Ok(u32::try_from(moved).unwrap_or(u32::MAX))
}

/// Deletes the folder's own index, closing it first if it is open. The
/// folder stays listed, and the next open builds the index again.
#[tauri::command]
#[specta::specta]
pub async fn explorer_folder_delete_index(
    root: String,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    let manager = index_manager(&container)?;
    folders::delete_index(&manager, &root)
        .await
        .map_err(ApiError::from)
}

/// Removes a folder from the list with its own index; with
/// `delete_threads`, its threads too. Returns how many threads were deleted.
#[tauri::command]
#[specta::specta]
pub async fn explorer_folder_remove(
    root: String,
    delete_threads: bool,
    container: State<'_, Container>,
) -> Result<u32, ApiError> {
    let manager = index_manager(&container)?;
    let deleted = folders::remove(container.inner(), &manager, &root, delete_threads)
        .await
        .map_err(ApiError::from)?;
    Ok(u32::try_from(deleted).unwrap_or(u32::MAX))
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("explorer")
        .setup(|app, _api| {
            let container = app.state::<Container>().inner().clone();
            // Registration resumes the builds the last process left running.
            if let Some(manager) = container.folder_index() {
                let window = app.clone();
                manager.on_folder_changed(Arc::new(move |status: &FolderIndexStatusDto| {
                    use tauri::Emitter;
                    if let Err(error) = window.emit(FOLDER_CHANGED_EVENT, status) {
                        tracing::debug!(%error, "Could not publish a folder index update");
                    }
                }));
                tauri::async_runtime::block_on(manager.register())?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            explorer_resolve_root,
            explorer_list_dir,
            explorer_read_file,
            explorer_locate_file,
            explorer_search,
            set_conversation_explorer_root,
            explorer_index_open,
            explorer_index_close,
            explorer_index_status,
            explorer_index_rebuild,
            explorer_folders_list,
            explorer_folder_rename,
            explorer_folder_set_pinned,
            explorer_folder_set_last_thread,
            explorer_folder_set_settings,
            explorer_folder_delete_index,
            explorer_folder_remove
        ])
        .build()
}
