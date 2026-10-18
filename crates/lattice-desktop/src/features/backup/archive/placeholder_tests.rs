//! Tests for cloud-folder classification, file availability and hydration waits.
use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Build a path from components so the tests are spelling-agnostic across
/// hosts (`/a/b` on POSIX, `\a\b` on Windows).
fn p(parts: &[&str]) -> PathBuf {
    let mut path = PathBuf::new();
    for part in parts {
        path.push(part);
    }
    path
}

fn no_env(_: &str) -> Option<String> {
    None
}

fn home() -> PathBuf {
    p(&["/", "home", "u"])
}

// -- classification -----------------------------------------------------

#[test]
fn classify_with_home_recognises_every_cloud_root() {
    let home = home();
    let cases: Vec<(PathBuf, CloudProvider)> = vec![
        // macOS File Provider roots under ~/Library/CloudStorage.
        (
            p(&[
                "/",
                "home",
                "u",
                "Library",
                "CloudStorage",
                "Dropbox",
                "b.lattice-backup",
            ]),
            CloudProvider::Dropbox,
        ),
        (
            p(&[
                "/",
                "home",
                "u",
                "Library",
                "CloudStorage",
                "Dropbox-Acme Inc",
                "b",
            ]),
            CloudProvider::Dropbox,
        ),
        (
            p(&[
                "/",
                "home",
                "u",
                "Library",
                "CloudStorage",
                "OneDrive-Personal",
                "b",
            ]),
            CloudProvider::OneDrive,
        ),
        (
            p(&[
                "/",
                "home",
                "u",
                "Library",
                "CloudStorage",
                "GoogleDrive-a@b.com",
                "b",
            ]),
            CloudProvider::GoogleDrive,
        ),
        (
            p(&["/", "home", "u", "Library", "CloudStorage", "Box-Box", "b"]),
            CloudProvider::Box,
        ),
        (
            p(&[
                "/",
                "home",
                "u",
                "Library",
                "CloudStorage",
                "pCloud-josh",
                "b",
            ]),
            CloudProvider::Other("pCloud".to_string()),
        ),
        // iCloud.
        (
            p(&[
                "/",
                "home",
                "u",
                "Library",
                "Mobile Documents",
                "com~apple~CloudDocs",
                "b",
            ]),
            CloudProvider::ICloud,
        ),
        (
            p(&["/", "home", "u", "iCloud Drive", "b"]),
            CloudProvider::ICloud,
        ),
        // Windows: %USERPROFILE%\iCloudDrive.
        (
            p(&["/", "home", "u", "iCloudDrive", "b"]),
            CloudProvider::ICloud,
        ),
        // Dropbox, including the team-suffixed folder names.
        (
            p(&["/", "home", "u", "Dropbox", "b"]),
            CloudProvider::Dropbox,
        ),
        (
            p(&["/", "home", "u", "Dropbox (Personal)", "b"]),
            CloudProvider::Dropbox,
        ),
        (
            p(&["/", "home", "u", "Dropbox (Acme Inc)", "b"]),
            CloudProvider::Dropbox,
        ),
        // OneDrive, personal and tenant-joined.
        (
            p(&["/", "home", "u", "OneDrive", "b"]),
            CloudProvider::OneDrive,
        ),
        (
            p(&["/", "home", "u", "OneDrive - Contoso", "b"]),
            CloudProvider::OneDrive,
        ),
        // Google Drive, desktop client and Linux conventions.
        (
            p(&["/", "home", "u", "Google Drive", "b"]),
            CloudProvider::GoogleDrive,
        ),
        (
            p(&["/", "home", "u", "My Drive", "b"]),
            CloudProvider::GoogleDrive,
        ),
        (
            p(&["/", "home", "u", "gdrive", "b"]),
            CloudProvider::GoogleDrive,
        ),
        (
            p(&["/", "home", "u", "google-drive", "b"]),
            CloudProvider::GoogleDrive,
        ),
        // Box.
        (p(&["/", "home", "u", "Box", "b"]), CloudProvider::Box),
        (p(&["/", "home", "u", "Box Sync", "b"]), CloudProvider::Box),
        // Insync (Linux) mirrors several services; name the client.
        (
            p(&["/", "home", "u", "Insync", "a@b.com", "Google Drive", "b"]),
            CloudProvider::Other("Insync".to_string()),
        ),
        // Legacy Drive File Stream mount, outside the home directory.
        (
            p(&["/", "Volumes", "GoogleDrive", "My Drive", "b"]),
            CloudProvider::GoogleDrive,
        ),
        (
            p(&["/", "Volumes", "GoogleDrive-a@b.com", "b"]),
            CloudProvider::GoogleDrive,
        ),
    ];

    for (path, expected) in cases {
        assert_eq!(
            classify_with_home(&path, &home, &no_env),
            Some(expected),
            "path {}",
            path.display()
        );
    }
}

#[test]
fn classify_with_home_ignores_ordinary_folders() {
    let home = home();
    let cases = vec![
        p(&["/", "home", "u", "Documents", "b.lattice-backup"]),
        p(&[
            "/",
            "home",
            "u",
            "Library",
            "Application Support",
            "Lattice",
            "b",
        ]),
        // The CloudStorage container itself is not a synced folder.
        p(&["/", "home", "u", "Library", "CloudStorage"]),
        p(&["/", "home", "u", "DropboxNotReally", "b"]),
        p(&["/", "home", "u", "OneDriveArchive", "b"]),
        p(&["/", "tmp", "b.lattice-backup"]),
        p(&["/", "Volumes", "Backup Disk", "b"]),
        // Another user's Dropbox is not below this home.
        p(&["/", "home", "other", "Dropbox", "b"]),
    ];
    for path in cases {
        assert_eq!(
            classify_with_home(&path, &home, &no_env),
            None,
            "path {}",
            path.display()
        );
    }
}

#[test]
fn classify_with_home_requires_a_home_to_match_home_relative_rules() {
    // An unresolvable home must not turn every path into a cloud folder.
    assert_eq!(
        classify_with_home(&p(&["/", "Dropbox", "b"]), Path::new(""), &no_env),
        None
    );
}

#[test]
fn classify_with_home_honours_windows_onedrive_env_vars() {
    let home = home();
    for var in ONEDRIVE_ENV_VARS {
        let root = p(&["/", "mnt", "work", "OneDriveRoot"]);
        let value = root.to_string_lossy().into_owned();
        let env = move |name: &str| {
            if name == var {
                Some(value.clone())
            } else {
                None
            }
        };
        let file = root.join("Lattice Backups").join("b.lattice-backup");
        assert_eq!(
            classify_with_home(&file, &home, &env),
            Some(CloudProvider::OneDrive),
            "env var {var}"
        );
        // A sibling directory with the same prefix is not inside the root.
        let sibling = p(&["/", "mnt", "work", "OneDriveRootOther", "b"]);
        assert_eq!(
            classify_with_home(&sibling, &home, &env),
            None,
            "env var {var}"
        );
    }
}

#[test]
fn classify_with_home_honours_userprofile_when_it_differs_from_home() {
    let profile = p(&["/", "Users", "roaming"]);
    let value = profile.to_string_lossy().into_owned();
    let env = |name: &str| {
        if name == "USERPROFILE" {
            Some(value.clone())
        } else {
            None
        }
    };
    // The Windows rules: %USERPROFILE%\Dropbox, \Google Drive, \iCloudDrive.
    assert_eq!(
        classify_with_home(&profile.join("Dropbox").join("b"), &home(), &env),
        Some(CloudProvider::Dropbox)
    );
    assert_eq!(
        classify_with_home(&profile.join("Google Drive").join("b"), &home(), &env),
        Some(CloudProvider::GoogleDrive)
    );
    assert_eq!(
        classify_with_home(&profile.join("iCloudDrive").join("b"), &home(), &env),
        Some(CloudProvider::ICloud)
    );
    assert_eq!(
        classify_with_home(&profile.join("Documents"), &home(), &env),
        None
    );
}

#[cfg(windows)]
#[test]
fn classify_with_home_is_case_insensitive_on_windows() {
    let home = home();
    assert_eq!(
        classify_with_home(&p(&["/", "home", "u", "dropbox", "b"]), &home, &no_env),
        Some(CloudProvider::Dropbox)
    );
    assert_eq!(
        classify_with_home(&p(&["/", "HOME", "U", "OneDrive", "b"]), &home, &no_env),
        Some(CloudProvider::OneDrive)
    );
}

#[cfg(windows)]
#[test]
fn classify_with_home_keeps_drive_letters_apart() {
    let home = p(&["C:", "/", "Users", "u"]);
    assert_eq!(
        classify_with_home(
            &p(&["C:", "/", "Users", "u", "Dropbox", "b"]),
            &home,
            &no_env
        ),
        Some(CloudProvider::Dropbox)
    );
    assert_eq!(
        classify_with_home(
            &p(&["D:", "/", "Users", "u", "Dropbox", "b"]),
            &home,
            &no_env
        ),
        None
    );
}

#[test]
fn provider_display_names_are_product_names() {
    assert_eq!(CloudProvider::ICloud.display_name(), "iCloud Drive");
    assert_eq!(CloudProvider::GoogleDrive.display_name(), "Google Drive");
    assert_eq!(
        CloudProvider::Other("pCloud".to_string()).display_name(),
        "pCloud"
    );
}

#[cfg(unix)]
#[test]
fn classify_resolving_matches_through_a_symlinked_home() {
    let tmp = tempfile::tempdir().unwrap();
    let real_home = tmp.path().join("real-home");
    std::fs::create_dir_all(real_home.join("Dropbox")).unwrap();
    let link_home = tmp.path().join("link-home");
    std::os::unix::fs::symlink(&real_home, &link_home).unwrap();

    let file = link_home.join("Dropbox").join("b.lattice-backup");
    std::fs::write(&file, b"x").unwrap();

    // The configured home is the real directory; the path arrived via the
    // symlink. Only the canonicalising pass can connect the two.
    assert_eq!(classify_with_home(&file, &real_home, &no_env), None);
    assert_eq!(
        classify_resolving(&file, &real_home, &no_env),
        Some(CloudProvider::Dropbox)
    );
}

// -- availability -------------------------------------------------------

#[test]
fn file_availability_reports_local_for_ordinary_files_and_directories() {
    let tmp = tempfile::tempdir().unwrap();

    let file = tmp.path().join("archive.lattice-backup");
    std::fs::write(&file, b"contents are on disk").unwrap();
    assert_eq!(file_availability(&file), FileAvailability::Local);

    // Zero-length is trivially local: there are no bytes to download.
    let empty = tmp.path().join("empty.lattice-backup");
    std::fs::write(&empty, b"").unwrap();
    assert_eq!(file_availability(&empty), FileAvailability::Local);

    assert_eq!(file_availability(tmp.path()), FileAvailability::Local);
}

#[test]
fn file_availability_reports_missing_for_absent_paths() {
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(
        file_availability(&tmp.path().join("nope.lattice-backup")),
        FileAvailability::Missing
    );
}

#[cfg(unix)]
#[test]
fn file_availability_stats_the_symlink_target() {
    // Regression guard: APFS reports a symlink's own inode with a non-zero
    // size and `st_blocks == 0`, which is exactly the shape of an evicted
    // file. Stating the link instead of its target would call every symlink
    // a placeholder.
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("archive.lattice-backup");
    std::fs::write(&target, b"contents are on disk").unwrap();

    let link = tmp.path().join("link.lattice-backup");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert_eq!(file_availability(&link), FileAvailability::Local);

    let dangling = tmp.path().join("dangling.lattice-backup");
    std::os::unix::fs::symlink(tmp.path().join("gone"), &dangling).unwrap();
    assert_eq!(file_availability(&dangling), FileAvailability::Missing);
}

// -- hydration ----------------------------------------------------------

#[tokio::test]
async fn wait_for_hydration_returns_immediately_for_a_local_file() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("archive.lattice-backup");
    std::fs::write(&file, b"local").unwrap();

    let mut ticks: Vec<Duration> = Vec::new();
    {
        let mut progress = |elapsed: Duration| ticks.push(elapsed);
        wait_for_hydration(&file, Duration::from_secs(5), &mut progress)
            .await
            .unwrap();
    }
    assert!(ticks.is_empty(), "a local file must not report progress");
}

#[tokio::test]
async fn wait_for_hydration_with_succeeds_once_the_file_becomes_local() {
    let remaining = AtomicUsize::new(3);
    let availability = |_: &Path| {
        if remaining.load(Ordering::Relaxed) == 0 {
            FileAvailability::Local
        } else {
            remaining.fetch_sub(1, Ordering::Relaxed);
            FileAvailability::Placeholder {
                provider: Some(CloudProvider::ICloud),
            }
        }
    };

    let mut ticks: Vec<Duration> = Vec::new();
    {
        let mut progress = |elapsed: Duration| ticks.push(elapsed);
        wait_for_hydration_with(
            Path::new("/nonexistent/lattice/archive.lattice-backup"),
            Duration::from_secs(30),
            Duration::from_millis(1),
            &mut progress,
            &availability,
        )
        .await
        .unwrap();
    }

    // One immediate report plus one per poll.
    assert_eq!(ticks.len(), 4);
    assert!(
        ticks.windows(2).all(|w| w[0] <= w[1]),
        "elapsed must not go backwards"
    );
}

#[tokio::test]
async fn wait_for_hydration_with_times_out_naming_the_provider() {
    let availability = |_: &Path| FileAvailability::Placeholder {
        provider: Some(CloudProvider::OneDrive),
    };
    let mut progress = |_: Duration| {};
    let err = wait_for_hydration_with(
        Path::new("/nonexistent/lattice/archive.lattice-backup"),
        Duration::from_millis(30),
        Duration::from_millis(5),
        &mut progress,
        &availability,
    )
    .await
    .unwrap_err();

    match err {
        ArchiveError::NotHydrated(name) => assert_eq!(name, "OneDrive"),
        other => panic!("expected NotHydrated, got {other:?}"),
    }
}

#[tokio::test]
async fn wait_for_hydration_with_falls_back_to_a_generic_provider_label() {
    let availability = |_: &Path| FileAvailability::Placeholder { provider: None };
    let mut progress = |_: Duration| {};
    let err = wait_for_hydration_with(
        Path::new("/nonexistent/lattice/archive.lattice-backup"),
        Duration::from_millis(10),
        Duration::from_millis(5),
        &mut progress,
        &availability,
    )
    .await
    .unwrap_err();

    match err {
        ArchiveError::NotHydrated(name) => assert_eq!(name, UNKNOWN_PROVIDER_LABEL),
        other => panic!("expected NotHydrated, got {other:?}"),
    }
}

#[tokio::test]
async fn wait_for_hydration_with_reports_missing_and_tolerates_unknown() {
    let mut progress = |_: Duration| {};

    let missing = |_: &Path| FileAvailability::Missing;
    let err = wait_for_hydration_with(
        Path::new("/nonexistent/lattice/archive.lattice-backup"),
        Duration::from_millis(10),
        Duration::from_millis(5),
        &mut progress,
        &missing,
    )
    .await
    .unwrap_err();
    match err {
        ArchiveError::Io(e) => assert_eq!(e.kind(), std::io::ErrorKind::NotFound),
        other => panic!("expected Io(NotFound), got {other:?}"),
    }

    // Unknown must not block the restore behind a guess.
    let unknown = |_: &Path| FileAvailability::Unknown;
    wait_for_hydration_with(
        Path::new("/nonexistent/lattice/archive.lattice-backup"),
        Duration::from_millis(10),
        Duration::from_millis(5),
        &mut progress,
        &unknown,
    )
    .await
    .unwrap();
}
