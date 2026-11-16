//! The composition the desktop builds, with no window: for the evals and the
//! headless runs that drive the app's own commands.

use super::Container;
use crate::features::settings::repository::SettingsRepository;
use crate::infrastructure::passage_index::EmbedderSource;
use crate::infrastructure::setup::database::setup_database;
use crate::interfaces::di::modules::standalone::StandaloneServers;
use async_trait::async_trait;
use lattice_core::application::contracts::settings::SettingsDto;
use lattice_core::application::ports::{EmbeddingPort, SettingsRepositoryPort};
use lattice_core::shared::error::Result;
use std::path::Path;
use std::sync::Arc;

/// The container's own embedder for the passage indexes, as `AppEmbedders`
/// reaches it in the app.
struct ContainerEmbedders(Container);

#[async_trait]
impl EmbedderSource for ContainerEmbedders {
    async fn embedder(&self) -> Option<Arc<dyn EmbeddingPort>> {
        self.0.passage_embedder().await
    }

    async fn identity(&self) -> Option<String> {
        self.0.passage_embedder_identity()
    }
}

/// Writes the settings file, then builds the container on a new database,
/// in the order the desktop's startup does. With no app to start sidecars,
/// the container serves embeddings, and the reranker in `reranker_gguf` when
/// there is one, from llama-servers it spawns itself. The folder and course
/// indexes are attached, as `with_app_handle` attaches them.
pub async fn open(
    data_dir: &Path,
    settings: &SettingsDto,
    reranker_gguf: Option<&Path>,
) -> Result<Container> {
    SettingsRepository::new(data_dir.to_path_buf())
        .await?
        .save_all(settings)
        .await?;
    let connection = setup_database(data_dir.join("lattice.db")).await?;
    let container = Container::new(
        connection.database().clone(),
        None,
        &settings.llm.ollama_url,
        &settings.llm.model,
        data_dir.to_path_buf(),
        Arc::default(),
    )
    .await?;
    let servers = Arc::new(StandaloneServers::from_env());
    let container = container.with_embedding_starter(servers.clone());
    let container = match reranker_gguf {
        Some(gguf) => container.with_reranker(gguf, servers).await,
        None => container,
    };
    // The embedders hold the container as it is before the indexes exist, so
    // the indexes it then owns do not point back at themselves.
    let embedders = Arc::new(ContainerEmbedders(container.clone()));
    Ok(container.with_passage_indexes(embedders))
}

/// The bare app's identifier, so its browser data stays apart from the
/// installed app's.
#[cfg(all(test, any(windows, target_os = "linux")))]
const BROWSER_APP_IDENTIFIER: &str = "tech.lattice.headless";

/// An app with no window, for the hidden browser windows the page reader
/// opens. Its event loop runs on a thread of its own for the rest of the
/// process.
#[cfg(all(test, any(windows, target_os = "linux")))]
pub fn browser_app() -> Result<tauri::AppHandle> {
    use lattice_core::shared::error::AppError;
    let (sender, receiver) =
        std::sync::mpsc::channel::<std::result::Result<tauri::AppHandle, String>>();
    std::thread::Builder::new()
        .name("headless-browser-app".into())
        .spawn(move || {
            let mut context = tauri::generate_context!();
            context.config_mut().identifier = BROWSER_APP_IDENTIFIER.to_owned();
            for window in &mut context.config_mut().app.windows {
                window.create = false;
            }
            let app = match tauri::Builder::default().any_thread().build(context) {
                Ok(app) => app,
                Err(error) => {
                    let _ = sender.send(Err(error.to_string()));
                    return;
                }
            };
            let _ = sender.send(Ok(app.handle().clone()));
            app.run(|_, event| {
                if let tauri::RunEvent::ExitRequested {
                    code: None, api, ..
                } = event
                {
                    api.prevent_exit();
                }
            });
        })
        .map_err(|error| {
            AppError::InternalError(format!("the browser app's thread did not start: {error}"))
        })?;
    match receiver.recv() {
        Ok(Ok(handle)) => Ok(handle),
        Ok(Err(error)) => Err(AppError::InternalError(format!(
            "the browser app did not build: {error}"
        ))),
        Err(_) => Err(AppError::InternalError(
            "the browser app's thread ended before it had an app".into(),
        )),
    }
}
