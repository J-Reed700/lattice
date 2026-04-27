//! Smoke tests for Backup plugin DTOs

use lattice::features::backup::commands::BackupInfo;

#[test]
fn test_backup_info_creation() {
    let info = BackupInfo {
        path: "/backups/lattice-2024-01-15.lattice-backup".to_string(),
        name: "lattice-2024-01-15.lattice-backup".to_string(),
        created_at: "2024-01-15T10:30:00Z".to_string(),
        version: "1.0".to_string(),
        file_count: 0,
        size: 52428800,
    };

    assert_eq!(info.name, "lattice-2024-01-15.lattice-backup");
    assert_eq!(info.version, "1.0");
    assert_eq!(info.size, 52428800);
}

#[test]
fn test_backup_info_empty() {
    let info = BackupInfo {
        path: "".to_string(),
        name: "".to_string(),
        created_at: "".to_string(),
        version: "1.0".to_string(),
        file_count: 0,
        size: 0,
    };

    assert_eq!(info.size, 0);
    assert!(info.name.is_empty());
}

#[test]
fn test_backup_info_large_file() {
    let info = BackupInfo {
        path: "/backups/large.lattice-backup".to_string(),
        name: "large.lattice-backup".to_string(),
        created_at: "2024-01-20T12:00:00Z".to_string(),
        version: "1.0".to_string(),
        file_count: 0,
        size: 1073741824, // 1 GB
    };

    assert_eq!(info.size, 1073741824);
}

#[test]
fn test_backup_info_serialization() {
    let info = BackupInfo {
        path: "/test/backup.lattice-backup".to_string(),
        name: "backup.lattice-backup".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        version: "1.0".to_string(),
        file_count: 10,
        size: 1024,
    };

    let json = serde_json::to_string(&info).expect("Failed to serialize");
    assert!(json.contains("backup.lattice-backup"));
    assert!(json.contains("1.0"));
}

#[test]
fn test_backup_info_deserialization() {
    let json = r#"{
        "path": "/test.lattice-backup",
        "name": "test.lattice-backup",
        "createdAt": "2024-01-01T00:00:00Z",
        "version": "1.0",
        "fileCount": 5,
        "size": 2048
    }"#;

    let info: BackupInfo = serde_json::from_str(json).expect("Failed to deserialize");
    assert_eq!(info.name, "test.lattice-backup");
    assert_eq!(info.file_count, 5);
    assert_eq!(info.size, 2048);
}
