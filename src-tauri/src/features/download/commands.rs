use crate::domain::download::{Checksum, ChecksumAlgorithm, DownloadError, DownloadSession};
use crate::domain::model_paths::ModelPaths;
use crate::features::download::manager::{DownloadManager, DownloadRequest};
use crate::infrastructure::security::RateLimiter;
use crate::shared::path_confinement::confine_to_root;
use crate::shared::ValidatedFilePath;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;
use tracing::{error, info};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct StartDownloadRequest {
    pub url: String,
    pub destination: String,
    pub checksum: Option<ChecksumRequest>,
    pub auth_token: Option<String>,
    pub model_name: Option<String>,
    pub model_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ChecksumRequest {
    pub algorithm: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
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

/// Hosts this command is permitted to fetch model weights from.
///
/// Without an allowlist, `start_model_download` is an unrestricted outbound
/// fetch primitive reachable from the webview: it will retrieve any URL and
/// write the response to disk, which makes it useful both for SSRF against
/// loopback/link-local services and for pulling attacker-controlled bytes.
/// Model downloads only ever target Hugging Face, so the allowlist is small.
const ALLOWED_DOWNLOAD_HOSTS: &[&str] = &["huggingface.co", "hf.co"];

/// True when `host` is an allowlisted host or a subdomain of one.
///
/// Matching the suffix on a dot boundary matters: a bare `ends_with` would
/// also accept `evil-huggingface.co`.
fn is_allowed_download_host(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    ALLOWED_DOWNLOAD_HOSTS
        .iter()
        .any(|allowed| host == *allowed || host.ends_with(&format!(".{}", allowed)))
}

fn validate_url(url: &str) -> Result<(), String> {
    if url.trim().is_empty() {
        return Err("URL cannot be empty".to_string());
    }

    if url.len() > 2048 {
        return Err("URL exceeds maximum length of 2048 characters".to_string());
    }

    let parsed = url::Url::parse(url).map_err(|e| format!("Invalid URL: {}", e))?;

    // Plaintext HTTP is not acceptable for content we execute as model
    // weights, and it also permits transparent interception.
    if parsed.scheme() != "https" {
        return Err("URL must use https".to_string());
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| "URL must include a host".to_string())?;

    if !is_allowed_download_host(host) {
        return Err(format!(
            "Downloads are only permitted from {}; got '{}'",
            ALLOWED_DOWNLOAD_HOSTS.join(", "),
            host
        ));
    }

    Ok(())
}

/// Resolve the caller-supplied destination and require it to sit under the
/// models root.
///
/// `ValidatedFilePath` alone is not sufficient — it rejects `..` but accepts
/// any absolute path, so a compromised renderer could previously write the
/// fetched bytes to e.g. `~/Library/LaunchAgents/com.evil.plist` and obtain
/// code execution at next login.
fn validate_destination(path: &str) -> Result<PathBuf, String> {
    if path.trim().is_empty() {
        return Err("Destination path cannot be empty".to_string());
    }

    let validated = ValidatedFilePath::new(PathBuf::from(path))
        .map_err(|e| format!("Invalid destination path: {}", e))?;

    let models_root =
        ModelPaths::models_root().map_err(|e| format!("Cannot resolve models directory: {}", e))?;

    confine_to_root(&models_root, validated.as_path())
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
        destination: validated_path,
        checksum,
        auth_token: request.auth_token,
        model_name: request.model_name,
        model_id: request.model_id,
        model_file_name: None,
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
    use crate::features::download::download_repository::mock::MockDownloadRepository;
    use crate::features::download::engine::mock::MockDownloadEngine;
    use crate::features::download::manager::DownloadManagerService;

    fn temp_root() -> std::path::PathBuf {
        std::env::temp_dir().join("lattice-download-command-tests")
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
        // Allowlisted hosts and their subdomains.
        assert!(validate_url("https://huggingface.co/repo/resolve/main/m.gguf").is_ok());
        assert!(validate_url("https://cdn-lfs.huggingface.co/x/y.bin").is_ok());
        assert!(validate_url("https://hf.co/repo/file.bin").is_ok());

        assert!(validate_url("").is_err());
        assert!(validate_url("ftp://huggingface.co/file.bin").is_err());
        assert!(validate_url(&"a".repeat(3000)).is_err());

        // Arbitrary hosts are an SSRF / attacker-content channel.
        assert!(validate_url("https://example.com/file.bin").is_err());
        assert!(validate_url("http://127.0.0.1:8080/admin").is_err());
        assert!(validate_url("https://169.254.169.254/latest/meta-data/").is_err());

        // Plaintext is refused even for an allowlisted host.
        assert!(validate_url("http://huggingface.co/file.bin").is_err());

        // Suffix matching must respect the dot boundary.
        assert!(validate_url("https://evil-huggingface.co/file.bin").is_err());
        assert!(validate_url("https://huggingface.co.evil.test/file.bin").is_err());
    }

    #[test]
    fn test_validate_destination() {
        assert!(validate_destination("").is_err());

        // Inside the models root: allowed.
        let models_root = ModelPaths::models_root().expect("models root");
        let inside = models_root.join("test-model").join("weights.gguf");
        assert!(
            validate_destination(&inside.to_string_lossy()).is_ok(),
            "a path under the models root must be accepted"
        );
    }

    /// The SEC-1 exploit: an absolute path with no `..` in it, which the old
    /// `..`-only filter accepted, writing attacker-controlled bytes to a
    /// login-time execution point.
    #[test]
    fn destination_outside_models_root_is_rejected() {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());

        for path in [
            format!("{}/Library/LaunchAgents/com.evil.plist", home),
            format!("{}/.zshenv", home),
            format!("{}/.ssh/authorized_keys", home),
            "/tmp/anywhere.bin".to_string(),
        ] {
            assert!(
                validate_destination(&path).is_err(),
                "destination outside the models root must be rejected: {}",
                path
            );
        }
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
