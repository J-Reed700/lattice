//! Tauri commands for corpus-shape clustering.
//!
//! Three commands exist today:
//!
//! - `cluster_vault_debug`  — runs the pipeline end-to-end and writes a
//!   human-readable JSON report into the user's Documents folder (or a
//!   fallback). Returns the file path. This is the 5.3 milestone — Josh
//!   eyeballs the output before any UI work.
//! - `cluster_vault_run`    — runs + persists. Returns a summary DTO.
//!   Foundation for Phase 5.4.
//! - `list_clusters`        — returns the most recent run's cluster list.
//!   Cheap read-only surface that Phase 5.4 will lean on.
//!
//! All three are explicit / user-triggered. Nothing here runs on ingest or
//! a timer.

use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::application::ports::{ChunkRepositoryPort, LLMPort};
use crate::features::corpus_shape::entity::{Cluster, ClusterRun, LabelSource};
use crate::features::corpus_shape::labeling::{
    RepresentativeDoc, REP_CONTENT_PREVIEW_CHARS,
};
use crate::features::corpus_shape::repository::{
    ClusterRepositoryPort, SqliteClusterRepository,
};
use crate::features::corpus_shape::use_cases::{
    run_clustering::DocumentContentPreviewPort, ProgressSink, RunClusteringOutcome,
    RunClusteringUseCase,
};
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};

// ============================================================================
// DTOs — what crosses the Tauri boundary.
// ============================================================================

/// Summary of a cluster run for UI consumption.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClusterRunDto {
    pub run_id: String,
    pub ran_at: String,
    pub doc_count: i64,
    pub cluster_count: i64,
    pub noise_count: i64,
    pub duration_ms: i64,
    pub llm_calls: i64,
}

/// Summary of a cluster for list views. Centroid + full member list are
/// intentionally omitted — those are internal.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClusterDto {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
    pub member_count: i64,
    pub sample_titles: Vec<String>,
    pub label_source: String,
    pub inherited_from_cluster_id: Option<String>,
    /// Member ids, so the Library can scope its list to a theme without a
    /// second round trip.
    pub member_document_ids: Vec<String>,
}

// ============================================================================
// Debug report JSON structure. Lives here because it's user-visible —
// changes here need to be intentional.
// ============================================================================

#[derive(Debug, Clone, Serialize)]
struct DebugReport {
    schema_version: &'static str,
    run: DebugRunBlock,
    parameters: DebugParamsBlock,
    cluster_count: usize,
    clusters: Vec<DebugClusterBlock>,
    noise: DebugNoiseBlock,
    notes: &'static [&'static str],
}

#[derive(Debug, Clone, Serialize)]
struct DebugRunBlock {
    run_id: String,
    ran_at: String,
    duration_ms: i64,
    total_docs: i64,
    llm_calls: i64,
}

#[derive(Debug, Clone, Serialize)]
struct DebugParamsBlock {
    min_cluster_size: usize,
    min_samples: usize,
    jaccard_inheritance_threshold: f32,
}

#[derive(Debug, Clone, Serialize)]
struct DebugClusterBlock {
    label: String,
    description: Option<String>,
    label_source: String,
    inherited_from_cluster_id: Option<String>,
    member_count: usize,
    sample_titles: Vec<String>,
    member_doc_ids: Vec<String>,
    fingerprint: String,
}

#[derive(Debug, Clone, Serialize)]
struct DebugNoiseBlock {
    count: usize,
    sample_doc_ids: Vec<String>,
}

// ============================================================================
// Command 1 — `cluster_vault_debug`
// ============================================================================

#[tauri::command]
#[specta::specta]
pub async fn cluster_vault_debug(container: State<'_, Container>) -> Result<String> {
    let use_case = build_use_case(&container).await?;
    let outcome = use_case.execute().await?;
    let path = write_debug_report(&container, &outcome).await?;
    Ok(path.display().to_string())
}

// ============================================================================
// Command 2 — `cluster_vault_run`
// ============================================================================

#[tauri::command]
#[specta::specta]
pub async fn cluster_vault_run<R: tauri::Runtime>(
    container: State<'_, Container>,
    window: tauri::Window<R>,
) -> Result<ClusterRunDto> {
    use tauri::Emitter;

    let sink: ProgressSink = Arc::new(move |progress| {
        // Progress is decoration; a dropped event never fails a run.
        let _ = window.emit("corpus-shape://progress", &progress);
    });
    let use_case = build_use_case(&container).await?.with_progress(sink);
    let outcome = use_case.execute().await?;
    Ok(run_to_dto(&outcome.run))
}

// ============================================================================
// Command 3 — `list_clusters`
// ============================================================================

#[tauri::command]
#[specta::specta]
pub async fn list_clusters(container: State<'_, Container>) -> Result<Vec<ClusterDto>> {
    let pool = container.db_pool().clone();
    let repo = SqliteClusterRepository::new(pool);
    let run = repo.get_latest_run().await?;
    let Some(run) = run else {
        return Ok(Vec::new());
    };
    let clusters = repo.get_clusters_for_run(&run.id).await?;

    let chunk_repo = container.chunk_repository();
    let document_repo = container.document_repository();

    let mut dtos = Vec::with_capacity(clusters.len());
    for cluster in clusters {
        let sample_titles = gather_sample_titles(
            &chunk_repo,
            &document_repo,
            &cluster,
        )
        .await;
        dtos.push(cluster_to_dto(&cluster, sample_titles));
    }
    Ok(dtos)
}

// ============================================================================
// Internal helpers
// ============================================================================

async fn build_use_case(container: &Container) -> Result<RunClusteringUseCase> {
    let pool = container.db_pool().clone();
    let cluster_repo: Arc<dyn ClusterRepositoryPort> =
        Arc::new(SqliteClusterRepository::new(pool));

    // Build the content-preview adapter on top of chunk + document repos.
    let content_preview: Arc<dyn DocumentContentPreviewPort> = Arc::new(
        ChunkBackedContentPreview::new(
            container.chunk_repository(),
            container.document_repository(),
        ),
    );

    // LLM is best-effort — if loading fails (e.g. no utility model set),
    // the pipeline still runs with fallback labels.
    let llm: Option<Arc<dyn LLMPort>> = match container.get_or_load_utility_llm().await {
        // `get_or_load_utility_llm` already returns `Option` — no utility model
        // configured is a normal state, not an error.
        Ok(llm) => llm,
        Err(err) => {
            tracing::warn!(
                error = %err,
                "corpus_shape: no utility LLM available; labels will use deterministic fallback"
            );
            None
        }
    };

    // Embedding repository is constructed directly from the pool — the
    // Container doesn't expose a port-shaped accessor for it today and the
    // underlying SQLite impl is stateless.
    let embedding_repo: Arc<dyn crate::application::ports::EmbeddingRepositoryPort> = Arc::new(
        crate::infrastructure::persistence::repositories::EmbeddingRepository::new(
            container.db_pool().clone(),
        ),
    );

    // Container exposes the trait-object form as `Arc<dyn DocumentRepository>`
    // (= RepositoryPort<Document> + DocumentRepositoryPort). Trait upcasting
    // (stable since 1.76) narrows it to the supertrait the use case wants —
    // `DocumentRepositoryPort`, for `find_all_paginated`.
    let document_repo_generic: Arc<dyn crate::application::ports::DocumentRepositoryPort> =
        container.document_repository();

    Ok(RunClusteringUseCase::new(
        document_repo_generic,
        embedding_repo,
        cluster_repo,
        content_preview,
        llm,
    ))
}

async fn write_debug_report(
    container: &Container,
    outcome: &RunClusteringOutcome,
) -> Result<PathBuf> {
    let chunk_repo = container.chunk_repository();
    let document_repo = container.document_repository();

    let mut cluster_blocks = Vec::with_capacity(outcome.clusters.len());
    for cluster in &outcome.clusters {
        let sample_titles = gather_sample_titles(&chunk_repo, &document_repo, cluster).await;
        cluster_blocks.push(DebugClusterBlock {
            label: cluster.label.clone(),
            description: cluster.description.clone(),
            label_source: cluster.label_source.as_str().to_string(),
            inherited_from_cluster_id: cluster.inherited_from_cluster_id.clone(),
            member_count: cluster.member_doc_ids.len(),
            sample_titles,
            member_doc_ids: cluster.member_doc_ids.clone(),
            fingerprint: cluster.fingerprint.clone(),
        });
    }

    let noise_sample: Vec<String> = outcome
        .noise_doc_ids
        .iter()
        .take(20)
        .cloned()
        .collect();

    let report = DebugReport {
        schema_version: "corpus-shape-debug-v1",
        run: DebugRunBlock {
            run_id: outcome.run.id.clone(),
            ran_at: outcome.run.ran_at.to_rfc3339(),
            duration_ms: outcome.run.duration_ms,
            total_docs: outcome.run.doc_count,
            llm_calls: outcome.run.llm_calls,
        },
        parameters: DebugParamsBlock {
            min_cluster_size:
                crate::features::corpus_shape::clustering::DEFAULT_MIN_CLUSTER_SIZE,
            min_samples: crate::features::corpus_shape::clustering::DEFAULT_MIN_SAMPLES,
            jaccard_inheritance_threshold:
                crate::features::corpus_shape::fingerprint::JACCARD_INHERITANCE_THRESHOLD,
        },
        cluster_count: outcome.clusters.len(),
        clusters: cluster_blocks,
        noise: DebugNoiseBlock {
            count: outcome.noise_doc_ids.len(),
            sample_doc_ids: noise_sample,
        },
        notes: &[
            "Noise is documents HDBSCAN could not fit into a dense enough cluster. Not a bug.",
            "Re-running immediately should produce zero llm_calls and identical labels.",
            "If more than ~50% of docs land in noise, min_cluster_size is likely too high or the corpus is genuinely heterogeneous.",
        ],
    };

    let json = serde_json::to_string_pretty(&report)
        .map_err(|e| AppError::Serialization(format!("debug report serialize: {}", e)))?;

    let out_dir = dirs::document_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let filename = format!("lattice-cluster-debug-{}.json", timestamp);
    let full_path = out_dir.join(filename);

    tokio::fs::write(&full_path, &json).await.map_err(|e| {
        AppError::FileStorage(format!("write debug report {}: {}", full_path.display(), e))
    })?;

    tracing::info!(
        path = %full_path.display(),
        clusters = outcome.clusters.len(),
        noise = outcome.noise_doc_ids.len(),
        "corpus_shape: wrote debug report"
    );

    Ok(full_path)
}

async fn gather_sample_titles(
    chunk_repo: &Arc<dyn ChunkRepositoryPort>,
    document_repo: &Arc<dyn crate::application::ports::DocumentRepository>,
    cluster: &Cluster,
) -> Vec<String> {
    let sample_ids: Vec<&String> = cluster.member_doc_ids.iter().take(5).collect();
    let mut titles = Vec::with_capacity(sample_ids.len());
    for id in sample_ids {
        match document_repo.find_by_id(id).await {
            Ok(Some(doc)) => titles.push(doc.file_name().to_string()),
            _ => {
                let _ = chunk_repo; // keep the arg list parallel even if unused here
                titles.push(id.clone());
            }
        }
    }
    titles
}

fn run_to_dto(run: &ClusterRun) -> ClusterRunDto {
    ClusterRunDto {
        run_id: run.id.clone(),
        ran_at: run.ran_at.to_rfc3339(),
        doc_count: run.doc_count,
        cluster_count: run.cluster_count,
        noise_count: run.noise_count,
        duration_ms: run.duration_ms,
        llm_calls: run.llm_calls,
    }
}

fn cluster_to_dto(cluster: &Cluster, sample_titles: Vec<String>) -> ClusterDto {
    ClusterDto {
        id: cluster.id.clone(),
        label: cluster.label.clone(),
        description: cluster.description.clone(),
        member_count: cluster.member_doc_ids.len() as i64,
        sample_titles,
        label_source: match cluster.label_source {
            LabelSource::Llm => "llm".to_string(),
            LabelSource::InheritedExact => "inherited_exact".to_string(),
            LabelSource::InheritedJaccard => "inherited_jaccard".to_string(),
            LabelSource::Fallback => "fallback".to_string(),
        },
        inherited_from_cluster_id: cluster.inherited_from_cluster_id.clone(),
        member_document_ids: cluster.member_doc_ids.clone(),
    }
}

// ============================================================================
// DocumentContentPreviewPort adapter backed by chunk + document repos.
// Kept inside `commands.rs` because it's a thin wiring layer — if the need
// grows it can move to its own file.
// ============================================================================

struct ChunkBackedContentPreview {
    chunk_repo: Arc<dyn ChunkRepositoryPort>,
    document_repo: Arc<dyn crate::application::ports::DocumentRepository>,
}

impl ChunkBackedContentPreview {
    fn new(
        chunk_repo: Arc<dyn ChunkRepositoryPort>,
        document_repo: Arc<dyn crate::application::ports::DocumentRepository>,
    ) -> Self {
        Self {
            chunk_repo,
            document_repo,
        }
    }
}

#[async_trait::async_trait]
impl DocumentContentPreviewPort for ChunkBackedContentPreview {
    async fn preview(&self, document_id: &str) -> Result<Option<RepresentativeDoc>> {
        let doc = self.document_repo.find_by_id(document_id).await?;
        let Some(doc) = doc else {
            return Ok(None);
        };

        // Pull up to a few chunks, concatenate up to REP_CONTENT_PREVIEW_CHARS.
        let chunks = self.chunk_repo.find_by_document(document_id).await?;
        let mut preview = String::new();
        for chunk in chunks {
            if preview.chars().count() >= REP_CONTENT_PREVIEW_CHARS {
                break;
            }
            if !preview.is_empty() {
                preview.push('\n');
            }
            preview.push_str(chunk.content());
        }

        let trimmed: String = preview.chars().take(REP_CONTENT_PREVIEW_CHARS).collect();

        Ok(Some(RepresentativeDoc {
            title: doc.file_name().to_string(),
            content_preview: trimmed,
        }))
    }
}
