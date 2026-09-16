//! `create_archive` / `restore_archive`: the two archive operations that need
//! rate limiting and an audit trail, split from their Tauri wrappers the same
//! way `create.rs` and `restore.rs` are.
//!
//! Everything else in the archive command surface is configuration, which
//! goes straight to `ArchiveService` from `plugin.rs`.

use std::path::PathBuf;

use crate::features::backup::archive::format::ArchiveError;
use crate::features::backup::archive::restore::RestoreOutcome;
use crate::features::backup::dto::{archive_outcome, ArchiveRunDto, RestoreArchiveResultDto};
use crate::infrastructure::audit::{get_audit_logger, AuditAction, AuditEvent, AuditResult};
use crate::interfaces::di::Container;
use crate::shared::api_result::{ApiError, ErrorCode};
use crate::shared::error::AppError;

/// Write one archive now.
///
/// Shares the backup rate limiter with `create_backup`: both walk the whole
/// corpus and both are one button click away, so a wedged UI must not be able
/// to spin them.
pub async fn create_archive_impl(container: &Container) -> Result<ArchiveRunDto, AppError> {
    let audit_logger = get_audit_logger();

    container
        .security_context()
        .rate_limiters()
        .backup
        .check_rate_limit("backup")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    let result = container
        .archive_service()
        .create_now()
        .await
        .map_err(AppError::from);

    match &result {
        Ok(run) => {
            let event = AuditEvent::new(AuditAction::BackupCreated, AuditResult::success())
                .with_resource_id(run.path.to_string_lossy().as_ref())
                .with_metadata("operation", "create_archive")
                .with_metadata("backup_type", "archive");
            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
        Err(e) => {
            let event = AuditEvent::new(
                AuditAction::BackupCreated,
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id("archive_failed")
            .with_metadata("operation", "create_archive")
            .with_metadata("backup_type", "archive");
            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
    }

    result.map(|run| ArchiveRunDto {
        path: run.path.to_string_lossy().to_string(),
        created_at: run.created_at.to_rfc3339(),
        size: run.size,
        duration_ms: run.duration_ms,
    })
}

/// Restore from `source`, which the *user* picked through a native dialog in
/// `plugin.rs` — never a path the webview supplied.
///
/// Returns `ArchiveError` rather than `AppError` so the caller can still tell
/// a wrong passphrase from a corrupt file; `AppError` flattens both into a
/// string and the UI has to react differently to each.
pub async fn restore_archive_impl(
    source: PathBuf,
    secret: Option<String>,
    container: &Container,
) -> Result<RestoreArchiveResultDto, ArchiveError> {
    let audit_logger = get_audit_logger();
    let source_display = source.to_string_lossy().to_string();

    let result = container
        .archive_service()
        .restore(&source, secret.as_deref())
        .await;

    // Only a completed restore is a state change worth recording as one; a
    // prompt for a passphrase is not, and logging those as failures would
    // bury the real events.
    match &result {
        Ok(RestoreOutcome::Restored { .. }) => {
            let event = AuditEvent::new(AuditAction::BackupRestored, AuditResult::success())
                .with_resource_id(&source_display)
                .with_metadata("operation", "restore_archive")
                .with_metadata("backup_type", "archive");
            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
        Ok(_) => {}
        Err(e) => {
            let event = AuditEvent::new(
                AuditAction::BackupRestored,
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id(&source_display)
            .with_metadata("operation", "restore_archive")
            .with_metadata("backup_type", "archive");
            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
    }

    result.map(restore_result_dto)
}

fn restore_result_dto(outcome: RestoreOutcome) -> RestoreArchiveResultDto {
    match outcome {
        RestoreOutcome::Restored {
            restart_required,
            reembed_required,
            vault_restored_to,
            files_restored,
        } => RestoreArchiveResultDto {
            outcome: archive_outcome::RESTORED.to_string(),
            message: Some(
                "Backup restored. Quit and reopen Lattice; embeddings will rebuild in the background."
                    .to_string(),
            ),
            restart_required,
            reembed_required,
            vault_restored_to: vault_restored_to.map(|p| p.to_string_lossy().to_string()),
            files_restored,
        },
        RestoreOutcome::NeedsSecret => RestoreArchiveResultDto {
            outcome: archive_outcome::NEEDS_SECRET.to_string(),
            message: Some(
                "This device does not have the backup key. Enter the passphrase or the 24-word recovery code."
                    .to_string(),
            ),
            restart_required: false,
            reembed_required: false,
            vault_restored_to: None,
            files_restored: 0,
        },
        // The bare provider name; the UI writes the sentence around it.
        RestoreOutcome::NotHydrated(provider) => RestoreArchiveResultDto {
            outcome: archive_outcome::NOT_HYDRATED.to_string(),
            message: Some(provider),
            restart_required: false,
            reembed_required: false,
            vault_restored_to: None,
            files_restored: 0,
        },
    }
}

/// Map a restore failure for IPC.
///
/// A wrong secret is the one error the user can act on directly, so it
/// crosses the wire as its own bare sentence rather than wrapped in
/// "Permission denied: …" — the dialog prints it verbatim under the field.
pub fn restore_api_error(err: ArchiveError) -> ApiError {
    match err {
        ArchiveError::WrongSecret => ApiError {
            code: ErrorCode::PermissionDenied,
            message: ArchiveError::WrongSecret.to_string(),
            details: None,
        },
        other => ApiError::from(AppError::from(other)),
    }
}

/// True once a restore attempt is over and the remembered source file should
/// be forgotten. A prompt for a secret, a file the cloud has not downloaded
/// yet, and a mistyped passphrase are all mid-conversation: the user answers
/// and calls again, and re-opening the file picker at them would be rude.
pub fn restore_attempt_is_finished(result: &Result<RestoreArchiveResultDto, ArchiveError>) -> bool {
    match result {
        Ok(dto) => dto.outcome == archive_outcome::RESTORED,
        Err(ArchiveError::WrongSecret) => false,
        Err(_) => true,
    }
}

/// What `plugin_restore_archive` returns when the user closes the file picker.
pub fn cancelled_restore() -> RestoreArchiveResultDto {
    RestoreArchiveResultDto {
        outcome: archive_outcome::CANCELLED.to_string(),
        message: None,
        restart_required: false,
        reembed_required: false,
        vault_restored_to: None,
        files_restored: 0,
    }
}
