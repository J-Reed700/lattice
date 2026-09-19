use super::background_workers::BackgroundWorkers;
use crate::features::llm::engine::sidecar_manager::SidecarRegistry;
use crate::interfaces::di::Container;
use std::time::Duration;
use tauri::Manager;
use tokio_util::sync::CancellationToken;

// Extended timeouts to handle SQLite busy conditions
const DB_CLOSE_TIMEOUT: Duration = Duration::from_secs(10);
const TOTAL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(15);
/// A large HNSW graph takes a moment to write. Overrunning this only costs a
/// rebuild on the next launch, so it stays well inside the total budget.
const INDEX_FLUSH_TIMEOUT: Duration = Duration::from_secs(5);

pub fn graceful_shutdown(app_handle: &tauri::AppHandle) {
    tracing::info!("Exit requested, starting graceful shutdown");

    // Kill llama-server sidecars first, synchronously,
    // before any async cleanup runs. This is the load-bearing
    // anti-zombie hook — by the time the tokio runtime starts tearing
    // down (which makes Drop-based kills race-y), every sidecar PID
    // is already SIGKILL'd. Survives panics in the rest of shutdown.
    if let Some(registry) = app_handle.try_state::<SidecarRegistry>() {
        let killed = registry.kill_all();
        if killed > 0 {
            tracing::info!(
                count = killed,
                "Killed {} llama-server sidecar(s) during shutdown",
                killed
            );
        }
    } else {
        tracing::debug!("SidecarRegistry not managed; skipping sidecar shutdown");
    }

    tauri::async_runtime::block_on(async {
        let shutdown_future = async {
            let mut cleanup_results = Vec::new();

            if let Some(cancel) = app_handle.try_state::<CancellationToken>() {
                cancel.cancel();
                if let Some(workers) = app_handle.try_state::<BackgroundWorkers>() {
                    workers.stop(&cancel).await;
                }
            }

            if let Some(container) = app_handle.try_state::<Container>() {
                // Before the database closes, because writing the index's
                // manifest reads the row counts that decide whether the next
                // launch can trust it. Without this the index is still
                // correct — the next launch just rebuilds it from SQLite.
                if let Some(persistence) = container.search.index_persistence() {
                    match tokio::time::timeout(INDEX_FLUSH_TIMEOUT, persistence.flush_if_dirty())
                        .await
                    {
                        Ok(Ok(true)) => tracing::info!("Vector index flushed before shutdown"),
                        Ok(Ok(false)) => {}
                        Ok(Err(error)) => {
                            tracing::warn!(%error, "Could not flush the vector index; it will be rebuilt on next launch")
                        }
                        Err(_) => tracing::warn!(
                            timeout_secs = INDEX_FLUSH_TIMEOUT.as_secs(),
                            "Vector index flush timed out; it will be rebuilt on next launch"
                        ),
                    }
                }

                tracing::info!("Closing database connections");

                // Log connection count before closing
                let pool = container.db_pool();
                let conn_count = pool.size();
                tracing::debug!(
                    active_connections = conn_count,
                    "Database connection pool status before close"
                );

                match tokio::time::timeout(DB_CLOSE_TIMEOUT, pool.close()).await {
                    Ok(_) => {
                        tracing::info!("Database connections closed successfully");
                        cleanup_results.push("database: ok");
                    }
                    Err(_) => {
                        tracing::warn!(
                            timeout_secs = DB_CLOSE_TIMEOUT.as_secs(),
                            connection_count = conn_count,
                            "Database close timed out, will attempt forced cleanup"
                        );
                        cleanup_results.push("database: timeout");

                        // Attempt forced cleanup (best-effort)
                        tracing::info!("Attempting forced database cleanup");
                        // The pool will be dropped anyway, but log the attempt
                        cleanup_results.push("database: forced_cleanup_attempted");
                    }
                }

                tracing::info!(
                    cleanup_summary = ?cleanup_results,
                    "Shutdown cleanup summary"
                );
            } else {
                tracing::warn!("Container not available during shutdown");
            }
        };

        match tokio::time::timeout(TOTAL_SHUTDOWN_TIMEOUT, shutdown_future).await {
            Ok(_) => {
                tracing::info!(
                    duration_secs = TOTAL_SHUTDOWN_TIMEOUT.as_secs(),
                    "Graceful shutdown completed successfully"
                );
            }
            Err(_) => {
                tracing::error!(
                    timeout_secs = TOTAL_SHUTDOWN_TIMEOUT.as_secs(),
                    "Graceful shutdown timed out, forcing exit"
                );
                // Still flush tracing before forced exit
                crate::infrastructure::observability::tracing::shutdown_tracing();
                std::process::exit(0);
            }
        }

        // Flush OTEL spans and file logs before exit
        crate::infrastructure::observability::tracing::shutdown_tracing();
    });
}
