#![cfg(test)]

use super::*;

fn temp_path(file_name: &str) -> PathBuf {
    std::env::temp_dir().join(file_name)
}

#[test]
fn test_download_state_transitions() {
    assert!(DownloadState::Pending.can_transition_to(&DownloadState::Downloading));
    assert!(DownloadState::Downloading.can_transition_to(&DownloadState::Paused));
    assert!(DownloadState::Paused.can_transition_to(&DownloadState::Downloading));
    assert!(!DownloadState::Completed.can_transition_to(&DownloadState::Downloading));
}

#[test]
fn test_download_state_predicates() {
    assert!(DownloadState::Completed.is_terminal());
    assert!(!DownloadState::Downloading.is_terminal());
    assert!(DownloadState::Downloading.is_active());
    assert!(DownloadState::Downloading.can_pause());
    assert!(DownloadState::Paused.can_resume());
}

#[test]
fn test_download_progress_percentage() {
    let mut progress = DownloadProgress::new(Some(1000));
    progress.update(500, 100.0);
    assert_eq!(progress.percentage(), Some(50.0));
}

#[test]
fn test_download_progress_percentage_edge_cases() {
    // Unknown total
    let progress = DownloadProgress::new(None);
    assert_eq!(progress.percentage(), None);

    // Zero total
    let progress = DownloadProgress::new(Some(0));
    assert_eq!(progress.percentage(), Some(0.0));

    // Complete download
    let mut progress = DownloadProgress::new(Some(1000));
    progress.update(1000, 100.0);
    assert_eq!(progress.percentage(), Some(100.0));

    // Over 100% (should clamp)
    let mut progress = DownloadProgress::new(Some(1000));
    progress.update(1500, 100.0);
    assert_eq!(progress.percentage(), Some(100.0));

    // Very small progress
    let mut progress = DownloadProgress::new(Some(1_000_000));
    progress.update(1, 100.0);
    let percentage = progress.percentage().unwrap();
    assert!(percentage > 0.0 && percentage < 1.0);
}

#[test]
fn test_download_progress_eta() {
    let mut progress = DownloadProgress::new(Some(1000));
    progress.update(500, 100.0);
    assert_eq!(progress.estimated_time_remaining(), Some(5));
}

#[test]
fn test_download_progress_eta_edge_cases() {
    // Zero speed
    let mut progress = DownloadProgress::new(Some(1000));
    progress.update(500, 0.0);
    assert_eq!(progress.estimated_time_remaining(), None);

    // Negative speed (clamped to 0)
    let mut progress = DownloadProgress::new(Some(1000));
    progress.update(500, -10.0);
    assert_eq!(progress.bytes_per_second(), 0.0);
    assert_eq!(progress.estimated_time_remaining(), None);

    // Unknown total
    let mut progress = DownloadProgress::new(None);
    progress.update(500, 100.0);
    assert_eq!(progress.estimated_time_remaining(), None);

    // Complete download
    let mut progress = DownloadProgress::new(Some(1000));
    progress.update(1000, 100.0);
    assert_eq!(progress.estimated_time_remaining(), Some(0));

    // Over complete
    let mut progress = DownloadProgress::new(Some(1000));
    progress.update(1200, 100.0);
    assert_eq!(progress.estimated_time_remaining(), Some(0));

    // Very slow speed
    let mut progress = DownloadProgress::new(Some(1_000_000_000));
    progress.update(0, 0.001);
    let eta = progress.estimated_time_remaining().unwrap();
    assert!(eta > 0);

    // Rounding test
    let mut progress = DownloadProgress::new(Some(1000));
    progress.update(500, 123.0);
    let eta = progress.estimated_time_remaining().unwrap();
    assert_eq!(eta, 4);
}

#[test]
fn test_download_progress_is_complete() {
    let mut progress = DownloadProgress::new(Some(1000));
    assert!(!progress.is_complete());

    progress.update(500, 100.0);
    assert!(!progress.is_complete());

    progress.update(1000, 100.0);
    assert!(progress.is_complete());

    progress.update(1200, 100.0);
    assert!(progress.is_complete());

    let progress_unknown = DownloadProgress::new(None);
    assert!(!progress_unknown.is_complete());
}

#[test]
fn test_checksum_validation() {
    let valid_sha256 = "a".repeat(64);
    let checksum = Checksum::new(ChecksumAlgorithm::Sha256, valid_sha256.clone());
    assert!(checksum.is_ok());

    let invalid_length = "abc123";
    let checksum = Checksum::new(ChecksumAlgorithm::Sha256, invalid_length.to_string());
    assert!(checksum.is_err());

    let invalid_chars = "z".repeat(64);
    let checksum = Checksum::new(ChecksumAlgorithm::Sha256, invalid_chars);
    assert!(checksum.is_err());
}

#[test]
fn test_checksum_verification() {
    let value = "a".repeat(64);
    let checksum = Checksum::new(ChecksumAlgorithm::Sha256, value.clone()).unwrap();

    assert!(checksum.verify(&value).is_ok());
    assert!(checksum.verify(&value.to_uppercase()).is_ok());

    let wrong_value = "b".repeat(64);
    assert!(checksum.verify(&wrong_value).is_err());
}

#[test]
fn test_download_session_creation() {
    let session = DownloadSession::new(
        "test-id".to_string(),
        "https://example.com/file.bin".to_string(),
        temp_path("file.bin"),
        Some(1000),
        None,
    );

    assert!(session.is_ok());
    let session = session.unwrap();
    assert_eq!(session.state(), &DownloadState::Pending);
    assert_eq!(session.retry_count(), 0);
}

#[test]
fn test_download_session_invalid_url() {
    let session = DownloadSession::new(
        "test-id".to_string(),
        "invalid-url".to_string(),
        temp_path("file.bin"),
        Some(1000),
        None,
    );

    assert!(session.is_err());
}

#[test]
fn test_download_session_lifecycle() {
    let mut session = DownloadSession::new(
        "test-id".to_string(),
        "https://example.com/file.bin".to_string(),
        temp_path("file.bin"),
        Some(1000),
        None,
    )
    .unwrap();

    assert!(session.start().is_ok());
    assert_eq!(session.state(), &DownloadState::Downloading);
    assert!(session.started_at().is_some());

    session.update_progress(500, 100.0);
    assert_eq!(session.progress().bytes_downloaded(), 500);

    assert!(session.pause().is_ok());
    assert_eq!(session.state(), &DownloadState::Paused);

    assert!(session.resume().is_ok());
    assert_eq!(
        session.state(),
        &DownloadState::Pending,
        "a resumed session waits for a slot"
    );
    session.start().unwrap();
    assert_eq!(session.state(), &DownloadState::Downloading);

    assert!(session.complete().is_ok());
    assert_eq!(session.state(), &DownloadState::Completed);
    assert!(session.completed_at().is_some());
}

#[test]
fn test_download_session_retry_logic() {
    let mut session = DownloadSession::new(
        "test-id".to_string(),
        "https://example.com/file.bin".to_string(),
        temp_path("file.bin"),
        Some(1000),
        None,
    )
    .unwrap();

    session.start().unwrap();
    session.fail("Network error".to_string()).unwrap();

    assert!(session.can_retry());
    assert_eq!(session.retry_count(), 1);

    session.resume().unwrap();
    session.fail("Network error".to_string()).unwrap();
    assert_eq!(session.retry_count(), 2);

    session.resume().unwrap();
    session.fail("Network error".to_string()).unwrap();
    assert_eq!(session.retry_count(), 3);

    assert!(!session.can_retry());
}

#[test]
fn test_retry_resets_progress() {
    let mut session = DownloadSession::new(
        "test-id".to_string(),
        "https://example.com/file.bin".to_string(),
        temp_path("file.bin"),
        Some(1000),
        None,
    )
    .unwrap();

    session.start().unwrap();
    session.update_progress(500, 100.0);
    assert_eq!(session.progress().bytes_downloaded(), 500);

    // Fail the download (e.g., 0 bytes downloaded, file cleaned up)
    session
        .fail("Validation failed: 0 bytes".to_string())
        .unwrap();
    assert_eq!(session.state(), &DownloadState::Failed);

    // Retry should reset progress to 0 (since file was deleted)
    session.retry().unwrap();
    assert_eq!(session.progress().bytes_downloaded(), 0);
    assert_eq!(session.state(), &DownloadState::Pending);
    assert!(session.error_message().is_none());

    // Progress should still know total_bytes
    assert_eq!(session.progress().total_bytes(), Some(1000));
}

#[test]
fn test_download_session_cancel() {
    let mut session = DownloadSession::new(
        "test-id".to_string(),
        "https://example.com/file.bin".to_string(),
        temp_path("file.bin"),
        Some(1000),
        None,
    )
    .unwrap();

    session.start().unwrap();
    assert!(session.cancel().is_ok());
    assert_eq!(session.state(), &DownloadState::Cancelled);

    assert!(session.resume().is_err());
}

#[test]
fn test_progress_percentage_calculation() {
    let mut progress = DownloadProgress::new(Some(1000));
    progress.update(750, 0.0);

    let percentage = progress.percentage();

    assert_eq!(percentage, Some(75.0), "750 of 1000 bytes should be 75%");
}

#[test]
fn test_progress_percentage_clamped_to_100() {
    let mut progress = DownloadProgress::new(Some(1000));
    progress.update(1200, 0.0); // More than total

    let percentage = progress.percentage();

    assert_eq!(percentage, Some(100.0), "Progress should never exceed 100%");
}

#[test]
fn test_eta_calculation() {
    let mut progress = DownloadProgress::new(Some(1000));
    progress.update(250, 250.0); // 250 bytes per second

    let eta_seconds = progress.estimated_time_remaining();

    assert!(eta_seconds.is_some(), "ETA should be calculable");
    let eta = eta_seconds.unwrap();
    assert!(
        (2..=4).contains(&eta),
        "ETA should be approximately 3 seconds, got {}",
        eta
    );
}

#[test]
fn test_eta_calculation_zero_speed() {
    let mut progress = DownloadProgress::new(Some(1000));
    progress.update(250, 0.0); // Zero speed - stalled download

    let eta = progress.estimated_time_remaining();

    assert!(
        eta.is_none(),
        "ETA should be None when speed is zero (stalled download)"
    );
}

#[test]
fn test_eta_calculation_completed() {
    let mut progress = DownloadProgress::new(Some(1000));
    progress.update(1000, 250.0);

    let eta = progress.estimated_time_remaining();

    assert!(
        eta == Some(0),
        "ETA should be 0 for completed download, got {:?}",
        eta
    );
}

#[test]
fn test_speed_calculation() {
    let test_cases = vec![
        (1000.0, "1 KB/s"),
        (1_000_000.0, "1 MB/s"),
        (500.5, "500.5 B/s"),
    ];

    for (speed_bps, expected_desc) in test_cases {
        let mut progress = DownloadProgress::new(Some(1000));
        progress.update(500, speed_bps);

        let actual_speed = progress.bytes_per_second();

        assert_eq!(
            actual_speed, speed_bps,
            "Speed should be {} ({})",
            speed_bps, expected_desc
        );
    }
}

#[test]
fn test_speed_clamped_to_zero() {
    let mut progress = DownloadProgress::new(Some(1000));
    progress.update(500, -100.0); // Negative speed (should be clamped)

    let actual_speed = progress.bytes_per_second();

    assert_eq!(actual_speed, 0.0, "Negative speed should be clamped to 0.0");
}

fn queued_session() -> DownloadSession {
    DownloadSession::new(
        "queued".to_string(),
        "https://example.com/queued.bin".to_string(),
        temp_path("queued.bin"),
        Some(1000),
        None,
    )
    .unwrap()
}

#[test]
fn a_queued_session_can_be_paused_or_failed() {
    assert!(DownloadState::Pending.can_transition_to(&DownloadState::Paused));
    assert!(DownloadState::Pending.can_transition_to(&DownloadState::Failed));
    assert!(DownloadState::Pending.can_pause());

    let mut paused = queued_session();
    paused.pause().unwrap();
    assert_eq!(paused.state(), &DownloadState::Paused);

    let mut failed = queued_session();
    failed.fail("never started".to_string()).unwrap();
    assert_eq!(failed.state(), &DownloadState::Failed);
    assert_eq!(failed.error_message(), Some("never started"));
}

#[test]
fn a_rejected_failure_changes_nothing() {
    let mut session = queued_session();
    session.start().unwrap();
    session.complete().unwrap();

    assert!(session.fail("too late".to_string()).is_err());

    assert_eq!(session.state(), &DownloadState::Completed);
    assert_eq!(session.error_message(), None);
    assert_eq!(session.retry_count(), 0);
}

#[test]
fn pending_and_downloading_sessions_go_stale_but_paused_ones_do_not() {
    let cutoff = Utc::now() + chrono::Duration::seconds(1);
    let pending = queued_session();
    assert!(pending.is_stale_at(cutoff));

    let mut downloading = queued_session();
    downloading.start().unwrap();
    assert!(downloading.is_stale_at(cutoff));

    let mut paused = queued_session();
    paused.pause().unwrap();
    assert!(!paused.is_stale_at(cutoff));
    assert!(!pending.is_stale_at(Utc::now() - chrono::Duration::seconds(10)));
}

#[test]
fn a_name_is_kept_without_a_model_id() {
    let session = queued_session().with_model_name("Vision projector".to_string());
    assert_eq!(session.model_name(), Some("Vision projector"));
    assert_eq!(session.model_id(), None);
}

#[test]
fn the_working_file_is_the_full_name_plus_part() {
    let session = queued_session();
    assert_eq!(
        session.working_path(),
        temp_path("queued.bin.part"),
        "the extension is kept, not replaced"
    );
    assert_eq!(
        part_path(Path::new("/m/x.gguf")),
        PathBuf::from("/m/x.gguf.part")
    );
}

#[test]
fn failure_messages_are_one_plain_sentence() {
    let checksum = DownloadError::ChecksumMismatch {
        expected: "a".repeat(64),
        actual: "b".repeat(64),
    }
    .user_message();
    assert_eq!(
        checksum,
        "The downloaded file didn't match its published checksum, so it was discarded. Try again."
    );

    let disk = DownloadError::InsufficientDiskSpace {
        required: 639_000_000,
        available: 120_400_000,
    }
    .user_message();
    let network =
        DownloadError::NetworkError("HEAD request failed: dns error".into()).user_message();
    assert_eq!(
        network,
        "Couldn't reach the download host. Check your connection and try again."
    );
    assert_eq!(disk, "Not enough disk space: 639 MB needed, 120 MB free.");
}

#[test]
fn byte_counts_use_the_units_the_ui_shows() {
    assert_eq!(human_bytes(512), "512 B");
    assert_eq!(human_bytes(1_500), "1.5 KB");
    assert_eq!(human_bytes(639_000_000), "639 MB");
    assert_eq!(human_bytes(2_400_000_000), "2.4 GB");
}

#[test]
fn a_busy_destination_error_reads_as_a_sentence() {
    let message = DownloadError::DestinationBusy("/m/x.gguf".to_string()).to_string();
    assert!(message.starts_with("This file is already downloading"));
}

#[test]
fn resuming_a_failed_session_clears_its_error_and_queues_it() {
    let mut session = queued_session();
    session.start().unwrap();
    session.fail("Network error".to_string()).unwrap();

    session.resume().unwrap();

    assert_eq!(session.state(), &DownloadState::Pending);
    assert_eq!(session.error_message(), None);
    assert!(DownloadState::Paused.can_transition_to(&DownloadState::Pending));
}

#[test]
fn resuming_a_running_session_names_pending_as_the_target() {
    let mut session = queued_session();
    session.start().unwrap();
    match session.resume().unwrap_err() {
        DownloadError::InvalidStateTransition { to, .. } => assert_eq!(to, DownloadState::Pending),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn http_and_io_failures_read_as_plain_sentences() {
    let http = |status| DownloadError::HttpError {
        status,
        message: "raw".into(),
    };
    assert_eq!(
        http(403).user_message(),
        "The download host refused access (HTTP 403)."
    );
    assert_eq!(
        http(401).user_message(),
        "The download host refused access (HTTP 401)."
    );
    assert_eq!(
        http(404).user_message(),
        "The file is no longer at its download address (HTTP 404)."
    );
    assert_eq!(
        http(410).user_message(),
        "The file is no longer at its download address (HTTP 410)."
    );
    assert_eq!(
        http(429).user_message(),
        "The download host is limiting requests. Try again in a few minutes."
    );
    assert_eq!(
        http(503).user_message(),
        "The download host returned an error (HTTP 503). Try again later."
    );
    assert_eq!(
        DownloadError::MaxRetriesExceeded.user_message(),
        "The download kept failing, so it stopped. Check your connection and try again."
    );
    assert_eq!(
        DownloadError::IoError("os error 28".into()).user_message(),
        "Couldn't save the download to disk. Check that the drive has space and can be written to."
    );
    assert_eq!(
        DownloadError::InvalidResponse("bad redirect".into()).user_message(),
        "The download host sent an unexpected response. Try again later."
    );
}

#[test]
fn start_errors_reach_the_app_as_the_plain_sentence_with_their_kind_kept() {
    use crate::shared::error::AppError;
    let offline: AppError = DownloadError::NetworkError(
        "HEAD request failed: error sending request for url (x)".into(),
    )
    .into();
    assert!(
        matches!(&offline, AppError::Network(m) if m.starts_with("Couldn't reach") && !m.contains("HEAD"))
    );

    let http: AppError = DownloadError::HttpError {
        status: 404,
        message: "File not found (404)".into(),
    }
    .into();
    assert!(
        matches!(&http, AppError::Network(m) if m.contains("no longer at its download address"))
    );

    let io: AppError = DownloadError::IoError("os error 28".into()).into();
    assert!(matches!(&io, AppError::FileSystem(m) if m.starts_with("Couldn't save")));

    let disk: AppError = DownloadError::InsufficientDiskSpace {
        required: 639_000_000,
        available: 1_000_000,
    }
    .into();
    assert!(matches!(&disk, AppError::FileSystem(m) if m.contains("639 MB")));

    let sum: AppError = DownloadError::ChecksumMismatch {
        expected: "a".into(),
        actual: "b".into(),
    }
    .into();
    assert!(matches!(&sum, AppError::InvalidData(m) if m.contains("checksum")));

    let retries: AppError = DownloadError::MaxRetriesExceeded.into();
    assert!(matches!(&retries, AppError::Other(m) if m.contains("kept failing")));

    let response: AppError = DownloadError::InvalidResponse("x".into()).into();
    assert!(matches!(&response, AppError::Network(m) if m.contains("unexpected response")));
}
