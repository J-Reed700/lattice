//! Summary tier tests.
//!
//! Everything a model would normally provide is stubbed: a fixture source, an
//! LLM that returns whatever the test hands it, and a deterministic embedder.
//! The real database and the real USearch index are used as-is, because those
//! are the two places a summary can be written and then not found again.

#![cfg(test)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use futures::stream::Stream;
use parking_lot::Mutex;
use sqlx::SqlitePool;

use crate::application::ports::vector_search_port::VectorSearchPort;
use crate::application::ports::{EmbeddingPort, LLMPort};
use crate::features::search::engine::vector_search::USearchVectorIndex;
use crate::features::summaries::entity::{DocumentSummary, SummaryHit, SummaryLevel};
use crate::features::summaries::openings::select_opening_documents;
use crate::features::summaries::repository::{
    document_level_text, SqliteSummaryRepository, SummaryRepositoryPort,
};
use crate::features::summaries::runtime::SummaryRuntimePort;
use crate::features::summaries::search::SummarySearchPort;
use crate::features::summaries::source::{SummarySection, SummarySource, SummarySourcePort};
use crate::features::summaries::use_cases::GenerateDocumentSummariesUseCase;
use crate::shared::error::Result;

const DIMENSION: usize = 8;
const IDENTITY: &str = "test-embedder:v1";

// ---------------------------------------------------------------- fixtures

/// Returns a canned response per call, so a test can mix valid JSON, partial
/// JSON and garbage in one run and see exactly which summaries survive.
struct ScriptedLlm {
    responses: Mutex<Vec<String>>,
    calls: Arc<Mutex<Vec<String>>>,
}

impl ScriptedLlm {
    fn new(responses: &[&str]) -> Self {
        let mut responses: Vec<String> = responses.iter().map(|r| (*r).to_owned()).collect();
        responses.reverse();
        Self {
            responses: Mutex::new(responses),
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn prompts(&self) -> Arc<Mutex<Vec<String>>> {
        Arc::clone(&self.calls)
    }
}

#[async_trait]
impl LLMPort for ScriptedLlm {
    async fn generate(
        &self,
        prompt: &str,
        _context: &[String],
        _images: Option<Vec<String>>,
    ) -> Result<String> {
        self.calls.lock().push(prompt.to_owned());
        Ok(self.responses.lock().pop().unwrap_or_default())
    }

    async fn generate_streaming(
        &self,
        _prompt: &str,
        _context: &[String],
        _images: Option<Vec<String>>,
    ) -> Result<Box<dyn Stream<Item = Result<String>> + Send + Unpin + '_>> {
        Ok(Box::new(Box::pin(futures::stream::empty())))
    }

    fn model_name(&self) -> &str {
        "scripted"
    }

    fn max_context_tokens(&self) -> usize {
        8192
    }

    fn count_tokens(&self, text: &str) -> usize {
        text.split_whitespace().count()
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// Deterministic unit vectors: equal texts embed equally, and a query that
/// repeats a summary's words lands nearest that summary.
struct WordEmbedder;

impl WordEmbedder {
    fn vector(text: &str) -> Vec<f32> {
        let mut vector = vec![0.0_f32; DIMENSION];
        for word in text.to_lowercase().split_whitespace() {
            let bucket = word
                .bytes()
                .fold(0_usize, |acc, byte| acc.wrapping_add(byte as usize))
                % DIMENSION;
            if let Some(slot) = vector.get_mut(bucket) {
                *slot += 1.0;
            }
        }
        let magnitude = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
        if magnitude > 0.0 {
            for value in &mut vector {
                *value /= magnitude;
            }
        } else if let Some(first) = vector.first_mut() {
            *first = 1.0;
        }
        vector
    }
}

#[async_trait]
impl EmbeddingPort for WordEmbedder {
    async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        Ok(Self::vector(text))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|text| Self::vector(text)).collect())
    }

    fn model_identity(&self) -> String {
        IDENTITY.to_string()
    }

    fn dimension(&self) -> usize {
        DIMENSION
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

struct StubRuntime {
    llm: Option<Arc<dyn LLMPort>>,
}

#[async_trait]
impl SummaryRuntimePort for StubRuntime {
    async fn utility_llm(&self) -> Option<Arc<dyn LLMPort>> {
        self.llm.clone()
    }

    async fn embedder(&self) -> Option<Arc<dyn EmbeddingPort>> {
        Some(Arc::new(WordEmbedder))
    }
}

struct FixtureSource {
    source: Option<SummarySource>,
}

#[async_trait]
impl SummarySourcePort for FixtureSource {
    async fn load(&self, _document_id: &str) -> Result<Option<SummarySource>> {
        Ok(self.source.clone())
    }
}

/// A summary tier that returns whatever a test scripted, used to check the
/// selection rules without a model or an index.
struct StubSummarySearch {
    hits: Vec<SummaryHit>,
}

#[async_trait]
impl SummarySearchPort for StubSummarySearch {
    async fn top_summaries(
        &self,
        _query: &str,
        scope: &HashSet<String>,
        limit: usize,
    ) -> Result<Vec<SummaryHit>> {
        Ok(self
            .hits
            .iter()
            .filter(|hit| scope.contains(&hit.document_id))
            .take(limit)
            .cloned()
            .collect())
    }
}

fn source_with_sections(sections: usize) -> SummarySource {
    SummarySource {
        document_id: "doc-1".into(),
        title: "Flight Manual".into(),
        body: "This manual covers preflight, engine start and emergencies.".into(),
        cluster_label: Some("Aviation Procedures".into()),
        sections: (0..sections)
            .map(|i| SummarySection {
                heading: format!("Section {i}"),
                text: format!("Section {i} explains a procedure in detail."),
            })
            .collect(),
    }
}

async fn pool_with_document() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    for (id, name) in [("doc-1", "manual.pdf"), ("doc-2", "primer.pdf")] {
        sqlx::query("INSERT INTO documents (id, file_path, file_name, size_bytes, modified_at, checksum) VALUES (?, ?, ?, 1, '2026-09-16', ?)")
            .bind(id)
            .bind(format!("/library/{name}"))
            .bind(name)
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
    }
    pool
}

fn summary(document_id: &str, section: Option<&str>, text: &str) -> DocumentSummary {
    DocumentSummary::new(
        document_id,
        section.map(str::to_owned),
        section.map_or(SummaryLevel::Document, |_| SummaryLevel::Section),
        text,
        IDENTITY,
    )
}

// -------------------------------------------------------------- repository

#[tokio::test]
async fn repository_round_trips_every_field() {
    let pool = pool_with_document().await;
    let repository = SqliteSummaryRepository::new(pool.clone());
    let written = vec![
        summary("doc-1", None, "The whole manual."),
        summary("doc-1", Some("Emergencies"), "What to do when it burns."),
    ];

    let replaced = repository
        .replace_for_document("doc-1", IDENTITY, &written)
        .await
        .unwrap();
    assert!(replaced.is_empty(), "nothing existed to replace");

    let scope: HashSet<String> = ["doc-1".to_string()].into_iter().collect();
    let mut read = repository
        .list_for_documents(&scope, IDENTITY)
        .await
        .unwrap();
    read.sort_by(|a, b| a.level.as_str().cmp(b.level.as_str()));
    assert_eq!(read.len(), 2);
    let document = read
        .iter()
        .find(|s| s.level == SummaryLevel::Document)
        .unwrap();
    let section = read
        .iter()
        .find(|s| s.level == SummaryLevel::Section)
        .unwrap();
    assert_eq!(document.summary_text, "The whole manual.");
    assert_eq!(document.section, None);
    assert_eq!(document.model_identity, IDENTITY);
    assert_eq!(section.section.as_deref(), Some("Emergencies"));
    // Timestamps survive the RFC 3339 round trip to the second.
    assert_eq!(
        document.created_at.timestamp(),
        written[0].created_at.timestamp()
    );

    assert_eq!(
        document_level_text(&read).get("doc-1").map(String::as_str),
        Some("The whole manual.")
    );
    let by_id = repository
        .find_by_ids(&[written[1].id.clone()])
        .await
        .unwrap();
    assert_eq!(by_id.len(), 1);
    assert_eq!(by_id[0].id, written[1].id);
}

#[tokio::test]
async fn regenerating_replaces_rather_than_accumulates() {
    let pool = pool_with_document().await;
    let repository = SqliteSummaryRepository::new(pool.clone());
    let first = vec![summary("doc-1", None, "First pass.")];
    repository
        .replace_for_document("doc-1", IDENTITY, &first)
        .await
        .unwrap();

    let second = vec![summary("doc-1", None, "Second pass.")];
    let replaced = repository
        .replace_for_document("doc-1", IDENTITY, &second)
        .await
        .unwrap();
    assert_eq!(replaced, vec![first[0].id.clone()]);
    assert_eq!(repository.count_for_identity(IDENTITY).await.unwrap(), 1);
}

#[tokio::test]
async fn other_identities_are_pruned_and_deleted_documents_cascade() {
    let pool = pool_with_document().await;
    let repository = SqliteSummaryRepository::new(pool.clone());
    repository
        .replace_for_document("doc-1", IDENTITY, &[summary("doc-1", None, "Current.")])
        .await
        .unwrap();
    repository
        .replace_for_document("doc-2", "older-model:v0", &[summary("doc-2", None, "Old.")])
        .await
        .unwrap();

    assert_eq!(
        repository.prune_other_identities(IDENTITY).await.unwrap(),
        1
    );
    assert_eq!(repository.count_for_identity(IDENTITY).await.unwrap(), 1);

    sqlx::query("DELETE FROM documents WHERE id = 'doc-1'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(repository.count_for_identity(IDENTITY).await.unwrap(), 0);
}

#[tokio::test]
async fn deleting_a_document_returns_the_ids_whose_vectors_must_go() {
    let pool = pool_with_document().await;
    let repository = SqliteSummaryRepository::new(pool.clone());
    let written = vec![summary("doc-1", None, "Text.")];
    repository
        .replace_for_document("doc-1", IDENTITY, &written)
        .await
        .unwrap();
    assert_eq!(
        repository.delete_for_document("doc-1").await.unwrap(),
        vec![written[0].id.clone()]
    );
}

// ------------------------------------------------------------- generation

fn use_case(
    enabled: bool,
    source: Option<SummarySource>,
    responses: &[&str],
    pool: &SqlitePool,
    index: Arc<dyn VectorSearchPort>,
) -> (GenerateDocumentSummariesUseCase, Arc<Mutex<Vec<String>>>) {
    let llm = ScriptedLlm::new(responses);
    let prompts = llm.prompts();
    let use_case = GenerateDocumentSummariesUseCase::new(
        enabled,
        Arc::new(FixtureSource { source }),
        Arc::new(SqliteSummaryRepository::new(pool.clone())),
        index,
        Arc::new(StubRuntime {
            llm: Some(Arc::new(llm)),
        }),
    );
    (use_case, prompts)
}

fn memory_index() -> Arc<dyn VectorSearchPort> {
    Arc::new(USearchVectorIndex::new(DIMENSION, None).unwrap())
}

#[tokio::test]
async fn disabled_is_a_no_op() {
    let pool = pool_with_document().await;
    let index = memory_index();
    let (use_case, prompts) = use_case(
        false,
        Some(source_with_sections(4)),
        &[r#"{"summary":"S.","topics":["a"]}"#],
        &pool,
        Arc::clone(&index),
    );
    assert_eq!(use_case.execute("doc-1").await.unwrap(), 0);
    assert!(prompts.lock().is_empty(), "no model call while disabled");
    assert_eq!(index.count(), 0);
}

#[tokio::test]
async fn generates_document_and_section_summaries_and_indexes_them() {
    let pool = pool_with_document().await;
    let index = memory_index();
    let responses: Vec<&str> =
        vec![r#"{"summary":"Three sentences.","topics":["preflight","engines"]}"#; 5];
    let (use_case, prompts) = use_case(
        true,
        Some(source_with_sections(4)),
        &responses,
        &pool,
        Arc::clone(&index),
    );

    // One document summary plus one per section.
    assert_eq!(use_case.execute("doc-1").await.unwrap(), 5);
    assert_eq!(index.count(), 5);

    let prompts = prompts.lock().clone();
    assert!(prompts[0].contains("Title: Flight Manual"));
    assert!(prompts[0].contains("Collection theme: Aviation Procedures"));
    assert!(prompts[0].contains("Section headings: Section 0 | Section 1"));
    assert!(prompts[1].contains("Section: Section 0"));

    let repository = SqliteSummaryRepository::new(pool.clone());
    let scope: HashSet<String> = ["doc-1".to_string()].into_iter().collect();
    let stored = repository
        .list_for_documents(&scope, IDENTITY)
        .await
        .unwrap();
    assert_eq!(stored.len(), 5);
    assert_eq!(
        stored
            .iter()
            .filter(|s| s.level == SummaryLevel::Section)
            .count(),
        4
    );
    assert!(stored[0]
        .summary_text
        .contains("Key topics: preflight, engines"));
}

#[tokio::test]
async fn too_few_sections_means_only_a_document_summary() {
    let pool = pool_with_document().await;
    let index = memory_index();
    let (use_case, prompts) = use_case(
        true,
        Some(source_with_sections(2)),
        &[r#"{"summary":"Just the document.","topics":[]}"#],
        &pool,
        Arc::clone(&index),
    );
    assert_eq!(use_case.execute("doc-1").await.unwrap(), 1);
    assert_eq!(prompts.lock().len(), 1);
}

#[tokio::test]
async fn section_summaries_are_capped_per_document() {
    let pool = pool_with_document().await;
    let index = memory_index();
    let responses: Vec<&str> = vec![r#"{"summary":"S."}"#; 40];
    let (use_case, _) = use_case(
        true,
        Some(source_with_sections(40)),
        &responses,
        &pool,
        Arc::clone(&index),
    );
    let written = use_case.execute("doc-1").await.unwrap();
    assert_eq!(
        written,
        1 + crate::features::summaries::prompt::MAX_SECTION_SUMMARIES
    );
}

#[tokio::test]
async fn unparseable_responses_are_skipped_not_stored() {
    let pool = pool_with_document().await;
    let index = memory_index();
    // Document summary is garbage; the first section is fine; the rest are empty.
    let (use_case, _) = use_case(
        true,
        Some(source_with_sections(3)),
        &[
            "I'm sorry, I can't do that.",
            r#"{"summary":"A real section summary."}"#,
            "",
            "{}",
        ],
        &pool,
        Arc::clone(&index),
    );
    assert_eq!(use_case.execute("doc-1").await.unwrap(), 1);
    assert_eq!(index.count(), 1);
    let repository = SqliteSummaryRepository::new(pool.clone());
    let scope: HashSet<String> = ["doc-1".to_string()].into_iter().collect();
    let stored = repository
        .list_for_documents(&scope, IDENTITY)
        .await
        .unwrap();
    assert_eq!(stored[0].level, SummaryLevel::Section);
}

#[tokio::test]
async fn a_document_with_no_text_and_a_missing_model_produce_nothing() {
    let pool = pool_with_document().await;
    let index = memory_index();
    let (empty, _) = use_case(
        true,
        Some(SummarySource {
            document_id: "doc-1".into(),
            title: "Empty".into(),
            body: "   ".into(),
            cluster_label: None,
            sections: Vec::new(),
        }),
        &[r#"{"summary":"S."}"#],
        &pool,
        Arc::clone(&index),
    );
    assert_eq!(empty.execute("doc-1").await.unwrap(), 0);

    let no_model = GenerateDocumentSummariesUseCase::new(
        true,
        Arc::new(FixtureSource {
            source: Some(source_with_sections(4)),
        }),
        Arc::new(SqliteSummaryRepository::new(pool.clone())),
        Arc::clone(&index),
        Arc::new(StubRuntime { llm: None }),
    );
    assert_eq!(no_model.execute("doc-1").await.unwrap(), 0);
    assert_eq!(index.count(), 0);
}

#[tokio::test]
async fn regeneration_swaps_the_vectors_rather_than_adding_to_them() {
    let pool = pool_with_document().await;
    let index = memory_index();
    for text in ["First.", "Second."] {
        let (use_case, _) = use_case(
            true,
            Some(source_with_sections(0)),
            &[&format!(r#"{{"summary":"{text}"}}"#)],
            &pool,
            Arc::clone(&index),
        );
        assert_eq!(use_case.execute("doc-1").await.unwrap(), 1);
    }
    assert_eq!(index.count(), 1, "the stale summary vector was removed");
}

// ------------------------------------------------------- retrieval / tier

#[tokio::test]
async fn summary_search_ranks_documents_and_ignores_rows_that_are_gone() {
    let pool = pool_with_document().await;
    let index = memory_index();
    let repository = Arc::new(SqliteSummaryRepository::new(pool.clone()));
    let aviation = summary("doc-1", None, "preflight engine start emergency landing");
    let baking = summary("doc-2", None, "sourdough starter hydration crumb");
    for document in [&aviation, &baking] {
        repository
            .replace_for_document(
                &document.document_id,
                IDENTITY,
                std::slice::from_ref(document),
            )
            .await
            .unwrap();
        index
            .publish_embeddings(vec![
                crate::application::ports::vector_search_port::VectorIndexEntry {
                    id: document.vector_id(),
                    embedding: WordEmbedder::vector(&document.summary_text),
                    content: document.summary_text.clone(),
                    chunk_id: document.id.clone(),
                    document_id: document.document_id.clone(),
                },
            ])
            .unwrap();
    }

    let search = crate::features::summaries::search::SummarySearch::new(
        Arc::clone(&index),
        repository.clone(),
        Arc::new(WordEmbedder),
    );
    let scope: HashSet<String> = ["doc-1", "doc-2"].iter().map(|s| s.to_string()).collect();
    let hits = search
        .top_summaries("preflight engine start emergency landing", &scope, 2)
        .await
        .unwrap();
    assert_eq!(
        hits.first().map(|hit| hit.document_id.as_str()),
        Some("doc-1")
    );

    // Deleting the document removes its row; the orphaned vector must not
    // survive as a hit.
    sqlx::query("DELETE FROM documents WHERE id = 'doc-1'")
        .execute(&pool)
        .await
        .unwrap();
    let hits = search
        .top_summaries("preflight engine start emergency landing", &scope, 2)
        .await
        .unwrap();
    assert!(hits.iter().all(|hit| hit.document_id != "doc-1"));

    let catalog = search.document_summaries(&scope).await.unwrap();
    assert_eq!(catalog.len(), 1);
    assert!(catalog.contains_key("doc-2"));
}

#[tokio::test]
async fn a_stubbed_tier_drives_opening_document_selection() {
    let hits = vec![
        SummaryHit {
            document_id: "doc-2".into(),
            summary_id: "s2".into(),
            level: SummaryLevel::Document,
            section: None,
            summary_text: "primer".into(),
            score: 0.91,
        },
        SummaryHit {
            document_id: "doc-1".into(),
            summary_id: "s1".into(),
            level: SummaryLevel::Section,
            section: Some("Intro".into()),
            summary_text: "manual".into(),
            score: 0.42,
        },
    ];
    let search = StubSummarySearch { hits };
    let scope: HashSet<String> = ["doc-1", "doc-2"].iter().map(|s| s.to_string()).collect();
    let found = search
        .top_summaries("where do I start", &scope, 4)
        .await
        .unwrap();
    assert_eq!(
        select_opening_documents(&found, &scope, 4),
        vec!["doc-2", "doc-1"]
    );

    // A scope that excludes the best hit falls through to the next one.
    let narrowed: HashSet<String> = ["doc-1".to_string()].into_iter().collect();
    let found = search
        .top_summaries("where do I start", &narrowed, 4)
        .await
        .unwrap();
    assert_eq!(
        select_opening_documents(&found, &narrowed, 4),
        vec!["doc-1"]
    );
}

#[tokio::test]
async fn forgetting_a_document_drops_its_rows_and_its_vectors() {
    let pool = pool_with_document().await;
    let index = memory_index();
    let (generator, _) = use_case(
        true,
        Some(source_with_sections(0)),
        &[r#"{"summary":"Only summary."}"#],
        &pool,
        Arc::clone(&index),
    );
    assert_eq!(generator.execute("doc-1").await.unwrap(), 1);
    assert_eq!(index.count(), 1);

    generator.forget("doc-1").await.unwrap();
    assert_eq!(index.count(), 0);
    let repository = SqliteSummaryRepository::new(pool.clone());
    assert_eq!(repository.count_for_identity(IDENTITY).await.unwrap(), 0);

    // Forgetting a document with nothing stored is a no-op, not an error.
    generator.forget("doc-2").await.unwrap();
}
