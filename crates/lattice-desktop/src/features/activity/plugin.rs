//! The `activity` Tauri plugin: one read of everything running, and an
//! event when something with no event of its own moves.

use super::assemble::assemble;
use super::dto::{ActivitySnapshotDto, CHANGED_EVENT};
use crate::interfaces::di::Container;
use lattice_core::shared::ipc::ApiError;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Emitter, Manager, Runtime, State,
};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

/// Everything running, paused or waiting now. A part whose read fails is
/// left out and logged, so one broken read never empties the list. Each
/// later change arrives on `jobs://status`, the download and indexing
/// events, or `activity://changed`.
#[tauri::command]
#[specta::specta]
pub async fn activity_snapshot(
    container: State<'_, Container>,
) -> Result<ActivitySnapshotDto, ApiError> {
    let sources = container.activity_sources();
    let reads = futures::future::join_all(sources.iter().map(|source| source.activity())).await;
    let mut items = Vec::new();
    for read in reads {
        match read {
            Ok(found) => items.extend(found),
            Err(error) => tracing::warn!(%error, "Could not read part of the app's activity"),
        }
    }
    let standing = container
        .index_turns()
        .map(|turns| turns.standing())
        .unwrap_or_default();
    Ok(assemble(items, &standing))
}

/// Emits `activity://changed` each time one of `changes` moves, until
/// `stop` fires. Moves that land while one is being told are told once.
async fn tell_changes<R: Runtime>(
    app: tauri::AppHandle<R>,
    mut changes: Vec<watch::Receiver<u64>>,
    stop: CancellationToken,
) {
    while !changes.is_empty() {
        let moved = {
            let waits = changes.iter_mut().map(|change| Box::pin(change.changed()));
            tokio::select! {
                () = stop.cancelled() => return,
                (moved, index, _) = futures::future::select_all(waits) => (moved.is_ok(), index),
            }
        };
        match moved {
            (true, _) => {
                if let Err(error) = app.emit(CHANGED_EVENT, ()) {
                    tracing::debug!(%error, "Could not tell the window the activity changed");
                }
            }
            // Its sender is gone; nothing will move it again.
            (false, index) => {
                changes.swap_remove(index);
            }
        }
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("activity")
        .setup(|app, _api| {
            let container = app.state::<Container>().inner().clone();
            let changes = container.activity_changes();
            let background = std::sync::Arc::clone(container.core.background());
            let stop = background.token();
            let app = app.clone();
            // Spawned from inside the runtime, so it runs on after setup
            // returns.
            tauri::async_runtime::block_on(async move {
                background.spawn(tell_changes(app, changes, stop));
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![activity_snapshot])
        .build()
}
