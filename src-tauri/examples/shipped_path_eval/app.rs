//! A desktop app with no window: the composition root the desktop builds, a
//! fresh data directory, the embedding model activated the way Settings
//! activates it, and the corpus imported through the import use case.

use crate::Res;
use lattice::application::contracts::settings::SettingsDto;
use lattice::application::ports::SettingsRepositoryPort;
use lattice::domain::models::downloaded::{DownloadedModel, ModelLocation};
use lattice::features::indexing::dto::{ChunkingStrategyDto, IndexFileRequestDto};
use lattice::features::model_management::commands_extra::{
    set_active_embedding_model_impl, set_active_utility_model_impl,
};
use lattice::features::settings::repository::SettingsRepository;
use lattice::infrastructure::setup::database::setup_database;
use lattice::Container;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

#[derive(Deserialize)]
pub struct CorpusDocument {
    pub id: String,
    pub title: Option<String>,
    #[serde(default)]
    pub collection: Option<String>,
    pub text: String,
}

#[derive(Deserialize)]
struct Corpus {
    documents: Vec<CorpusDocument>,
}

/// Writes the settings file, then builds the container on a new database,
/// in the order the desktop's startup does.
pub async fn open(data_dir: &Path, settings: &SettingsDto) -> Res<Container> {
    SettingsRepository::new(data_dir.to_path_buf())
        .await?
        .save_all(settings)
        .await?;
    let connection = setup_database(data_dir.join("lattice.db")).await?;
    let pool = connection.pool().clone();
    let container = Container::new(
        pool,
        Arc::new(connection),
        None,
        &settings.llm.ollama_url,
        &settings.llm.model,
        data_dir.to_path_buf(),
        Arc::default(),
    )
    .await?;
    Ok(container)
}

/// Registers the embedding directory as a downloaded model and activates it
/// through the same command path Settings uses, which establishes its
/// artifact identity and prepares its vector generation.
pub async fn activate_embedding(container: &Container, dir: &Path, model_id: &str) -> Res<Value> {
    let started = Instant::now();
    let size = std::fs::read_dir(dir)?
        .filter_map(|entry| entry.ok()?.metadata().ok())
        .map(|metadata| metadata.len())
        .sum::<u64>();
    let model = DownloadedModel::new(
        uuid::Uuid::new_v4().to_string(),
        model_id.to_owned(),
        model_id.to_owned(),
        ModelLocation::LocalDirectory {
            path: dir.to_path_buf(),
        },
        i64::try_from(size)?,
        "qwen3".to_owned(),
        None,
    )?;
    container.downloaded_model_repository().save(&model).await?;
    set_active_embedding_model_impl(container, model_id).await?;
    let embedding = container.get_or_load_embedding().await?;
    Ok(json!({
        "model_id": model_id,
        "model_identity": embedding.model_identity(),
        "dimension": embedding.dimension(),
        "activation_ms": started.elapsed().as_millis(),
    }))
}

/// Records a server-hosted model and gives it the utility role through the
/// Settings command, so intent, planning, HyDE, the corrective retry and the
/// grounding judge resolve it exactly as they do on a desktop with that role set.
pub async fn assign_utility(container: &Container, model_id: &str, served_as: &str) -> Res<Value> {
    let model = DownloadedModel::new(
        uuid::Uuid::new_v4().to_string(),
        served_as.to_owned(),
        model_id.to_owned(),
        ModelLocation::RemoteOllama,
        0,
        "qwen35".to_owned(),
        None,
    )?;
    container.downloaded_model_repository().save(&model).await?;
    set_active_utility_model_impl(container, model_id).await?;
    let loaded = container.get_or_load_utility_llm().await?;
    let name = loaded.as_ref().map(|llm| llm.model_name().to_owned());
    if name.as_deref() != Some(served_as) {
        return Err(format!("utility role resolved to {name:?}, not {served_as}").into());
    }
    Ok(json!({"model_id": model_id, "served_as": served_as}))
}

/// Imports every corpus document as a Markdown file named after its title,
/// the way a user's vault holds notes, and returns the map from Lattice's
/// document ids back to the dataset's.
///
/// Titles rather than dataset ids become file names: an id like
/// `runbook-core-00` would hand the lexical branch a hint no user's vault
/// carries.
pub async fn import_corpus(
    container: &Container,
    corpus_path: &Path,
    staging: &Path,
    chunk_tokens: usize,
) -> Res<(HashMap<String, String>, Value)> {
    let corpus: Corpus = serde_json::from_str(&std::fs::read_to_string(corpus_path)?)?;
    let started = Instant::now();
    let mut taken = HashSet::new();
    let mut to_dataset = HashMap::new();
    let mut duplicates = Vec::new();
    let mut chunks = 0usize;
    let mut failures = BTreeMap::new();
    let use_case = container.index_file_use_case();
    for (position, document) in corpus.documents.iter().enumerate() {
        let folder = staging.join(document.collection.as_deref().unwrap_or("notes"));
        std::fs::create_dir_all(&folder)?;
        let stem = file_stem(document.title.as_deref().unwrap_or(&document.id));
        let mut name = format!("{stem}.md");
        let mut copy = 2;
        while !taken.insert(folder.join(&name)) {
            name = format!("{stem} ({copy}).md");
            copy += 1;
        }
        let path = folder.join(&name);
        std::fs::write(&path, &document.text)?;
        let request = IndexFileRequestDto {
            path: path.to_string_lossy().into_owned(),
            chunking_strategy: ChunkingStrategyDto::Semantic {
                max_tokens: chunk_tokens,
            },
            tags: None,
            metadata: None,
            space_id: None,
        };
        match use_case.execute(request).await {
            Ok(response) => {
                if response.status == "already_indexed" {
                    duplicates.push(document.id.clone());
                }
                chunks += response.chunks_created;
                to_dataset.insert(response.document_id, document.id.clone());
            }
            Err(error) => {
                failures.insert(document.id.clone(), error.to_string());
            }
        }
        if (position + 1) % 50 == 0 {
            eprintln!(
                "indexed {}/{} documents ({:.0} s)",
                position + 1,
                corpus.documents.len(),
                started.elapsed().as_secs_f64()
            );
        }
    }
    if !failures.is_empty() {
        return Err(format!("{} documents failed to index: {failures:?}", failures.len()).into());
    }
    let report = json!({
        "documents": corpus.documents.len(),
        "lattice_documents": to_dataset.len(),
        "chunks": chunks,
        "duplicates": duplicates,
        "chunk_tokens": chunk_tokens,
        "index_ms": started.elapsed().as_millis(),
    });
    Ok((to_dataset, report))
}

/// A file name a title can safely become.
fn file_stem(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.');
    let stem: String = trimmed.chars().take(120).collect();
    if stem.is_empty() {
        "untitled".to_owned()
    } else {
        stem
    }
}
