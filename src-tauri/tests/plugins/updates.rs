//! Smoke tests for Updates plugin DTOs

use lattice::features::updates::dto::{UpdateInfoDto, VersionInfoDto};

#[test]
fn test_update_info_dto_available() {
    let update_info = UpdateInfoDto {
        available: true,
        current_version: "1.1.0".to_string(),
        latest_version: Some("1.2.0".to_string()),
        download_url: Some(
            "https://github.com/owner/repo/releases/download/v1.2.0/app.dmg".to_string(),
        ),
        release_notes: Some("# What's New\n- Feature 1\n- Bug fix 2".to_string()),
    };

    assert!(update_info.available);
    assert_eq!(update_info.latest_version.as_ref().unwrap(), "1.2.0");
    assert_eq!(update_info.current_version, "1.1.0");
    assert!(update_info
        .download_url
        .as_ref()
        .unwrap()
        .contains("v1.2.0"));
}

#[test]
fn test_update_info_dto_not_available() {
    let update_info = UpdateInfoDto {
        available: false,
        current_version: "1.1.0".to_string(),
        latest_version: None,
        download_url: None,
        release_notes: None,
    };

    assert!(!update_info.available);
    assert!(update_info.latest_version.is_none());
}

#[test]
fn test_version_info_dto_creation() {
    let version_info = VersionInfoDto {
        version: "1.1.0".to_string(),
        build_date: Some("2024-01-10T08:00:00Z".to_string()),
        commit_hash: Some("abc1234".to_string()),
    };

    assert_eq!(version_info.version, "1.1.0");
    assert_eq!(version_info.commit_hash.as_ref().unwrap(), "abc1234");
    assert_eq!(
        version_info.build_date.as_ref().unwrap(),
        "2024-01-10T08:00:00Z"
    );
}

#[test]
fn test_version_info_dto_minimal() {
    let version_info = VersionInfoDto {
        version: "0.1.0".to_string(),
        build_date: None,
        commit_hash: None,
    };

    assert_eq!(version_info.version, "0.1.0");
    assert!(version_info.commit_hash.is_none());
}

#[test]
fn test_update_info_serialization() {
    let update_info = UpdateInfoDto {
        available: true,
        current_version: "1.5.0".to_string(),
        latest_version: Some("2.0.0".to_string()),
        download_url: Some("https://example.com/download".to_string()),
        release_notes: Some("New features".to_string()),
    };

    let json = serde_json::to_string(&update_info).expect("Failed to serialize");
    assert!(json.contains("\"available\":true"));
    assert!(json.contains("\"latestVersion\""));
}

#[test]
fn test_version_info_serialization() {
    let version_info = VersionInfoDto {
        version: "1.0.0".to_string(),
        build_date: Some("2024-01-01".to_string()),
        commit_hash: Some("xyz7890".to_string()),
    };

    let json = serde_json::to_string(&version_info).expect("Failed to serialize");
    assert!(json.contains("\"version\":\"1.0.0\""));
    assert!(json.contains("xyz7890"));
}
