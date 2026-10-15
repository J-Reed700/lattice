//! Discovering GGUF files in user-configured external directories and syncing
//! them into the downloaded-model registry.
use super::*;
use lattice_core::domain::models::downloaded::StoredDownloadedModel;

const EXTERNAL_MODEL_SOURCE: &str = "external_directory";
const MAX_EXTERNAL_MODEL_FILES: usize = 5000;
const MAX_EXTERNAL_SCAN_DEPTH: usize = 8;

#[derive(Debug, Clone)]
struct DiscoveredExternalModel {
    file_path: PathBuf,
    file_size_bytes: i64,
    source_directory: String,
}

fn normalize_external_directories(paths: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();

    for raw in paths {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }

        let candidate = PathBuf::from(trimmed);
        if !candidate.is_absolute() {
            continue;
        }

        let canonical = std::fs::canonicalize(&candidate).unwrap_or(candidate);
        let canonical_str = canonical.to_string_lossy().to_string();
        if seen.insert(canonical_str.clone()) {
            normalized.push(canonical_str);
        }
    }

    normalized
}

fn is_supported_external_model_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            let lower = ext.to_ascii_lowercase();
            lower == "gguf" || lower == "onnx"
        })
        .unwrap_or(false)
}

fn derive_external_model_id(path: &Path) -> String {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut hasher = Sha256::new();
    hasher.update(canonical.to_string_lossy().as_bytes());
    let digest = hex::encode(hasher.finalize());
    format!("external-{}", &digest[..24])
}

fn derive_external_model_name(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| stem.trim())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("External Model")
        .to_string()
}

fn derive_external_architecture(path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if ext == "onnx" {
        return "onnx".to_string();
    }
    if ext == "gguf" {
        return "gguf".to_string();
    }

    "unknown".to_string()
}

fn derive_external_model_type(path: &Path) -> ModelType {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "onnx" => ModelType::TextEmbeddings,
        _ => ModelType::LanguageModel,
    }
}

fn scan_directory_recursive(
    root_dir: &Path,
    current: &Path,
    depth: usize,
    discovered: &mut Vec<DiscoveredExternalModel>,
    seen_files: &mut HashSet<String>,
) {
    if depth > MAX_EXTERNAL_SCAN_DEPTH || discovered.len() >= MAX_EXTERNAL_MODEL_FILES {
        return;
    }

    // repository-barrier-allow: external-model discovery walks user-configured model resources.
    let Ok(entries) = std::fs::read_dir(current) else {
        return;
    };

    for entry in entries.flatten() {
        if discovered.len() >= MAX_EXTERNAL_MODEL_FILES {
            return;
        }

        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }

        let path = entry.path();
        // repository-barrier-allow: classify entries in the external-model resource tree.
        if file_type.is_dir() {
            scan_directory_recursive(root_dir, &path, depth + 1, discovered, seen_files);
            continue;
        }

        // repository-barrier-allow: only regular model artifact files are importable.
        if !file_type.is_file() || !is_supported_external_model_file(&path) {
            continue;
        }

        // repository-barrier-allow: record size from the discovered model artifact itself.
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        if metadata.len() == 0 {
            continue;
        }

        let canonical = std::fs::canonicalize(&path).unwrap_or(path);
        let canonical_str = canonical.to_string_lossy().to_string();
        if !seen_files.insert(canonical_str.clone()) {
            continue;
        }

        discovered.push(DiscoveredExternalModel {
            file_path: PathBuf::from(canonical_str),
            file_size_bytes: metadata.len() as i64,
            source_directory: root_dir.to_string_lossy().to_string(),
        });
    }
}

fn scan_external_models_sync(directories: &[String]) -> Vec<DiscoveredExternalModel> {
    let mut discovered = Vec::new();
    let mut seen_files = HashSet::new();

    for dir in directories {
        if discovered.len() >= MAX_EXTERNAL_MODEL_FILES {
            break;
        }

        let root = PathBuf::from(dir);
        // repository-barrier-allow: validate each user-configured external-model directory resource.
        if !root.is_dir() {
            continue;
        }

        scan_directory_recursive(&root, &root, 0, &mut discovered, &mut seen_files);
    }

    discovered
}

fn is_external_directory_model(model: &DownloadedModel) -> bool {
    let Some(metadata) = model.metadata() else {
        return false;
    };

    let source_matches = metadata
        .get("source")
        .and_then(|v| v.as_str())
        .map(|v| v == EXTERNAL_MODEL_SOURCE)
        .unwrap_or(false);
    let external_flag = metadata
        .get("external")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    source_matches || external_flag
}

pub(super) async fn sync_external_model_directories(
    container: &Container,
    repository: &DownloadedModelRepository,
) -> Result<(), AppError> {
    let settings = container
        .get_settings_use_case()
        .execute()
        .await
        .map_err(|e| {
            AppError::InvalidConfig(format!(
                "Failed to load settings for external model sync: {}",
                e
            ))
        })?;

    let directories = normalize_external_directories(&settings.llm.external_model_directories);
    let directories_for_scan = directories.clone();
    let discovered = task::spawn_blocking(move || scan_external_models_sync(&directories_for_scan))
        .await
        .map_err(|e| AppError::InternalError(format!("External model scan task failed: {}", e)))?;

    let mut discovered_model_ids: HashSet<String> = HashSet::new();

    for candidate in discovered {
        let model_id = derive_external_model_id(&candidate.file_path);
        discovered_model_ids.insert(model_id.clone());

        let already_exists = repository
            .find_by_model_id(&model_id)
            .await
            .map_err(|e| e.with_context("Failed checking external model existence"))?
            .is_some();
        if already_exists {
            continue;
        }

        let model_name = derive_external_model_name(&candidate.file_path);
        let architecture = derive_external_architecture(&candidate.file_path);
        let model_type = derive_external_model_type(&candidate.file_path);
        let metadata = serde_json::json!({
            "source": EXTERNAL_MODEL_SOURCE,
            "external": true,
            "source_directory": candidate.source_directory,
            "path": candidate.file_path.to_string_lossy().to_string(),
        });

        // External imports always come in as a single weight file —
        // we don't recursively scan into config.json/tokenizer.json
        // siblings here, so this is always a LocalFile artifact.
        let external_model = DownloadedModel::from_db(StoredDownloadedModel {
            id: uuid::Uuid::new_v4().to_string(),
            model_name,
            model_id: model_id.clone(),
            location: ModelLocation::LocalFile {
                path: candidate.file_path,
            },
            file_size_bytes: candidate.file_size_bytes,
            model_type,
            architecture,
            downloaded_at: Utc::now(),
            last_used_at: None,
            use_count: 0,
            is_active_for_chat: false,
            is_active_for_embedding: false,
            metadata: Some(metadata),
            is_active_for_utility: false,
            embedding_artifact_identity: None,
        });

        if let Err(e) = repository.save(&external_model).await {
            warn!(model_id = %model_id, error = %e, "Failed to save discovered external model");
        }
    }

    let existing_models = repository
        .list_all()
        .await
        .map_err(|e| e.with_context("Failed to list models for external sync cleanup"))?;

    for model in existing_models {
        if !is_external_directory_model(&model) {
            continue;
        }

        let still_discovered = discovered_model_ids.contains(model.model_id());
        // External rows are always LocalFile (see external_model
        // construction above). loadable_path() returns the file path;
        // we re-check existence on disk to detect manual deletion.
        // repository-barrier-allow: external model rows intentionally reconcile against user-owned artifacts.
        let file_exists = model
            .loadable_path()
            // repository-barrier-allow: validate the external model artifact itself.
            .map(|p| p.exists() && p.is_file())
            .unwrap_or(false);
        if still_discovered && file_exists {
            continue;
        }

        match repository.delete_if_not_active(model.id()).await {
            Ok(Some(_)) => {
                info!(
                    model_id = %model.model_id(),
                    "Removed stale external model record"
                );
            }
            Ok(None) => {
                debug!(
                    model_id = %model.model_id(),
                    "External model record already absent during cleanup"
                );
            }
            Err(AppError::InvalidInput(_)) => {
                warn!(
                    model_id = %model.model_id(),
                    "Skipped stale external model cleanup because model is active"
                );
            }
            Err(e) => {
                warn!(
                    model_id = %model.model_id(),
                    error = %e,
                    "Failed to cleanup stale external model record"
                );
            }
        }
    }

    Ok(())
}
