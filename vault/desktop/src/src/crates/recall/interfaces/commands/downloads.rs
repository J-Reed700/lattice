use crate::audit::AuditAction;
use crate::domain::download::{Checksum, ChecksumAlgorithm, DownloadError, DownloadSession};
use crate::infrastructure::security::RateLimiter;
use crate::infrastructure::services::download_manager::{DownloadManager, DownloadRequest};
use crate::shared::ValidatedFilePath;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;
use tracing::{error, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartDownloadRequest {
    pub url: String,
    pub destination: String,
    pub checksum: Option<ChecksumRequest>,
    pub auth_token: Option<String>,
    pub model_name: Option<String>,
    pub model_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecksumRequest {
    pub algorithm: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadStatusResponse {
    pub id: String,
    pub url: String,
    pub destination: String,
    pub state: String,
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,
    pub bytes_per_second: f64,
    pub percentage: Option<f64>,
    pub eta_seconds: Option<u64>,
    pub error_message: Option<String>,
    pub retry_count: u32,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub model_name: Option<String>,
    pub model_id: Option<String>,
}

impl From<DownloadSession> for DownloadStatusResponse {
    fn from(session: DownloadSession) -> Self {
        Self {
            id: session.id().to_string(),
            url: session.url().to_string(),
            destination: session.destination().to_string_lossy().to_string(),
            state: format!("{:?}", session.state()),
            bytes_downloaded: session.progress().bytes_downloaded(),
            total_bytes: session.progress().total_bytes(),
            bytes_per_second: session.progress().bytes_per_second(),
            percentage: session.progress().percentage(),
            eta_seconds: session.progress().estimated_time_remaining(),
            error_message: session.error_message().map(|s| s.to_string()),
            retry_count: session.retry_count(),
            created_at: session.created_at().to_rfc3339(),
            started_at: session.started_at().map(|t| t.to_rfc3339()),
            completed_at: session.completed_at().map(|t| t.to_rfc3339()),
            model_name: session.model_name().map(|s| s.to_string()),
            model_id: session.model_id().map(|s| s.to_string()),
        }
    }
}

pub struct DownloadCommandState {
    pub manager: Arc<dyn DownloadManager>,
    pub start_rate_limiter: Arc<RateLimiter>,
    pub query_rate_limiter: Arc<RateLimiter>,
}

impl DownloadCommandState {
    pub fn new(manager: Arc<dyn DownloadManager>) -> Self {
        Self {
            manager,
            start_rate_limiter: Arc::new(RateLimiter::new(10, 60)),
            query_rate_limiter: Arc::new(RateLimiter::new(100, 60)),
        }
    }
}

fn validate_url(url: &str) -> Result<(), String> {
    if url.trim().is_empty() {
        return Err("URL cannot be empty".to_string());
    }

    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("URL must start with http:// or https://".to_string());
    }

    if url.len() > 2048 {
        return Err("URL exceeds maximum length of 2048 characters".to_string());
    }

    Ok(())
}

fn validate_destination(path: &str) -> Result<ValidatedFilePath, String> {
    if path.trim().is_empty() {
        return Err("Destination path cannot be empty".to_string());
    }

    ValidatedFilePath::new(PathBuf::from(path))
        .map_err(|e| format!("Invalid destination path: {}", e))
}

fn parse_checksum(req: Option<ChecksumRequest>) -> Result<Option<Checksum>, String> {
    if let Some(checksum_req) = req {
        let algorithm = match checksum_req.algorithm.to_lowercase().as_str() {
            "sha256" => ChecksumAlgorithm::Sha256,
            "md5" => ChecksumAlgorithm::Md5,
            _ => {
                return Err(format!(
                    "Unsupported checksum algorithm: {}",
                    checksum_req.algorithm
                ))
            }
        };

        Checksum::new(algorithm, checksum_req.value)
            .map(Some)
            .map_err(|e| format!("Invalid checksum: {}", e))
    } else {
        Ok(None)
    }
}

async fn start_model_download_impl(
    state: &DownloadCommandState,
    request: StartDownloadRequest,
) -> Result<String, String> {
    let logger = crate::audit::get_audit_logger();

    state
        .start_rate_limiter
        .check_rate_limit("start_download")
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

    validate_url(&request.url)?;
    let validated_path = validate_destination(&request.destination)?;
    let checksum = parse_checksum(request.checksum)?;

    let download_request = DownloadRequest {
        url: request.url.clone(),
        destination: validated_path.into_inner(),
        checksum,
        auth_token: request.auth_token,
        model_name: request.model_name,
        model_id: request.model_id,
    };

    match state.manager.start_download(download_request).await {
        Ok(id) => {
            info!(id = %id, url = %request.url, "Download started");

            crate::audit_success!(
                logger,
                crate::audit::AuditAction::DownloadStarted,
                "download",
                "id" => id.as_str(),
                "url" => request.url.as_str(),
                "destination" => request.destination.as_str()
            )
            .await
            .ok();

            Ok(id)
        }
        Err(e) => {
            error!(url = %request.url, error = %e, "Failed to start download");

            crate::audit_failure!(
                logger,
                crate::audit::AuditAction::DownloadStarted,
                "download",
                format!("Failed to start download: {}", e)
            )
            .await
            .ok();

            Err(format!("Failed to start download: {}", e))
        }
    }
}

pub async fn start_model_download(
    state: State<'_, DownloadCommandState>,
    request: StartDownloadRequest,
) -> Result<String, String> {
    start_model_download_impl(&state, request).await
}

async fn pause_download_impl(state: &DownloadCommandState, id: String) -> Result<(), String> {
    let logger = crate::audit::get_audit_logger();

    state
        .query_rate_limiter
        .check_rate_limit("query")
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

    if id.trim().is_empty() {
        return Err("Download ID cannot be empty".to_string());
    }

    match state.manager.pause_download(&id).await {
        Ok(_) => {
            info!(id = %id, "Download paused");

            crate::audit_success!(
                logger,
                crate::audit::AuditAction::DownloadPaused,
                "download",
                "id" => id.as_str()
            )
            .await
            .ok();

            Ok(())
        }
        Err(e) => {
            error!(id = %id, error = %e, "Failed to pause download");

            crate::audit_failure!(
                logger,
                crate::audit::AuditAction::DownloadPaused,
                "download",
                format!("Failed to pause download: {}", e),
                "id" => id.as_str()
            )
            .await
            .ok();

            Err(format!("Failed to pause download: {}", e))
        }
    }
}

pub async fn pause_download(
    state: State<'_, DownloadCommandState>,
    id: String,
) -> Result<(), String> {
    pause_download_impl(&state, id).await
}

async fn resume_download_impl(state: &DownloadCommandState, id: String) -> Result<(), String> {
    let logger = crate::audit::get_audit_logger();

    state
        .query_rate_limiter
        .check_rate_limit("query")
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

    if id.trim().is_empty() {
        return Err("Download ID cannot be empty".to_string());
    }

    match state.manager.resume_download(&id).await {
        Ok(_) => {
            info!(id = %id, "Download resumed");

            crate::audit_success!(
                logger,
                crate::audit::AuditAction::DownloadResumed,
                "download",
                "id" => id.as_str()
            )
            .await
            .ok();

            Ok(())
        }
        Err(e) => {
            error!(id = %id, error = %e, "Failed to resume download");

            crate::audit_failure!(
                logger,
                crate::audit::AuditAction::DownloadResumed,
                "download",
                format!("Failed to resume download: {}", e),
                "id" => id.as_str()
            )
            .await
            .ok();

            Err(format!("Failed to resume download: {}", e))
        }
    }
}

pub async fn resume_download(
    state: State<'_, DownloadCommandState>,
    id: String,
) -> Result<(), String> {
    resume_download_impl(&state, id).await
}

async fn cancel_download_impl(state: &DownloadCommandState, id: String) -> Result<(), String> {
    let logger = crate::audit::get_audit_logger();

    state
        .query_rate_limiter
        .check_rate_limit("query")
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

    if id.trim().is_empty() {
        return Err("Download ID cannot be empty".to_string());
    }

    match state.manager.cancel_download(&id).await {
        Ok(_) => {
            info!(id = %id, "Download cancelled");

            crate::audit_success!(
                logger,
                crate::audit::AuditAction::DownloadCancelled,
                "download",
                "id" => id.as_str()
            )
            .await
            .ok();

            Ok(())
        }
        Err(e) => {
            error!(id = %id, error = %e, "Failed to cancel download");

            crate::audit_failure!(
                logger,
                crate::audit::AuditAction::DownloadCancelled,
                "download",
                format!("Failed to cancel download: {}", e),
                "id" => id.as_str()
            )
            .await
            .ok();

            Err(format!("Failed to cancel download: {}", e))
        }
    }
}

pub async fn cancel_download(
    state: State<'_, DownloadCommandState>,
    id: String,
) -> Result<(), String> {
    cancel_download_impl(&state, id).await
}

pub async fn get_download_status(
    state: State<'_, DownloadCommandState>,
    id: String,
) -> Result<Option<DownloadStatusResponse>, String> {
    let logger = crate::audit::get_audit_logger();

    state
        .query_rate_limiter
        .check_rate_limit("query")
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

    if id.trim().is_empty() {
        return Err("Download ID cannot be empty".to_string());
    }

    match state.manager.get_download_status(&id).await {
        Ok(Some(session)) => Ok(Some(session.into())),
        Ok(None) => Ok(None),
        Err(e) => {
            crate::audit_failure!(
                logger,
                crate::audit::AuditAction::DocumentAccessed,
                "download",
                format!("Failed to query download: {}", e),
                "id" => id.as_str()
            )
            .await
            .ok();

            Err(format!("Failed to get download status: {}", e))
        }
    }
}

pub async fn list_downloads(
    state: State<'_, DownloadCommandState>,
) -> Result<Vec<DownloadStatusResponse>, String> {
    state
        .query_rate_limiter
        .check_rate_limit("query")
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

    match state.manager.list_downloads().await {
        Ok(sessions) => Ok(sessions.into_iter().map(|s| s.into()).collect()),
        Err(e) => Err(format!("Failed to list downloads: {}", e)),
    }
}

/// Retry a failed download
pub async fn retry_download(
    state: State<'_, DownloadCommandState>,
    id: String,
) -> Result<(), String> {
    let logger = crate::audit::get_audit_logger();

    // Rate limiting
    state
        .query_rate_limiter
        .check_rate_limit("query")
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

    // Input validation
    if id.trim().is_empty() {
        return Err("Download ID cannot be empty".to_string());
    }

    // Execute
    match state.manager.retry_download(&id).await {
        Ok(_) => {
            info!(id = %id, "Download retried");

            crate::audit_success!(
                logger,
                crate::audit::AuditAction::DownloadRetried,
                "download",
                "id" => id.as_str()
            )
            .await
            .ok();

            Ok(())
        }
        Err(DownloadError::MaxRetriesExceeded) => {
            let msg = "Cannot retry: maximum retry attempts (3) exceeded".to_string();
            error!(id = %id, error = %msg);

            crate::audit_failure!(
                logger,
                crate::audit::AuditAction::DownloadRetried,
                "download",
                msg.clone(),
                "id" => id.as_str()
            )
            .await
            .ok();

            Err(msg)
        }
        Err(e) => {
            error!(id = %id, error = %e, "Failed to retry download");

            crate::audit_failure!(
                logger,
                crate::audit::AuditAction::DownloadRetried,
                "download",
                format!("Failed to retry download: {}", e),
                "id" => id.as_str()
            )
            .await
            .ok();

            Err(format!("Failed to retry download: {}", e))
        }
    }
}

/// Remove a download (works for any state)
pub async fn remove_download(
    state: State<'_, DownloadCommandState>,
    id: String,
) -> Result<(), String> {
    let logger = crate::audit::get_audit_logger();

    state
        .query_rate_limiter
        .check_rate_limit("query")
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

    if id.trim().is_empty() {
        return Err("Download ID cannot be empty".to_string());
    }

    match state.manager.delete_download(&id).await {
        Ok(_) => {
            info!(id = %id, "Download removed");

            crate::audit_success!(
                logger,
                crate::audit::AuditAction::DownloadDeleted,
                "download",
                "id" => id.as_str()
            )
            .await
            .ok();

            Ok(())
        }
        Err(e) => {
            error!(id = %id, error = %e, "Failed to remove download");

            crate::audit_failure!(
                logger,
                crate::audit::AuditAction::DownloadDeleted,
                "download",
                format!("Failed to remove download: {}", e),
                "id" => id.as_str()
            )
            .await
            .ok();

            Err(format!("Failed to remove download: {}", e))
        }
    }
}

/// Clear all completed downloads
pub async fn clear_completed_downloads(
    state: State<'_, DownloadCommandState>,
) -> Result<usize, String> {
    let logger = crate::audit::get_audit_logger();

    state
        .query_rate_limiter
        .check_rate_limit("query")
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

    match state.manager.clear_completed_downloads().await {
        Ok(count) => {
            info!(count = %count, "Completed downloads cleared");

            crate::audit_success!(
                logger,
                crate::audit::AuditAction::DownloadsCleared,
                "downloads",
                "count" => count.to_string().as_str()
            )
            .await
            .ok();

            Ok(count)
        }
        Err(e) => {
            error!(error = %e, "Failed to clear completed downloads");

            crate::audit_failure!(
                logger,
                crate::audit::AuditAction::DownloadsCleared,
                "downloads",
                format!("Failed to clear downloads: {}", e)
            )
            .await
            .ok();

            Err(format!("Failed to clear completed downloads: {}", e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::persistence::download_repository::mock::MockDownloadRepository;
    use crate::infrastructure::services::download_engine::mock::MockDownloadEngine;
    use crate::infrastructure::services::download_manager::DownloadManagerService;

    fn temp_root() -> std::path::PathBuf {
        std::env::temp_dir().join("recall-download-command-tests")
    }

    fn temp_file() -> String {
        temp_root().join("file.bin").to_string_lossy().to_string()
    }

    fn create_test_state() -> DownloadCommandState {
        let repository = Arc::new(MockDownloadRepository::new());
        let engine = Arc::new(MockDownloadEngine::new());
        let allowed_root = temp_root();
        let manager = Arc::new(DownloadManagerService::new(
            repository,
            engine,
            allowed_root,
        ));
        DownloadCommandState::new(manager)
    }

    #[test]
    fn test_validate_url() {
        assert!(validate_url("https://example.com/file.bin").is_ok());
        assert!(validate_url("http://example.com/file.bin").is_ok());
        assert!(validate_url("").is_err());
        assert!(validate_url("ftp://example.com/file.bin").is_err());
        assert!(validate_url(&"a".repeat(3000)).is_err());
    }

    #[test]
    fn test_validate_destination() {
        assert!(validate_destination(&temp_file()).is_ok());
        assert!(validate_destination("").is_err());
    }

    #[test]
    fn test_parse_checksum() {
        let valid_sha256 = ChecksumRequest {
            algorithm: "sha256".to_string(),
            value: "a".repeat(64),
        };
        assert!(parse_checksum(Some(valid_sha256)).is_ok());

        let invalid_algorithm = ChecksumRequest {
            algorithm: "invalid".to_string(),
            value: "abc".to_string(),
        };
        assert!(parse_checksum(Some(invalid_algorithm)).is_err());

        assert!(parse_checksum(None).unwrap().is_none());
    }

    #[tokio::test]
    async fn test_start_model_download_validation() {
        let state = create_test_state();

        let invalid_url = StartDownloadRequest {
            url: "".to_string(),
            destination: temp_file(),
            checksum: None,
            auth_token: None,
            model_name: None,
            model_id: None,
        };
        assert!(start_model_download_impl(&state, invalid_url)
            .await
            .is_err());

        let invalid_dest = StartDownloadRequest {
            url: "https://example.com/file.bin".to_string(),
            destination: "".to_string(),
            checksum: None,
            auth_token: None,
            model_name: None,
            model_id: None,
        };
        assert!(start_model_download_impl(&state, invalid_dest)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_pause_download_validation() {
        let state = create_test_state();
        assert!(pause_download_impl(&state, "".to_string()).await.is_err());
    }

    #[tokio::test]
    async fn test_resume_download_validation() {
        let state = create_test_state();
        assert!(resume_download_impl(&state, "".to_string()).await.is_err());
    }

    #[tokio::test]
    async fn test_cancel_download_validation() {
        let state = create_test_state();
        assert!(cancel_download_impl(&state, "".to_string()).await.is_err());
    }
}
