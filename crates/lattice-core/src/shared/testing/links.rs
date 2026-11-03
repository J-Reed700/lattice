//! Links for the tests that prove a link cannot be used to leave a folder, or to loop.
//!
//! Windows refuses a symbolic link without `SeCreateSymbolicLinkPrivilege` (an elevated shell
//! or Developer Mode), so a test that cannot get one says so and returns. A directory junction
//! needs no privilege and is the way out of a folder any Windows user has, so a folder-link
//! test runs for a junction as well and never skips it.

use std::fmt;
use std::io;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::symlink as symlink_dir;
#[cfg(unix)]
use std::os::unix::fs::symlink as symlink_file;
#[cfg(windows)]
use std::os::windows::fs::{symlink_dir, symlink_file};

/// `ERROR_PRIVILEGE_NOT_HELD`, how Windows refuses a symbolic link.
#[cfg(windows)]
const PRIVILEGE_NOT_HELD: i32 = 1314;

/// What a folder link is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    Symlink,
    /// Windows only, and only for folders.
    #[cfg(windows)]
    Junction,
}

impl fmt::Display for LinkKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Symlink => "symbolic link",
            #[cfg(windows)]
            Self::Junction => "junction",
        })
    }
}

/// Makes `link` a `kind` link to the folder `target`.
///
/// `Ok(false)`, after printing why, when Windows lacks the privilege for a symbolic link: the
/// caller returns. Every other failure is an `Err`.
pub fn link_dir(kind: LinkKind, target: &Path, link: &Path) -> io::Result<bool> {
    match kind {
        LinkKind::Symlink => made(symlink_dir(target, link), target, link),
        #[cfg(windows)]
        LinkKind::Junction => junction(target, link).map(|()| true),
    }
}

/// Makes `link` a symbolic link to the file `target`; a junction cannot point at a file.
/// Returns what [`link_dir`] does.
pub fn link_file(target: &Path, link: &Path) -> io::Result<bool> {
    made(symlink_file(target, link), target, link)
}

fn made(result: io::Result<()>, target: &Path, link: &Path) -> io::Result<bool> {
    match result {
        Ok(()) => Ok(true),
        #[cfg(windows)]
        Err(error) if error.raw_os_error() == Some(PRIVILEGE_NOT_HELD) => {
            eprintln!(
                "skipping: no symbolic link at {}: Windows needs SeCreateSymbolicLinkPrivilege \
                 (an elevated shell or Developer Mode) to make one (os error {PRIVILEGE_NOT_HELD})",
                link.display()
            );
            Ok(false)
        }
        Err(error) => Err(io::Error::new(
            error.kind(),
            format!(
                "cannot link {} to {}: {error}",
                link.display(),
                target.display()
            ),
        )),
    }
}

#[cfg(windows)]
fn junction(target: &Path, link: &Path) -> io::Result<()> {
    let output = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()?;
    if output.status.success() {
        return Ok(());
    }
    Err(io::Error::other(format!(
        "mklink /J {} {} failed ({}): {}{}",
        link.display(),
        target.display(),
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    )))
}
