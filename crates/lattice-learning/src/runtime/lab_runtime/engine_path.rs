//! Locating the Docker/Podman CLI when a desktop launch's PATH omits it.
use super::*;

#[cfg(windows)]
pub(super) fn engine_filename(engine: LearningContainerEngine) -> OsString {
    OsString::from(format!("{}.exe", engine.executable()))
}

#[cfg(not(windows))]
pub(super) fn engine_filename(engine: LearningContainerEngine) -> OsString {
    OsString::from(engine.executable())
}

#[cfg(windows)]
pub(super) fn engine_home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(PathBuf::from)
}

#[cfg(not(windows))]
pub(super) fn engine_home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[cfg(windows)]
pub(super) fn unresolved_engine_path(engine: LearningContainerEngine) -> PathBuf {
    PathBuf::from(format!(
        r"C:\__lattice_container_cli_unresolved__\{}.exe",
        engine.executable()
    ))
}

#[cfg(not(windows))]
pub(super) fn unresolved_engine_path(engine: LearningContainerEngine) -> PathBuf {
    PathBuf::from(format!(
        "/__lattice_container_cli_unresolved__/{}",
        engine.executable()
    ))
}

fn is_usable_engine_executable(candidate: &Path) -> bool {
    if !candidate.is_absolute() || !candidate.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        candidate
            .metadata()
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

pub(super) fn resolve_engine_executable(
    filename: &OsStr,
    inherited_path: Option<&OsStr>,
    known_locations: &[PathBuf],
) -> Option<PathBuf> {
    inherited_path
        .into_iter()
        .flat_map(std::env::split_paths)
        .map(|directory| directory.join(filename))
        .chain(known_locations.iter().cloned())
        .find(|candidate| is_usable_engine_executable(candidate))
}

pub(super) fn extended_engine_path(
    inherited_path: Option<&OsStr>,
    known_locations: &[PathBuf],
) -> OsString {
    let mut directories = Vec::new();
    if let Some(path) = inherited_path {
        for directory in std::env::split_paths(path) {
            if directory.is_absolute() && !directories.contains(&directory) {
                directories.push(directory);
            }
        }
    }
    for directory in known_locations
        .iter()
        .filter_map(|location| location.parent())
    {
        if directory.is_absolute() && !directories.iter().any(|item| item == directory) {
            directories.push(directory.to_owned());
        }
    }
    std::env::join_paths(directories).unwrap_or_default()
}

pub(super) fn known_engine_locations(
    engine: LearningContainerEngine,
    home: Option<&Path>,
) -> Vec<PathBuf> {
    let filename = engine_filename(engine);
    let mut locations = Vec::new();

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = home {
            locations.push(home.join(".docker/bin").join(&filename));
            let desktop_app = match engine {
                LearningContainerEngine::Docker => "Docker.app",
                LearningContainerEngine::Podman => "Podman Desktop.app",
            };
            locations.push(
                home.join("Applications")
                    .join(desktop_app)
                    .join("Contents/Resources/bin")
                    .join(&filename),
            );
        }
        let desktop_app = match engine {
            LearningContainerEngine::Docker => "Docker.app",
            LearningContainerEngine::Podman => "Podman Desktop.app",
        };
        locations.push(
            PathBuf::from("/Applications")
                .join(desktop_app)
                .join("Contents/Resources/bin")
                .join(&filename),
        );
        locations.push(PathBuf::from("/opt/homebrew/bin").join(&filename));
        locations.push(PathBuf::from("/usr/local/bin").join(&filename));
        if engine == LearningContainerEngine::Podman {
            locations.push(PathBuf::from("/opt/podman/bin").join(&filename));
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(home) = home {
            locations.push(home.join(".docker/bin").join(&filename));
            locations.push(home.join(".local/bin").join(&filename));
        }
        locations.push(PathBuf::from("/usr/local/bin").join(&filename));
        locations.push(PathBuf::from("/usr/bin").join(&filename));
        locations.push(PathBuf::from("/snap/bin").join(&filename));
    }

    #[cfg(windows)]
    {
        if let Some(home) = home {
            locations.push(home.join(".docker/bin").join(&filename));
            if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
                let app = match engine {
                    LearningContainerEngine::Docker => "Docker/Docker/resources/bin",
                    LearningContainerEngine::Podman => "Programs/RedHat/Podman",
                };
                locations.push(PathBuf::from(local_app_data).join(app).join(&filename));
            }
        }
        if let Some(program_files) = std::env::var_os("ProgramFiles") {
            let app = match engine {
                LearningContainerEngine::Docker => "Docker/Docker/resources/bin",
                LearningContainerEngine::Podman => "RedHat/Podman",
            };
            locations.push(PathBuf::from(program_files).join(app).join(&filename));
        }
    }

    locations
}
