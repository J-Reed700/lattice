//! Validating lab specs, files and paths before anything touches the host.
use super::*;

fn validate_image_id(value: &str) -> Result<()> {
    let digest = value.strip_prefix("sha256:").ok_or_else(|| {
        AppError::InvalidInput("A lab runtime must use a locally resolved sha256 image ID.".into())
    })?;
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(AppError::InvalidInput(
            "A lab runtime image ID must contain a 64-character SHA-256 digest.".into(),
        ));
    }
    Ok(())
}

/// Return a normalized relative path containing only ordinary components.
/// Windows prefixes, roots, `.` and `..` are rejected on every host so a pack
/// cannot become dangerous merely by being imported on another OS.
pub fn validate_lab_path(value: &str) -> Result<PathBuf> {
    if value.is_empty() || value.chars().count() > 240 || value.contains('\0') {
        return Err(AppError::InvalidInput(
            "Lab file paths must contain 1–240 safe characters.".into(),
        ));
    }
    if value.contains('\\')
        || value.contains("//")
        || value.starts_with('/')
        || value.contains(':')
        || value.contains(',')
    {
        return Err(AppError::InvalidInput(
            "Lab file paths must be portable relative paths.".into(),
        ));
    }
    if value.split('/').next().is_some_and(|part| {
        [".lattice-build", "node_modules"]
            .iter()
            .any(|reserved| part.eq_ignore_ascii_case(reserved))
    }) {
        return Err(AppError::InvalidInput(
            "The .lattice-build and node_modules directories are reserved for the runtime.".into(),
        ));
    }
    let path = Path::new(value);
    let mut normal = 0;
    for component in path.components() {
        match component {
            Component::Normal(part) if part != OsStr::new("") => normal += 1,
            _ => {
                return Err(AppError::InvalidInput(
                    "Lab file paths cannot escape the run workspace.".into(),
                ))
            }
        }
    }
    if normal == 0 {
        return Err(AppError::InvalidInput(
            "Lab file paths must name a file.".into(),
        ));
    }
    Ok(path.to_path_buf())
}

pub fn validate_execution_spec(spec: &LearningLabExecutionSpec) -> Result<()> {
    uuid(&spec.run_id, "lab run")?;
    validate_image_id(&spec.image_id)?;
    spec.limits.validate()?;
    if spec.command.is_empty() || spec.command.len() > MAX_ARGS {
        return Err(AppError::InvalidInput(format!(
            "A lab command must contain 1–{MAX_ARGS} arguments."
        )));
    }
    for argument in &spec.command {
        if argument.is_empty()
            || argument.contains('\0')
            || argument.chars().count() > MAX_ARG_CHARS
        {
            return Err(AppError::InvalidInput(
                "A lab command contains an invalid or oversized argument.".into(),
            ));
        }
    }
    validate_lab_files(
        &spec.run_id,
        &spec.files,
        &spec.read_only_paths,
        &spec.limits,
    )
}

/// Shared input boundary for embedded and container providers.
pub fn validate_lab_files(
    run_id: &str,
    files: &[LearningLabFile],
    read_only_paths: &[String],
    limits: &LearningLabLimits,
) -> Result<()> {
    uuid(run_id, "lab run")?;
    limits.validate()?;
    if files.is_empty() || files.len() > MAX_FILES {
        return Err(AppError::InvalidInput(format!(
            "A lab run must contain 1–{MAX_FILES} files."
        )));
    }
    let mut seen = HashSet::new();
    let mut total = 0usize;
    for file in files {
        let path = validate_lab_path(&file.path)?;
        let size = file.content.len();
        if size > MAX_FILE_BYTES {
            return Err(AppError::InvalidInput(format!(
                "Lab file '{}' exceeds 256 KiB.",
                file.path
            )));
        }
        total = total.saturating_add(size);
        if total > MAX_TOTAL_BYTES {
            return Err(AppError::InvalidInput(
                "Lab files exceed the 2 MiB run limit.".into(),
            ));
        }
        if !seen.insert(path) {
            return Err(AppError::InvalidInput(
                "A lab run cannot contain duplicate file paths.".into(),
            ));
        }
    }
    if read_only_paths.len() > files.len() {
        return Err(AppError::InvalidInput(
            "A lab run has more read-only paths than files.".into(),
        ));
    }
    let mut read_only = HashSet::new();
    for value in read_only_paths {
        let path = validate_lab_path(value)?;
        if !read_only.insert(path.clone()) {
            return Err(AppError::InvalidInput(
                "A lab run cannot repeat a read-only path.".into(),
            ));
        }
        if !seen.contains(&path) {
            return Err(AppError::InvalidInput(
                "Every read-only lab path must reference a materialized file.".into(),
            ));
        }
    }
    Ok(())
}
