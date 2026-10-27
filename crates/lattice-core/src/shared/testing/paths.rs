//! Absolute filesystem paths for tests, spelled the way the host OS spells
//! them: `abs("lattice/docs")` is `/lattice/docs` on Unix and
//! `C:\lattice\docs` on Windows, where a POSIX root without a drive prefix is
//! not absolute. Tests then assert on absoluteness and containment rather than
//! one platform's spelling.

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
