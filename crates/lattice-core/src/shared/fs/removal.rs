//! Removal that outlasts Windows' brief hold on files just closed.
//!
//! Windows refuses to delete a file something still has open, and something
//! often does for a moment after the app closes it: an antivirus scan of a
//! file just written, or a handle the system has not finished releasing.
//! Unix deletes such a file at once. These helpers try again on those
//! refusals only, waiting longer each time, and report any other error, or a
//! refusal that outlasts the waits, as it is.

use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Tries before a refusal is reported. With the doubling wait from
/// [`FIRST_WAIT`] the waits add up to about 1.3 seconds.
const ATTEMPTS: u32 = 8;
const FIRST_WAIT: Duration = Duration::from_millis(10);

/// Tries for a directory of files just written, which a scan of new files can
/// hold for seconds: the waits add up to about ten seconds.
const WRITTEN_ATTEMPTS: u32 = 11;

/// [`std::fs::remove_file`], tried again while Windows holds the file.
pub fn remove_file(path: &Path) -> io::Result<()> {
    patiently(ATTEMPTS, || std::fs::remove_file(path))
}

/// [`std::fs::remove_dir_all`], tried again while Windows holds a file in it.
pub fn remove_dir_all(dir: &Path) -> io::Result<()> {
    patiently(ATTEMPTS, || std::fs::remove_dir_all(dir))
}

/// [`remove_dir_all`] for a directory whose files were just written and
/// closed, as an index's are: tried for longer, on a blocking thread so the
/// waits hold up no other work.
pub async fn remove_written_dir(dir: PathBuf) -> io::Result<()> {
    tokio::task::spawn_blocking(move || {
        patiently(WRITTEN_ATTEMPTS, || std::fs::remove_dir_all(&dir))
    })
    .await
    .unwrap_or_else(|error| Err(io::Error::other(error)))
}

fn patiently(attempts: u32, mut remove: impl FnMut() -> io::Result<()>) -> io::Result<()> {
    let mut wait = FIRST_WAIT;
    for _ in 1..attempts {
        match remove() {
            Err(error) if held(&error) => {
                std::thread::sleep(wait);
                wait *= 2;
            }
            done => return done,
        }
    }
    remove()
}

/// Whether `error` is Windows refusing a file something still holds: access
/// denied (5), a sharing or lock violation (32, 33), or a directory not yet
/// empty because a file in it is still being deleted (145).
fn held(error: &io::Error) -> bool {
    cfg!(windows) && matches!(error.raw_os_error(), Some(5 | 32 | 33 | 145))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_a_file_and_a_directory_and_reports_one_that_is_missing() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("index");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("index.sqlite");
        std::fs::write(&file, b"rows").unwrap();

        remove_file(&file).unwrap();
        assert!(!file.exists());
        remove_dir_all(&dir).unwrap();
        assert!(!dir.exists());
        assert_eq!(
            remove_dir_all(&dir).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }

    #[test]
    fn only_a_held_file_is_tried_again() {
        let mut tries = 0;
        let error = patiently(ATTEMPTS, || {
            tries += 1;
            Err(io::Error::from(io::ErrorKind::NotFound))
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert_eq!(tries, 1);

        let mut tries = 0;
        let outcome = patiently(ATTEMPTS, || {
            tries += 1;
            if tries < 3 {
                Err(io::Error::from_raw_os_error(32))
            } else {
                Ok(())
            }
        });
        if cfg!(windows) {
            assert!(outcome.is_ok());
            assert_eq!(tries, 3);
        } else {
            assert_eq!(tries, 1, "Unix never holds a file against deletion");
        }
    }
}
