//! Absolute filesystem paths for tests, spelled the way the host OS spells them.
//!
//! A great many tests here used to hard-code POSIX roots — `/lattice`,
//! `/Users/example/Documents`, `/etc/passwd`. That reads as "any absolute
//! path", but on Windows it is not one: `Path::new("/lattice").is_absolute()`
//! is `false` there, because Windows requires a prefix (`C:`) as well as a
//! root. Every validator, confinement check and canonicalisation in this crate
//! therefore behaved differently under such a path, and 22 unit tests failed
//! the first time the suite ever ran on Windows.
//!
//! The fix is to stop writing a separator-and-prefix convention into test data
//! by hand. `abs("lattice/docs")` yields `/lattice/docs` on Unix and
//! `C:\lattice\docs` on Windows, so a test asserts on *absoluteness* and
//! *containment* rather than on one platform's spelling of them.

use std::path::{Path, PathBuf};

/// The root every [`abs`] path hangs off: `/` on Unix, `C:\` on Windows.
///
/// `C:` is what GitHub's Windows runners and essentially every Windows
/// install use for the system volume. Tests only need a root that is real
/// enough to be *absolute*; none of them require it to exist on disk.
pub fn root() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from("C:\\")
    } else {
        PathBuf::from("/")
    }
}

/// Builds an absolute host-native path from `/`-separated components.
///
/// `relative` must not itself begin with a separator; leading separators are
/// stripped so `abs("/lattice")` and `abs("lattice")` agree.
pub fn abs(relative: &str) -> PathBuf {
    let mut path = root();
    for component in relative.split('/').filter(|c| !c.is_empty()) {
        path.push(component);
    }
    path
}

/// [`abs`], as the string a DTO or an error message would carry.
pub fn abs_str(relative: &str) -> String {
    abs(relative).to_string_lossy().into_owned()
}

/// Joins `/`-separated components onto an existing base, host-natively.
///
/// Use this with a [`tempfile::TempDir`] when the test needs the path to
/// really exist; use [`abs`] when it only needs to be well-formed.
pub fn join(base: impl AsRef<Path>, relative: &str) -> PathBuf {
    let mut path = base.as_ref().to_path_buf();
    for component in relative.split('/').filter(|c| !c.is_empty()) {
        path.push(component);
    }
    path
}

/// An absolute path that is guaranteed *not* to sit under [`root`].
///
/// Tests that prove a confinement check rejects an outside path need a second
/// root, not merely a different directory: on Windows the only way to be
/// certainly outside `C:\…` is to be on another volume.
pub fn abs_on_other_volume(relative: &str) -> PathBuf {
    if cfg!(windows) {
        let mut path = PathBuf::from("Z:\\");
        for component in relative.split('/').filter(|c| !c.is_empty()) {
            path.push(component);
        }
        path
    } else {
        // On Unix there is one root, so "outside" is a sibling subtree.
        abs(&format!("elsewhere/{}", relative.trim_start_matches('/')))
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod tests {
    use super::*;

    #[test]
    fn built_paths_are_absolute_on_this_platform() {
        assert!(abs("lattice/docs/file.txt").is_absolute());
        assert!(root().is_absolute());
        assert!(abs_on_other_volume("etc/passwd").is_absolute());
    }

    #[test]
    fn a_leading_separator_is_not_doubled() {
        assert_eq!(abs("/lattice"), abs("lattice"));
    }

    #[test]
    fn components_are_joined_with_the_host_separator() {
        let path = abs("lattice/docs");
        assert!(path.ends_with(Path::new("lattice").join("docs")));
        assert_eq!(
            abs_str("lattice/docs"),
            path.to_string_lossy().into_owned(),
            "abs_str must agree with abs"
        );
    }

    #[test]
    fn the_other_volume_is_outside_the_main_root() {
        let outside = abs_on_other_volume("secrets/passwd");
        assert!(!outside.starts_with(abs("lattice")));
    }

    #[test]
    fn join_extends_an_existing_base() {
        let base = abs("lattice");
        assert_eq!(join(&base, "docs/file.txt"), abs("lattice/docs/file.txt"));
    }
}
