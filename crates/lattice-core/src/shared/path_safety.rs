//! The one policy for paths and names that come from a caller.
//!
//! A path is unsafe when it holds a parent-directory component, never when a
//! name merely contains two dots (`v1..v2`, `notes..md`). Substring tests on
//! the path text refuse those names and still miss separator spellings, so
//! every check goes through the path's components instead.

use std::fmt;
use std::path::{Component, Path};

/// Why a path or name was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathUnsafe {
    /// A parent-directory component (`..`).
    ParentDir,
    /// A NUL byte.
    NulByte,
    /// Not exactly one plain name: empty, `.`, or holding a separator.
    NotSingleName,
    /// Not two plain names joined by one `/`.
    NotRepoId,
}

impl fmt::Display for PathUnsafe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ParentDir => "contains a parent-directory component",
            Self::NulByte => "contains a null byte",
            Self::NotSingleName => "is not a single plain name",
            Self::NotRepoId => "is not an organisation/name pair of plain names",
        })
    }
}

impl std::error::Error for PathUnsafe {}

/// A segment that steps to the parent directory. Windows strips trailing dots
/// and spaces from a segment, so there `.. ` steps up too; any name made only
/// of dots and spaces (other than `.`) is refused.
fn is_parent_segment(segment: &str) -> bool {
    segment == ".."
        || (cfg!(windows)
            && segment != "."
            && segment.contains('.')
            && segment.chars().all(|c| c == '.' || c == ' '))
}

/// Refuse a path with a parent-directory component or a NUL byte.
///
/// A backslash separates segments on every platform, so `a\..\b` is refused
/// on Unix too. Relative and absolute paths are both accepted.
pub fn reject_parent_components(path: &Path) -> Result<(), PathUnsafe> {
    if path.to_string_lossy().contains('\0') {
        return Err(PathUnsafe::NulByte);
    }
    for component in path.components() {
        match component {
            Component::ParentDir => return Err(PathUnsafe::ParentDir),
            Component::Normal(name)
                if name.to_string_lossy().split('\\').any(is_parent_segment) =>
            {
                return Err(PathUnsafe::ParentDir)
            }
            _ => {}
        }
    }
    Ok(())
}

/// Accept exactly one plain name: no separator, no `.` or `..`, no NUL.
pub fn single_file_name(name: &str) -> Result<(), PathUnsafe> {
    if name.contains('\0') {
        return Err(PathUnsafe::NulByte);
    }
    if is_parent_segment(name) {
        return Err(PathUnsafe::ParentDir);
    }
    if name.is_empty() || name.contains(['/', '\\']) {
        return Err(PathUnsafe::NotSingleName);
    }
    let mut components = Path::new(name).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(()),
        _ => Err(PathUnsafe::NotSingleName),
    }
}

/// Split a Hugging Face repository id `org/name` into its two halves, each
/// a plain name.
pub fn repo_id_parts(id: &str) -> Result<(&str, &str), PathUnsafe> {
    let (org, name) = id.split_once('/').ok_or(PathUnsafe::NotRepoId)?;
    if name.contains('/') {
        return Err(PathUnsafe::NotRepoId);
    }
    single_file_name(org)?;
    single_file_name(name)?;
    Ok((org, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_dots_inside_a_name_are_not_a_parent_component() {
        for path in [
            "v1..v2/notes..md",
            "/lib/v1..v2/notes..md",
            "a../..b",
            "..x",
        ] {
            assert_eq!(reject_parent_components(Path::new(path)), Ok(()), "{path}");
        }
    }

    #[test]
    fn a_parent_component_is_refused_wherever_it_sits() {
        for path in ["..", "../secret", "a/../../b", "/lib/..", "a/b/../c"] {
            assert_eq!(
                reject_parent_components(Path::new(path)),
                Err(PathUnsafe::ParentDir),
                "{path}"
            );
        }
    }

    #[test]
    fn a_backslash_separated_parent_component_is_refused_on_every_platform() {
        for path in ["a\\..\\b", "/var/log/..\\..\\etc", "..\\x"] {
            assert_eq!(
                reject_parent_components(Path::new(path)),
                Err(PathUnsafe::ParentDir),
                "{path}"
            );
        }
    }

    #[test]
    fn a_nul_byte_is_refused_in_a_path_and_in_a_name() {
        assert_eq!(
            reject_parent_components(Path::new("a\0b")),
            Err(PathUnsafe::NulByte)
        );
        assert_eq!(single_file_name("a\0b"), Err(PathUnsafe::NulByte));
    }

    #[cfg(windows)]
    #[test]
    fn a_windows_path_with_a_parent_component_is_refused() {
        for path in [r"C:\lib\..\x", r"a\..\b", r"..\x", r"C:\lib\...\x"] {
            assert_eq!(
                reject_parent_components(Path::new(path)),
                Err(PathUnsafe::ParentDir),
                "{path}"
            );
        }
        assert_eq!(
            reject_parent_components(Path::new(r"C:\lib\v1..v2\notes..md")),
            Ok(())
        );
    }

    #[test]
    fn a_plain_name_is_one_file_name_even_with_dots_inside() {
        for name in [
            "model.gguf",
            "model..Q4_K_M.gguf",
            "v1..v2",
            "notes..md",
            "a b",
            ".hidden",
            "x.",
        ] {
            assert_eq!(single_file_name(name), Ok(()), "{name}");
        }
    }

    #[test]
    fn anything_but_one_plain_name_is_refused() {
        for name in ["", ".", "/", "\\", "a/b", "a\\b", "/abs", "dir/", "./x"] {
            assert_eq!(
                single_file_name(name),
                Err(PathUnsafe::NotSingleName),
                "{name:?}"
            );
        }
        assert_eq!(single_file_name(".."), Err(PathUnsafe::ParentDir));
    }

    #[cfg(windows)]
    #[test]
    fn a_drive_relative_name_is_refused_on_windows() {
        for name in ["C:x", "C:"] {
            assert_eq!(
                single_file_name(name),
                Err(PathUnsafe::NotSingleName),
                "{name}"
            );
        }
    }

    #[test]
    fn a_repo_id_is_two_plain_names_joined_by_one_slash() {
        assert_eq!(repo_id_parts("org/name"), Ok(("org", "name")));
        assert_eq!(repo_id_parts("org/repo..v2"), Ok(("org", "repo..v2")));
        for id in [
            "", "name", "a/b/c", "/b", "a/", "../b", "a/..", "a\\b/c", "a/b\0",
        ] {
            assert!(repo_id_parts(id).is_err(), "{id:?}");
        }
    }
}
