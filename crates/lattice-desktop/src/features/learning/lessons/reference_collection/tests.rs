#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
use super::*;
use crate::features::learning::{
    repository::LearningRepository, source_library::CapturedLearningSource,
    sources::InitialReference, tests,
};
use lattice_core::application::ports::{LibraryChunk, LibraryPassagesPort};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};

#[test]
fn verification_windows_restore_conditions_without_changing_source_bytes() -> Result<()> {
    let before = "This procedure applies only while the local setting is active.\n";
    let target = "The selected value follows the local setting.";
    let after = "\nAn explicit override takes precedence over that setting.";
    let text = format!(
        "{before}{}{target}{}{after}",
        "背景🌱 ".repeat(150),
        " context ".repeat(100)
    );
    let source = LearningSourceDto {
        id: "reference".into(),
        title: "Procedure".into(),
        url: None,
        excerpt: text.clone(),
        acquired_at: 1,
    };
    let collection = ReferenceCollection::lexical(&[source])?;
    let start = text.find(target).unwrap();
    let hit = ReferencePassage {
        source_id: "reference".into(),
        text: target.into(),
        start_byte: start,
        end_byte: start + target.len(),
        retrieval_kind: "lexical_fallback".into(),
        score: 1.0,
    };
    let expanded = collection.verification_context(vec![hit.clone(), hit.clone()])?;
    assert_eq!(
        expanded.len(),
        1,
        "Overlapping context must not crowd out independent sources"
    );
    assert!(expanded[0].text.contains(before));
    assert!(expanded[0].text.contains(after));
    assert_eq!(
        &text[expanded[0].start_byte..expanded[0].end_byte],
        expanded[0].text
    );
    assert_eq!(expanded[0].score, hit.score);
    let mut invented = hit;
    invented.text = "A fabricated quotation".into();
    assert!(collection.verification_context(vec![invented]).is_err());
    Ok(())
}

#[test]
fn merging_context_keeps_both_partly_overlapping_hits() -> Result<()> {
    let text = format!(
        "{}first matching fact\n{}second matching caveat\n{}",
        "préfixe ".repeat(400),
        "background ".repeat(250),
        "suffix ".repeat(400)
    );
    let source = LearningSourceDto {
        id: "reference".into(),
        title: "Procedure".into(),
        url: None,
        excerpt: text.clone(),
        acquired_at: 1,
    };
    let collection = ReferenceCollection::lexical(&[source])?;
    let passages = ["first matching fact", "second matching caveat"].map(|target| {
        let start = text.find(target).unwrap();
        ReferencePassage {
            source_id: "reference".into(),
            text: target.into(),
            start_byte: start,
            end_byte: start + target.len(),
            retrieval_kind: "lexical_fallback".into(),
            score: 1.0,
        }
    });
    let expanded = collection.verification_context(passages.into())?;
    assert_eq!(expanded.len(), 1);
    assert!(expanded[0].text.contains("first matching fact"));
    assert!(expanded[0].text.contains("second matching caveat"));
    assert_eq!(
        &text[expanded[0].start_byte..expanded[0].end_byte],
        expanded[0].text
    );
    Ok(())
}

/// A library holding fixed documents. Search returns the scoped documents'
/// chunks containing a query word, in document order, and records each scope.
#[derive(Default)]
pub(in crate::features::learning) struct FakeLibrary {
    documents: HashMap<String, Vec<LibraryChunk>>,
    importing: HashSet<String>,
    scopes: std::sync::Mutex<Vec<HashSet<String>>>,
}

impl FakeLibrary {
    pub(in crate::features::learning) fn with_document(
        mut self,
        id: &str,
        chunks: &[&str],
    ) -> Self {
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
async fn collection_fixture(
    pool: &SqlitePool,
    documents: [&str; 2],
) -> Result<(String, Vec<LearningSourceDto>)> {
    let mut program = tests::fixture();
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
                signals: crate::features::learning::references::trust::capture::not_extracted(
                    "test fixture",
                ),
            },
        })
        .collect();
    let repo = LearningRepository::new(pool.clone());
    repo.create_with_references(&program, &references).await?;
    let sources = repo.verification_sources(&program.summary.id).await?;
    Ok((program.summary.id, sources))
}

fn source_of<'s>(
    sources: &'s [LearningSourceDto],
    hit: &ReferencePassage,
) -> &'s LearningSourceDto {
    sources.iter().find(|s| s.id == hit.source_id).unwrap()
}

#[tokio::test]
async fn document_and_web_snapshots_rank_through_the_course_index() -> Result<()> {
    use crate::features::learning::references::course_index::{tests::indexes, IndexMode};
    use crate::infrastructure::passage_index::testing::{FakeEmbedder, FixedEmbedders};
    let pool = tests::pool().await?;
    let (id, sources) = collection_fixture(&pool, ["doc-ownership", "doc-plants"]).await?;
    // Another course's references live in another index and never rank here.
    let (other, _) = collection_fixture(&pool, ["doc-elsewhere", "doc-elsewhere-2"]).await?;
    let temp = tempfile::tempdir().unwrap();
    let courses = indexes(
        &pool,
        temp.path(),
        FixedEmbedders::new(Some(FakeEmbedder::new("fixture-model"))),
    );
    courses.ensure_current_quietly(&other).await?;
    let course = courses
        .search(&id, tokio_util::sync::CancellationToken::new())
        .await?;
    let collection = ReferenceCollection::current(Some(&course), &sources).await?;
    assert_eq!(collection.mode(), IndexMode::Hybrid);
    assert_eq!(
        collection.embedding_model().as_deref(),
        Some("fixture-model")
    );
    for (query, expected) in [
        ("borrowing", "Borrowing gives access"),
        ("photosynthesis", "chloroplasts"),
        ("pastry crust", "Keep butter cold"),
    ] {
        let hits = collection.retrieve(query, 2).await?;
        assert!(hits[0].text.contains(expected), "{query}: {hits:?}");
        assert_eq!(hits[0].retrieval_kind, "hybrid", "{query}");
        assert!(hits
            .iter()
            .all(|hit| sources.iter().any(|s| s.id == hit.source_id)));
        assert_eq!(
            &source_of(&sources, &hits[0]).excerpt[hits[0].start_byte..hits[0].end_byte],
            hits[0].text
        );
    }
    Ok(())
}

#[tokio::test]
async fn without_a_course_index_every_snapshot_is_ranked_by_its_words() -> Result<()> {
    let pool = tests::pool().await?;
    let (_, sources) = collection_fixture(&pool, ["doc-ownership", "doc-plants"]).await?;
    let lexical = ReferenceCollection::current(None, &sources).await?;
    assert!(lexical.retrieve("sunlight conversion", 3).await?.is_empty());
    let hits = lexical.retrieve("photosynthesis", 3).await?;
    assert!(!hits.is_empty());
    assert!(hits.iter().all(|h| h.retrieval_kind == "lexical"));
    assert_eq!(lexical.mode().label(), "lexical");
    assert_eq!(lexical.embedding_model(), None);
    Ok(())
}

#[tokio::test]
async fn a_ranking_keeps_to_its_scope_and_records_its_branches() -> Result<()> {
    let pool = tests::pool().await?;
    let (_, sources) = collection_fixture(&pool, ["doc-ownership", "doc-plants"]).await?;
    let collection = ReferenceCollection::lexical(&sources)?;
    let plants = sources
        .iter()
        .find(|source| source.excerpt.contains("chloroplasts"))
        .unwrap()
        .id
        .clone();
    let scope = Scope::Keys(HashSet::from([plants.clone()]));
    let ranked = collection
        .rank(&["photosynthesis", "borrowing"], &scope, 4)
        .await?;
    assert!(!ranked.is_empty());
    assert!(ranked.iter().all(|hit| hit.passage.source_id == plants));
    assert!(ranked
        .iter()
        .all(|hit| hit.lexical.is_some() && hit.dense.is_none()));
    assert!(collection
        .rank(
            &["photosynthesis"],
            &Scope::Keys(HashSet::from(["elsewhere".to_string()])),
            4
        )
        .await?
        .is_empty());
    Ok(())
}

#[tokio::test]
#[ignore = "Downloads the public Rust Book through the production safe web reader"]
async fn live_rust_book_is_captured_beyond_chat_limit() -> Result<()> {
    use lattice_core::application::ports::WebServiceTrait;
    let dir = tempfile::tempdir().unwrap();
    let web = crate::features::web::services::web::WebService::new(dir.path())?;
    let page = web
        .fetch_reference_content("https://doc.rust-lang.org/book/print.html")
        .await?;
    assert!(!page.content_truncated);
    assert!(page.content.chars().count() > 50_000);
    assert!(page.content.contains("Appendix G"));
    println!(
        "Captured {} characters; truncation={}; final URL={}",
        page.content.chars().count(),
        page.content_truncated,
        page.url
    );
    Ok(())
}

#[tokio::test]
async fn large_collection_keeps_evidence_after_the_former_twenty_million_character_boundary(
) -> Result<()> {
    let suffix = "Unique nebular measurement equals forty seven units.";
    let source = LearningSourceDto {
        id: "large-reference".into(),
        title: "Large reference".into(),
        url: None,
        excerpt: format!("{}\n{suffix}", "background ".repeat(1_820_000)),
        acquired_at: 0,
    };
    assert!(source.excerpt.len() > 20_000_000);
    let collection = ReferenceCollection::lexical(&[source])?;
    let hits = collection.retrieve("nebular measurement", 1).await?;
    assert!(hits[0].text.contains(suffix));
    assert_eq!(
        &collection.sources[0].excerpt[hits[0].start_byte..hits[0].end_byte],
        hits[0].text
    );
    Ok(())
}

#[test]
fn unicode_passage_offsets_remain_lossless_without_a_whole_document_offset_array() {
    use crate::features::learning::references::course_index::{CHUNK_CHARS, STEP_CHARS};
    use crate::infrastructure::passage_index::chunking::spans::span_chunks;
    let text = "🧪é中sample\n".repeat(400);
    let passages = span_chunks(&text, CHUNK_CHARS, STEP_CHARS);
    assert!(passages.len() > 1);
    for passage in &passages {
        assert_eq!(
            text.get(passage.start..passage.end),
            Some(passage.text.as_str())
        );
        assert!(passage.text.chars().count() <= CHUNK_CHARS);
    }
    assert!(passages.last().unwrap().end == text.trim_end().len());
}

#[tokio::test]
async fn deeper_evidence_must_mention_what_the_claim_is_about() -> Result<()> {
    let sources: Vec<LearningSourceDto> = [
        "ASP.NET Core hosts the app. ASP.NET Core configures logging.",
        "ASP.NET Core runs middleware in order for each request.",
        "ASP.NET Core minimal APIs support endpoint filters.",
    ]
    .iter()
    .enumerate()
    .map(|(i, text)| LearningSourceDto {
        id: format!("s{i}"),
        title: format!("Reference {i}"),
        url: None,
        excerpt: (*text).into(),
        acquired_at: 1,
    })
    .collect();
    let collection = ReferenceCollection::lexical(&sources)?;
    let claim = "ASP.NET Core endpoint filters run before the handler.";
    let subject = collection.subject_of(claim).await?;
    // Shares only the subject's common words.
    assert!(!subject.mentioned_in("ASP.NET Core runs middleware in order."));
    // Mentions what the claim is about.
    assert!(subject.mentioned_in("Endpoint filters wrap a minimal API handler."));
    // A page sharing nothing with the claim stops the walk.
    assert!(!collection
        .subject_of("ASP.NET Core")
        .await?
        .mentioned_in("Vacuum reclaims storage."));
    Ok(())
}

#[tokio::test]
async fn keyword_ranking_is_the_same_on_every_build_and_call() -> Result<()> {
    // Near-tied passages: the same terms, in different amounts.
    let words = ["schema", "tool", "enum", "value", "model", "call", "field"];
    let sources: Vec<_> = (0..24)
        .map(|index| LearningSourceDto {
            id: format!("source-{index}"),
            title: format!("Source {index}"),
            url: None,
            excerpt: words
                .iter()
                .enumerate()
                .map(|(at, word)| format!("{word} ").repeat(1 + (index + at) % 4))
                .collect(),
            acquired_at: 1,
        })
        .collect();
    let query = words.join(" ");
    let order = |hits: Vec<ReferencePassage>| {
        hits.into_iter()
            .map(|hit| (hit.source_id, hit.start_byte))
            .collect::<Vec<_>>()
    };
    let first = order(
        ReferenceCollection::lexical(&sources)?
            .retrieve(&query, 24)
            .await?,
    );
    for _ in 0..16 {
        let rebuilt = ReferenceCollection::lexical(&sources)?;
        assert_eq!(order(rebuilt.retrieve(&query, 24).await?), first);
    }
    Ok(())
}

#[tokio::test]
async fn retrieve_from_reads_one_source_version_only() -> Result<()> {
    let source = |id: &str, text: &str| LearningSourceDto {
        id: id.into(),
        title: id.into(),
        url: None,
        excerpt: text.into(),
        acquired_at: 1,
    };
    let collection = ReferenceCollection::lexical(&[
        source(
            "course-notes",
            "Water boils at 100 degrees Celsius at sea level, the notes say.",
        ),
        source(
            "added-by-you",
            "At sea level pure water boils at 100 degrees Celsius; on a mountain it boils lower.",
        ),
    ])?;
    let passages = collection
        .retrieve_from("added-by-you", "water boils at sea level", |_| true)
        .await?;
    assert!(!passages.is_empty());
    assert!(passages
        .iter()
        .all(|passage| passage.source_id == "added-by-you"));
    assert!(collection
        .retrieve_from("not-in-the-course", "water boils", |_| true)
        .await?
        .is_empty());
    Ok(())
}
