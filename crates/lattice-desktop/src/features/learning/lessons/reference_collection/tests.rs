#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
use super::*;
use crate::features::learning::{
    repository::LearningRepository, source_library::CapturedLearningSource,
    sources::InitialReference, tests,
};
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
        branch: None,
        branch_score: 0.0,
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
            branch: None,
            branch_score: 0.0,
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
    fn scopes(&self) -> Vec<HashSet<String>> {
        self.scopes.lock().unwrap().clone()
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
async fn document_snapshots_rank_through_the_library_scoped_to_their_program() -> Result<()> {
    let pool = tests::pool().await?;
    let (id, sources) = collection_fixture(&pool, ["doc-ownership", "doc-plants"]).await?;
    // Another program's document must never enter this program's scope.
    collection_fixture(&pool, ["doc-elsewhere", "doc-elsewhere-2"]).await?;
    let library = FakeLibrary::default()
        .with_document("doc-ownership", &OWNERSHIP)
        .with_document("doc-plants", &PLANTS)
        .with_document("doc-elsewhere", &OWNERSHIP);
    let collection = ReferenceCollection::load(&pool, &id, &sources, Some(&library)).await?;
    assert_eq!(collection.mode(), "hybrid");
    assert_eq!(
        collection.embedding_model().as_deref(),
        Some("fixture-library-model")
    );
    for (query, expected, kind) in [
        ("borrowing", "Borrowing gives access", "hybrid"),
        ("photosynthesis", "chloroplasts", "hybrid"),
        ("pastry crust", "Keep butter cold", "lexical_fallback"),
    ] {
        let hits = collection.retrieve(query, 2).await?;
        assert!(hits[0].text.contains(expected), "{query}: {hits:?}");
        assert_eq!(hits[0].retrieval_kind, kind, "{query}");
        assert_eq!(
            &source_of(&sources, &hits[0]).excerpt[hits[0].start_byte..hits[0].end_byte],
            hits[0].text
        );
    }
    let program_documents: HashSet<String> = ["doc-ownership", "doc-plants"]
        .into_iter()
        .map(String::from)
        .collect();
    assert!(library
        .scopes()
        .iter()
        .all(|scope| *scope == program_documents));
    Ok(())
}

#[tokio::test]
async fn a_changed_or_importing_library_document_falls_back_to_keywords() -> Result<()> {
    let pool = tests::pool().await?;
    let (id, sources) = collection_fixture(&pool, ["doc-ownership", "doc-plants"]).await?;
    let mut library = FakeLibrary::default()
        .with_document(
            "doc-ownership",
            &["A rewritten chapter about something else."],
        )
        .with_document("doc-plants", &PLANTS);
    library.importing.insert("doc-plants".into());
    let collection = ReferenceCollection::load(&pool, &id, &sources, Some(&library)).await?;
    assert_eq!(collection.mode(), "lexical_fallback");
    assert_eq!(collection.embedding_model(), None);
    for query in ["borrowing", "photosynthesis"] {
        let hits = collection.retrieve(query, 2).await?;
        assert_eq!(hits[0].retrieval_kind, "lexical_fallback", "{query}");
        assert_eq!(
            &source_of(&sources, &hits[0]).excerpt[hits[0].start_byte..hits[0].end_byte],
            hits[0].text
        );
    }
    assert!(library.scopes().iter().all(HashSet::is_empty));
    Ok(())
}

#[tokio::test]
async fn without_a_library_every_snapshot_is_ranked_by_keyword() -> Result<()> {
    let pool = tests::pool().await?;
    let (id, sources) = collection_fixture(&pool, ["doc-ownership", "doc-plants"]).await?;
    let lexical = ReferenceCollection::load(&pool, &id, &sources, None).await?;
    assert!(lexical.retrieve("sunlight conversion", 3).await?.is_empty());
    let hits = lexical.retrieve("photosynthesis", 3).await?;
    assert!(!hits.is_empty());
    assert!(hits.iter().all(|h| h.retrieval_kind == "lexical_fallback"));
    assert_eq!(lexical.mode(), "lexical_fallback");
    Ok(())
}

#[test]
fn library_chunks_are_located_in_order_and_a_missing_chunk_rejects_the_snapshot() {
    let chunk = |id: &str, text: &str| LibraryChunk {
        id: id.into(),
        text: text.into(),
    };
    let text = "alpha beta\n\nalpha gamma";
    let located = locate_chunks(
        text,
        &[chunk("1", "alpha beta"), chunk("2", " alpha gamma ")],
    );
    assert_eq!(
        located,
        Some(vec![("1".into(), 0, 10), ("2".into(), 12, 23)])
    );
    assert_eq!(locate_chunks(text, &[chunk("1", "alpha delta")]), None);
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

#[test]
fn large_collection_keeps_evidence_after_the_former_twenty_million_character_boundary() -> Result<()>
{
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
    let last = collection.chunks.last().unwrap();
    assert!(collection.sources[0].excerpt[last.start..last.end].contains(suffix));
    assert!(last.terms.contains_key("nebular"));
    Ok(())
}

#[test]
fn unicode_passage_offsets_remain_lossless_without_a_whole_document_offset_array() {
    let text = "🧪é中sample\n".repeat(400);
    let actual = spans(&text);
    let mut boundaries: Vec<_> = text.char_indices().map(|(i, _)| i).collect();
    boundaries.push(text.len());
    let mut expected = Vec::new();
    let mut start = 0;
    while start + 1 < boundaries.len() {
        let end = (start + CHUNK_CHARS).min(boundaries.len() - 1);
        expected.push((boundaries[start], boundaries[end]));
        if end == boundaries.len() - 1 {
            break;
        }
        start += STEP_CHARS;
    }
    assert_eq!(actual, expected);
}

#[test]
fn deeper_evidence_must_mention_what_the_claim_is_about() -> Result<()> {
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
    // Shares only the subject's common words.
    assert!(!collection.mentions_subject_of(claim, "ASP.NET Core runs middleware in order."));
    // Mentions what the claim is about.
    assert!(collection.mentions_subject_of(claim, "Endpoint filters wrap a minimal API handler."));
    // A page sharing nothing with the claim stops the walk.
    assert!(!collection.mentions_subject_of("ASP.NET Core", "Vacuum reclaims storage."));
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

#[tokio::test]
async fn retrieved_passages_carry_their_branch_and_its_score() -> Result<()> {
    let pool = tests::pool().await?;
    let (id, sources) = collection_fixture(&pool, ["doc-ownership", "doc-plants"]).await?;
    let library = FakeLibrary::default()
        .with_document("doc-ownership", &OWNERSHIP)
        .with_document("doc-plants", &PLANTS);
    let collection = ReferenceCollection::load(&pool, &id, &sources, Some(&library)).await?;
    let library_hit = &collection.retrieve("borrowing", 1).await?[0];
    assert_eq!(library_hit.branch, Some(Branch::Library));
    assert_eq!(library_hit.branch_score, 1.0, "the library's own score");
    let keyword_hit = &collection.retrieve("pastry crust", 1).await?[0];
    assert_eq!(keyword_hit.branch, Some(Branch::Keyword));
    assert!(keyword_hit.branch_score > 0.0, "its BM25 score");
    // The position score is still the merged rank, whatever the branch.
    assert_eq!(library_hit.score, 1.0 / 61.0);
    assert_eq!(keyword_hit.score, 1.0 / 61.0);
    Ok(())
}

#[test]
fn a_merged_window_ranks_as_the_better_of_its_hits() -> Result<()> {
    let text = format!(
        "{}first matching fact\n{}second matching caveat\n{}",
        "préfixe ".repeat(400),
        "background ".repeat(250),
        "suffix ".repeat(400)
    );
    let collection = ReferenceCollection::lexical(&[LearningSourceDto {
        id: "reference".into(),
        title: "Procedure".into(),
        url: None,
        excerpt: text.clone(),
        acquired_at: 1,
    }])?;
    let hit = |target: &str, score: f64, branch_score: f64| {
        let start = text.find(target).unwrap();
        ReferencePassage {
            source_id: "reference".into(),
            text: target.into(),
            start_byte: start,
            end_byte: start + target.len(),
            retrieval_kind: "lexical_fallback".into(),
            score,
            branch: Some(Branch::Keyword),
            branch_score,
        }
    };
    let merged = collection.verification_context(vec![
        hit("first matching fact", 1.0 / 62.0, 3.0),
        hit("second matching caveat", 1.0 / 61.0, 5.0),
    ])?;
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].score, 1.0 / 61.0);
    assert_eq!(merged[0].branch_score, 5.0);
    Ok(())
}

#[test]
fn a_page_ends_after_the_largest_gap_in_its_scores() {
    // One drop larger than the rest of the descent together: a clear gap.
    let plain = [10.0, 9.5, 9.0, 2.0, 1.5];
    assert_eq!(pages::gap_cut(&plain), Some(3));
    assert!(pages::gap_is_clear(&plain));
    // A smooth descent still ends at its largest drop; the gap is not clear.
    let smooth = [10.0, 7.0, 5.0, 2.0];
    assert_eq!(pages::gap_cut(&smooth), Some(1));
    assert!(!pages::gap_is_clear(&smooth));
    // Equal drops and equal scores cut after the first.
    assert_eq!(pages::gap_cut(&[10.0, 8.0, 6.0, 4.0]), Some(1));
    assert_eq!(pages::gap_cut(&[3.0, 3.0, 3.0]), Some(1));
    // One candidate passes; of two, the first only.
    assert_eq!(pages::gap_cut(&[5.0]), None);
    assert_eq!(pages::gap_cut(&[5.0, 3.0]), Some(1));
    assert_eq!(pages::gap_cut(&[]), None);
    // A drop deep in a list whose head is flat is the knee.
    let knee = [5.0, 5.0, 5.0, 5.0, 5.0, 1.0, 0.9, 0.8];
    assert_eq!(pages::gap_cut(&knee), Some(5));
    assert!(pages::gap_is_clear(&knee));
}

#[test]
fn each_branch_is_cut_on_its_own_scores() {
    let candidate = |id: &str, branch: Branch, branch_score: f64| ReferencePassage {
        source_id: id.into(),
        text: id.into(),
        start_byte: 0,
        end_byte: id.len(),
        retrieval_kind: "hybrid".into(),
        score: 0.0,
        branch: Some(branch),
        branch_score,
    };
    let (above, gaps) = above_cut(vec![
        candidate("library-1", Branch::Library, 0.033),
        candidate("keyword-1", Branch::Keyword, 12.0),
        candidate("library-2", Branch::Library, 0.032),
        candidate("keyword-2", Branch::Keyword, 11.0),
        candidate("library-3", Branch::Library, 0.016),
        candidate("keyword-3", Branch::Keyword, 10.0),
        candidate("keyword-4", Branch::Keyword, 9.0),
    ]);
    let kept: Vec<&str> = above.iter().map(|p| p.source_id.as_str()).collect();
    assert_eq!(
        kept,
        [
            "library-1",
            "keyword-1",
            "library-2",
            "keyword-2",
            "keyword-3",
            "keyword-4"
        ]
    );
    assert_eq!(
        gaps,
        Gaps {
            library: Some(true),
            keyword: Some(false),
        }
    );
}

#[test]
fn a_counter_query_drops_the_claims_values_and_keeps_its_subject() -> Result<()> {
    let collection = ReferenceCollection::lexical(&[LearningSourceDto {
        id: "notes".into(),
        title: "Notes".into(),
        url: None,
        excerpt: "Quartz zeta resists acid etching for 40 hours in March near Lisbon. \
                  Quartz zeta does not resist acid etching."
            .into(),
        acquired_at: 1,
    }])?;
    let query = collection
        .counter_query("Quartz zeta resists acid etching for 40 hours in March near Lisbon.")
        .unwrap();
    let words: Vec<&str> = query.split(' ').collect();
    for value in ["40", "march", "lisbon"] {
        assert!(!words.contains(&value), "{query}");
    }
    for subject in ["quartz", "zeta", "resists", "acid", "etching"] {
        assert!(words.contains(&subject), "{query}");
    }
    // A negated claim asks for passages that assert the positive.
    let positive = collection
        .counter_query("Quartz zeta does not resist acid etching.")
        .unwrap();
    assert!(!positive.split(' ').any(|word| word == "not"), "{positive}");
    assert!(
        positive.split(' ').any(|word| word == "resist"),
        "{positive}"
    );
    // A claim with no value terms has no counter query: it would be the
    // support query.
    assert_eq!(
        collection.counter_query("Quartz zeta resists acid etching."),
        None
    );
    Ok(())
}
