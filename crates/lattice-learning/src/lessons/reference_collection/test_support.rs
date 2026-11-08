//! A library holding fixed documents, shared with lattice-desktop's tests.

#![allow(clippy::indexing_slicing, clippy::unwrap_used)]

use super::{invalid, ReferencePassage};
use crate::dto::LearningSourceDto;
use crate::repository::LearningRepository;
use crate::source_library::CapturedLearningSource;
use crate::sources::InitialReference;
use lattice_core::application::ports::{LibraryChunk, LibraryPassagesPort};
use lattice_core::shared::error::Result;
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};

/// A library holding fixed documents. Search returns the scoped documents'
/// chunks containing a query word, in document order, and records each scope.
#[derive(Default)]
pub struct FakeLibrary {
    documents: HashMap<String, Vec<LibraryChunk>>,
    importing: HashSet<String>,
    scopes: std::sync::Mutex<Vec<HashSet<String>>>,
}

impl FakeLibrary {
    pub fn with_document(mut self, id: &str, chunks: &[&str]) -> Self {
        let chunks = chunks
            .iter()
            .enumerate()
            .map(|(index, text)| LibraryChunk {
                id: format!("{id}#{index}"),
                text: (*text).into(),
            })
            .collect();
        self.documents.insert(id.into(), chunks);
        self
    }
}

#[async_trait::async_trait]
impl LibraryPassagesPort for FakeLibrary {
    fn model_identity(&self) -> String {
        "fixture-library-model".into()
    }
    async fn document_text(
        &self,
        document_id: &str,
    ) -> Result<Option<lattice_core::application::ports::LibraryDocumentText>> {
        if self.importing.contains(document_id) {
            return Err(invalid("The document is not fully indexed."));
        }
        Ok(self.documents.get(document_id).map(|chunks| {
            lattice_core::application::ports::LibraryDocumentText {
                title: format!("{document_id}.md"),
                file_path: format!("/library/{document_id}"),
                mime_type: "text/markdown".into(),
                chunks: chunks.clone(),
            }
        }))
    }
    async fn search(
        &self,
        query: &str,
        document_ids: &HashSet<String>,
        limit: usize,
    ) -> Result<Vec<lattice_core::application::ports::LibraryPassageHit>> {
        self.scopes.lock().unwrap().push(document_ids.clone());
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        let mut scoped: Vec<_> = document_ids.iter().collect();
        scoped.sort();
        let mut hits = Vec::new();
        for document_id in scoped {
            for chunk in self.documents.get(document_id).into_iter().flatten() {
                let text = chunk.text.to_lowercase();
                if words.iter().any(|word| text.contains(word.as_str())) {
                    hits.push(lattice_core::application::ports::LibraryPassageHit {
                        document_id: document_id.clone(),
                        chunk_id: chunk.id.clone(),
                        score: 1.0,
                    });
                }
            }
        }
        hits.truncate(limit);
        Ok(hits)
    }
}

const OWNERSHIP: [&str; 2] = [
    "# Ownership\nBorrowing gives access without transferring ownership. 🦀",
    "Lifetimes describe how long a reference stays valid.",
];
const PLANTS: [&str; 1] = ["# Plant cells\nPhotosynthesis takes place in chloroplasts."];

/// A program with a document snapshot, a web snapshot and a second document
/// snapshot, captured the way the app captures them.
pub async fn collection_fixture(
    pool: &SqlitePool,
    documents: [&str; 2],
) -> Result<(String, Vec<LearningSourceDto>)> {
    let mut program = crate::test_support::fixture();
    let texts = [
        OWNERSHIP.join("\n\n"),
        "# Pastry\nKeep butter cold when making a flaky pie crust.".to_string(),
        PLANTS.join("\n\n"),
    ];
    program.sources = texts
        .iter()
        .enumerate()
        .map(|(i, text)| LearningSourceDto {
            id: uuid::Uuid::new_v4().to_string(),
            title: format!("Reference {i}"),
            url: (i == 1).then(|| "https://example.org/pastry".into()),
            excerpt: text.chars().take(2400).collect(),
            acquired_at: 1,
        })
        .collect();
    let origins = [documents[0], "https://example.org/pastry", documents[1]];
    let references: Vec<_> = program
        .sources
        .iter()
        .zip(&texts)
        .zip(origins)
        .map(|((s, text), origin)| InitialReference {
            source_id: s.id.clone(),
            origin: origin.into(),
            captured: CapturedLearningSource {
                title: s.title.clone(),
                publisher: None,
                requested_url: s.url.clone(),
                resolved_url: s.url.clone(),
                text: text.clone(),
                truncated: false,
                extraction_version: "fixture_full_text".into(),
                signals: crate::references::trust::capture::not_extracted("test fixture"),
            },
            found: false,
        })
        .collect();
    let repo = LearningRepository::new(pool.clone());
    repo.create_with_references(&program, &references).await?;
    let sources = repo.verification_sources(&program.summary.id).await?;
    Ok((program.summary.id, sources))
}

pub fn source_of<'s>(
    sources: &'s [LearningSourceDto],
    hit: &ReferencePassage,
) -> &'s LearningSourceDto {
    sources.iter().find(|s| s.id == hit.source_id).unwrap()
}
