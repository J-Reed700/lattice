//! Storage roots Lattice keeps outside the app data directory.
//!
//! Everything else lives under the platform app data directory that Tauri
//! resolves (`lattice.db`, search indexes, `folder-index/`, `learning-packs/`).
//! The two roots here hold user content that a backup must carry, so every
//! owner and the backup writer resolve them from this one place.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use crate::shared::error::{AppError, Result};

/// When set and not empty, replaces `~/.lattice` as the parent of every root
/// below, and puts the default vault at `<LATTICE_HOME>/Lattice` instead of
/// `~/Lattice`. A second copy of the app on one machine sets it to a folder of
/// its own, so neither collects or writes into the other's files.
pub const LATTICE_HOME_ENV: &str = "LATTICE_HOME";

// global-state: which home the process has, as `LATTICE_HOME` is; any process
// the app's entry point does not mark keeps a scratch one.
static USERS_HOME: AtomicBool = AtomicBool::new(false);

/// Let this process use `~/.lattice` when `LATTICE_HOME` is not set. Only the
/// app's own entry point calls it: any other process (a test, an eval) gets a
/// scratch home of its own, so none can write into the user's library.
pub fn use_the_users_home() {
    USERS_HOME.store(true, Ordering::SeqCst);
}

/// `$LATTICE_HOME`, else `~/.lattice` for the app and a scratch home for any
/// other process: the parent of every root below.
fn lattice_home() -> Result<PathBuf> {
    if let Some(home) = lattice_home_override() {
        return Ok(home);
    }
    if !USERS_HOME.load(Ordering::SeqCst) {
        return Ok(scratch_home().clone());
    }
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::InvalidState("Cannot determine home directory".to_string()))?;
    Ok(home.join(".lattice"))
}

/// `$LATTICE_HOME`, when set and not empty.
fn lattice_home_override() -> Option<PathBuf> {
    std::env::var_os(LATTICE_HOME_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// One per process, under the system temp folder, laid out as a home is.
fn scratch_home() -> &'static PathBuf {
    // global-state: the process's scratch home, one per process like `USERS_HOME`.
    static SCRATCH: OnceLock<PathBuf> = OnceLock::new();
    SCRATCH.get_or_init(|| {
        std::env::temp_dir()
            .join(format!("lattice-scratch-home-{}", uuid::Uuid::new_v4()))
            .join(".lattice")
    })
}

/// Holds the id of the database that owns the files in the roots below:
/// `~/.lattice/owner`.
pub fn library_owner_file() -> Result<PathBuf> {
    Ok(lattice_home()?.join("owner"))
}

/// Content-addressed copies of imported files: `~/.lattice/files/{sha256}/{name}`.
pub fn library_root() -> Result<PathBuf> {
    Ok(lattice_home()?.join("files"))
}

/// Archived web pages that web documents and citations open: `~/.lattice/web-archive`.
pub fn web_archive_root() -> Result<PathBuf> {
    Ok(lattice_home()?.join("web-archive"))
}

/// Frozen copies of cited web pages: `~/.lattice/web-snapshots/<id>.html`.
pub fn web_snapshots_root() -> Result<PathBuf> {
    Ok(lattice_home()?.join("web-snapshots"))
}

/// The markdown vault: the configured path, or when the setting is empty
/// `<LATTICE_HOME>/Lattice` if that is set, else `~/Lattice` for the app and
/// `Lattice` beside the scratch lattice home for any other process.
pub fn resolve_vault_root(configured: &str) -> Option<PathBuf> {
    let trimmed = configured.trim();
    if !trimmed.is_empty() {
        return Some(PathBuf::from(trimmed));
    }
    if let Some(home) = lattice_home_override() {
        return Some(home.join("Lattice"));
    }
    if !USERS_HOME.load(Ordering::SeqCst) {
        return scratch_home().parent().map(|home| home.join("Lattice"));
    }
    dirs::home_dir().map(|h| h.join("Lattice"))
}

/// Pictures assistant answers show inline: `~/.lattice/chat-images/<id>.<ext>`.
pub fn chat_images_root() -> Result<PathBuf> {
    Ok(lattice_home()?.join("chat-images"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Set on a child copy of this test binary so that [`print_roots`] runs;
    /// [`CLAIMED`] as its value makes the child claim the user's home first.
    const PROBE: &str = "LATTICE_ROOTS_PROBE";
    const CLAIMED: &str = "claimed";
    /// The vault setting the child passes to `resolve_vault_root`; unset means
    /// none configured.
    const CONFIGURED_VAULT: &str = "LATTICE_ROOTS_CONFIGURED_VAULT";

    /// What a child process resolved: the name of each root and where it is.
    /// The variable is set only on the child, never on the process running the
    /// tests, because another test reading a root would see it.
    fn roots_with(configured: Option<&str>) -> Vec<(String, PathBuf)> {
        roots_of_child(configured, "1")
    }

    fn roots_of_child(configured: Option<&str>, probe: &str) -> Vec<(String, PathBuf)> {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap());
        child
            .args([
                "--exact",
                "shared::fs::roots::tests::print_roots",
                "--nocapture",
            ])
            .env(PROBE, probe);
        match configured {
            Some(value) => child.env(LATTICE_HOME_ENV, value),
            None => child.env_remove(LATTICE_HOME_ENV),
        };
        let output = child.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| {
                let (_, fields) = line.split_once("ROOT\t")?;
                let (name, path) = fields.split_once('\t')?;
                Some((name.to_string(), PathBuf::from(path)))
            })
            .collect()
    }

    /// Prints every root for [`roots_with`]; does nothing in a normal run.
    #[test]
    fn print_roots() {
        let Some(probe) = std::env::var_os(PROBE) else {
            return;
        };
        if probe == CLAIMED {
            use_the_users_home();
        }
        for (name, root) in [
            ("home", lattice_home()),
            ("files", library_root()),
            ("web-archive", web_archive_root()),
            ("web-snapshots", web_snapshots_root()),
            ("chat-images", chat_images_root()),
            ("owner", library_owner_file()),
        ] {
            println!("ROOT\t{name}\t{}", root.unwrap().display());
        }
        let configured = std::env::var(CONFIGURED_VAULT).unwrap_or_default();
        if let Some(vault) = resolve_vault_root(&configured) {
            println!("VAULT\t{}", vault.display());
        }
    }

    /// The vault a child process resolves when none is configured.
    fn default_vault_of_child(probe: &str) -> Option<PathBuf> {
        vault_of_child(probe, None, "")
    }

    /// The vault a child process resolves for `configured`, with
    /// `LATTICE_HOME` set to `home` or removed.
    fn vault_of_child(probe: &str, home: Option<&str>, configured: &str) -> Option<PathBuf> {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap());
        child
            .args([
                "--exact",
                "shared::fs::roots::tests::print_roots",
                "--nocapture",
            ])
            .env(PROBE, probe);
        if configured.is_empty() {
            child.env_remove(CONFIGURED_VAULT);
        } else {
            child.env(CONFIGURED_VAULT, configured);
        }
        match home {
            Some(value) => child.env(LATTICE_HOME_ENV, value),
            None => child.env_remove(LATTICE_HOME_ENV),
        };
        let output = child.output().unwrap();
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .find_map(|line| {
                line.split_once("VAULT\t")
                    .map(|(_, path)| PathBuf::from(path))
            })
    }

    #[test]
    fn the_app_that_claims_the_users_home_gets_its_vault_by_default() {
        assert_eq!(
            default_vault_of_child(CLAIMED),
            Some(dirs::home_dir().unwrap().join("Lattice"))
        );
    }

    /// A test that saves a note with no vault configured must not write into
    /// the user's own `~/Lattice`.
    #[test]
    fn a_process_that_has_not_claimed_the_users_home_gets_a_scratch_vault_by_default() {
        let users = dirs::home_dir().unwrap().join("Lattice");
        let vault = default_vault_of_child("1").expect("a default vault");
        assert!(
            !vault.starts_with(&users),
            "the default vault is the user's at {}",
            vault.display()
        );
    }

    #[test]
    fn lattice_home_moves_the_default_vault_for_every_process() {
        let dir = tempfile::tempdir().unwrap();
        let second = dir.path().join("second");
        for probe in [CLAIMED, "1"] {
            assert_eq!(
                vault_of_child(probe, Some(second.to_str().unwrap()), ""),
                Some(second.join("Lattice")),
                "probe {probe}"
            );
        }
    }

    #[test]
    fn an_empty_lattice_home_leaves_the_default_vault_as_if_unset() {
        let users = dirs::home_dir().unwrap().join("Lattice");
        assert_eq!(
            vault_of_child(CLAIMED, Some(""), ""),
            Some(users.clone()),
            "a claimed process"
        );
        let vault = vault_of_child("1", Some(""), "").expect("a default vault");
        assert_ne!(vault, users);
        assert!(vault.ends_with("Lattice"));
    }

    #[test]
    fn a_configured_vault_wins_over_lattice_home() {
        let dir = tempfile::tempdir().unwrap();
        let second = dir.path().join("second");
        let configured = dir.path().join("mine");
        for probe in [CLAIMED, "1"] {
            for home in [None, Some(second.to_str().unwrap())] {
                assert_eq!(
                    vault_of_child(probe, home, configured.to_str().unwrap()),
                    Some(configured.clone()),
                    "probe {probe}, home {home:?}"
                );
            }
        }
    }

    fn all_under(home: &Path) -> Vec<(String, PathBuf)> {
        let mut roots = vec![("home".to_string(), home.to_path_buf())];
        roots.extend(
            [
                "files",
                "web-archive",
                "web-snapshots",
                "chat-images",
                "owner",
            ]
            .map(|name| (name.to_string(), home.join(name))),
        );
        roots
    }

    #[test]
    fn lattice_home_moves_every_root() {
        let dir = tempfile::tempdir().unwrap();
        let second = dir.path().join("second");
        assert_eq!(
            roots_with(Some(second.to_str().unwrap())),
            all_under(&second)
        );
    }

    #[test]
    fn the_app_that_claims_the_users_home_gets_dot_lattice() {
        let users = dirs::home_dir().unwrap().join(".lattice");
        assert_eq!(roots_of_child(None, CLAIMED), all_under(&users));
        let dir = tempfile::tempdir().unwrap();
        let second = dir.path().join("second");
        assert_eq!(
            roots_of_child(Some(second.to_str().unwrap()), CLAIMED),
            all_under(&second),
            "LATTICE_HOME still moves the app's roots"
        );
    }

    /// A test or an eval that sets no home of its own must not write into the
    /// user's library: on Windows `HOME` does not move it, so a run that
    /// pointed only `HOME` at a scratch folder imported into the real one.
    #[test]
    fn a_process_that_has_not_claimed_the_users_home_gets_a_scratch_home() {
        let users = dirs::home_dir().unwrap().join(".lattice");
        for configured in [None, Some("")] {
            let roots = roots_with(configured);
            assert_eq!(roots.len(), 6, "{configured:?}");
            for (name, path) in roots {
                assert!(
                    !path.starts_with(&users),
                    "{name} resolved to the user's library at {} with LATTICE_HOME {configured:?}",
                    path.display()
                );
            }
        }
    }

    #[test]
    fn both_roots_share_the_lattice_home() {
        let library = library_root().expect("home directory");
        let archive = web_archive_root().expect("home directory");
        assert_eq!(library.parent(), archive.parent());
        assert!(library.ends_with(".lattice/files"));
        assert!(archive.ends_with(".lattice/web-archive"));
    }

    #[test]
    fn resolve_vault_root_uses_configured_when_present() {
        let root = resolve_vault_root("/explicit/vault").unwrap();
        assert_eq!(root, PathBuf::from("/explicit/vault"));
    }

    #[test]
    fn resolve_vault_root_falls_back_to_default_on_empty_string() {
        let root = resolve_vault_root("");
        if let Some(p) = root {
            assert!(p.ends_with("Lattice"));
        }
    }

    #[test]
    fn resolve_vault_root_treats_whitespace_as_empty() {
        let root = resolve_vault_root("   ");
        if let Some(p) = root {
            assert!(p.ends_with("Lattice"));
        }
    }
}
