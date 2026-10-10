//! Tauri commands for corpus-shape clustering.
//!
//! - `cluster_vault_run` — runs + persists. Returns a summary DTO.
//! - `list_clusters`     — returns the most recent run's cluster list.
//!   This is a cheap, read-only operation.
//!
//! Both are explicit / user-triggered. Nothing here runs on ingest or
//! a timer.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::application::ports::{ChunkRepositoryPort, LLMPort};
use crate::features::corpus_shape::entity::{Cluster, ClusterRun, LabelSource};
use crate::features::corpus_shape::labeling::{RepresentativeDoc, REP_CONTENT_PREVIEW_CHARS};
use crate::features::corpus_shape::repository::{ClusterRepositoryPort, SqliteClusterRepository};
use crate::features::corpus_shape::use_cases::{
    run_clustering::DocumentContentPreviewPort, ProgressSink, RunClusteringUseCase,
};
use crate::interfaces::di::Container;
use crate::shared::error::Result;

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
        let sample_titles = gather_sample_titles(&chunk_repo, &document_repo, &cluster).await;
        dtos.push(cluster_to_dto(&cluster, sample_titles));
    }
    Ok(dtos)
}

async fn build_use_case(container: &Container) -> Result<RunClusteringUseCase> {
    let pool = container.db_pool().clone();
    let cluster_repo: Arc<dyn ClusterRepositoryPort> = Arc::new(SqliteClusterRepository::new(pool));

    let content_preview: Arc<dyn DocumentContentPreviewPort> =
        Arc::new(ChunkBackedContentPreview::new(
            container.chunk_repository(),
            container.document_repository(),
        ));

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
        crate::features::embedding::repository::EmbeddingRepository::new(
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
