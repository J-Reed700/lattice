//! Backup scheduler service.
//!
//! Runs scheduled backups using a simple interval-based scheduler.

use crate::application::ports::{BackupSchedulerPort, SettingsRepositoryPort};
use crate::features::backup::use_cases::CreateBackupUseCase;
use crate::shared::error::Result;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, Mutex};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

#[derive(Default)]
struct SchedulerState {
    shutdown_tx: Option<oneshot::Sender<()>>,
    handle: Option<JoinHandle<()>>,
}

pub struct BackupScheduler {
    create_backup_use_case: Arc<CreateBackupUseCase>,
    settings_repo: Arc<dyn SettingsRepositoryPort>,
    state: Mutex<SchedulerState>,
}

impl BackupScheduler {
    pub fn new(
        create_backup_use_case: Arc<CreateBackupUseCase>,
        settings_repo: Arc<dyn SettingsRepositoryPort>,
    ) -> Self {
        Self {
            create_backup_use_case,
            settings_repo,
            state: Mutex::new(SchedulerState::default()),
        }
    }

    pub async fn start(&self, interval: Duration) -> Result<()> {
        let mut state = self.state.lock().await;

        if let Some(shutdown) = state.shutdown_tx.take() {
            let _ = shutdown.send(());
        }
        if let Some(handle) = state.handle.take() {
            handle.abort();
        }

        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        let create_backup_use_case = self.create_backup_use_case.clone();
        let settings_repo = self.settings_repo.clone();

        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            let mut consecutive_failures: u32 = 0;
            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        let settings = match settings_repo.get_all().await {
                            Ok(settings) => settings,
                            Err(e) => {
                                warn!("Failed to read settings for auto-backup: {}", e);
                                continue;
                            }
                        };

                        if !settings.backup.auto_backup_enabled {
                            continue;
                        }

                        let backup_path = if settings.backup.backup_path.is_empty() {
                            None
                        } else {
                            Some(PathBuf::from(settings.backup.backup_path))
                        };

                        match create_backup_use_case.execute(backup_path).await {
                            Ok(result) => {
                                if consecutive_failures > 0 {
                                    info!(
                                        recovered_after = consecutive_failures,
                                        "Auto-backup recovered after repeated failures"
                                    );
                                }
                                consecutive_failures = 0;
                                info!(backup_path = %result.backup_path, "Auto-backup completed");
                            }
                            Err(e) => {
                                consecutive_failures += 1;
                                // A backup job that fails every tick is the
                                // worst kind of failure: the user believes
                                // they are protected and are not. Make the
                                // repetition impossible to miss in the log.
                                error!(
                                    consecutive_failures,
                                    error = %e,
                                    "Auto-backup failed; NO BACKUPS ARE BEING CREATED"
                                );
                            }
                        }
                    }
                    _ = &mut shutdown_rx => {
                        break;
                    }
                }
            }
        });

        state.shutdown_tx = Some(shutdown_tx);
        state.handle = Some(handle);
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        let mut state = self.state.lock().await;

        if let Some(shutdown) = state.shutdown_tx.take() {
            let _ = shutdown.send(());
        }
        if let Some(handle) = state.handle.take() {
            handle.abort();
        }

        Ok(())
    }
}

#[async_trait]
impl BackupSchedulerPort for BackupScheduler {
    async fn start(&self, interval: Duration) -> Result<()> {
        BackupScheduler::start(self, interval).await
    }

    async fn stop(&self) -> Result<()> {
        BackupScheduler::stop(self).await
    }
}
