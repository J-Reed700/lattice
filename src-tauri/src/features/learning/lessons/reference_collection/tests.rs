#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
use super::*;
use crate::features::learning::{
    repository::LearningRepository, source_library::CapturedLearningSource,
    sources::InitialReference, tests,
};
use std::sync::atomic::{AtomicUsize, Ordering};

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

struct Embedder {
    identity: &'static str,
    calls: AtomicUsize,
    fail: bool,
}
#[async_trait::async_trait]
impl EmbeddingPort for Embedder {
    async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        if self.fail {
            return Err(invalid("Embedding fixture unavailable"));
        }
        // Deterministic semantics for plumbing tests, not a quality benchmark.
        let text = text.to_lowercase();
        Ok(vec![
            if text.contains("ownership") || text.contains("borrowing") {
                1.0
            } else {
                0.0
            },
            if text.contains("pastry") || text.contains("crust") {
                1.0
            } else {
                0.0
            },
            if text.contains("photosynthesis")
                || text.contains("chloroplast")
                || text.contains("sunlight conversion")
            {
                1.0
            } else {
                0.0
            },
            0.1,
        ])
    }
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let mut out = Vec::new();
        for text in texts {
            out.push(self.embed_single(text).await?);
        }
        Ok(out)
    }
    fn model_identity(&self) -> String {
        self.identity.into()
    }
    fn dimension(&self) -> usize {
        4
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}
fn embedder(identity: &'static str) -> Embedder {
    Embedder {
        identity,
        calls: AtomicUsize::new(0),
        fail: false,
    }
}

async fn collection_fixture(pool: &SqlitePool) -> Result<(String, Vec<LearningSourceDto>)> {
    let mut program = tests::fixture();
    let texts = [
        format!(
            "{}\n# Ownership\nBorrowing gives access without transferring ownership. 🦀\n",
            "Background without the target concept.\n".repeat(2200)
        ),
        "# Pastry\nKeep butter cold when making a flaky pie crust.".into(),
        "# Plant cells\nPhotosynthesis takes place in chloroplasts.".into(),
    ];
    program.sources = texts
        .iter()
        .enumerate()
        .map(|(i, text)| LearningSourceDto {
            id: uuid::Uuid::new_v4().to_string(),
            title: format!("Reference {i}"),
            url: (i == 0).then(|| "https://example.org/v1/guide".into()),
            excerpt: text.chars().take(2400).collect(),
            acquired_at: 1,
        })
        .collect();
    let references: Vec<_> = program
        .sources
        .iter()
        .zip(&texts)
        .map(|(s, text)| InitialReference {
            source_id: s.id.clone(),
            origin: s.id.clone(),
            captured: CapturedLearningSource {
                title: s.title.clone(),
                publisher: None,
                requested_url: s.url.as_ref().map(|_| "https://example.org/guide".into()),
                resolved_url: s.url.clone(),
                text: text.clone(),
                truncated: false,
                extraction_version: "fixture_full_text".into(),
            },
        })
        .collect();
    let repo = LearningRepository::new(pool.clone());
    repo.create_with_references(&program, &references).await?;
    let sources = repo.verification_sources(&program.summary.id).await?;
    Ok((program.summary.id, sources))
}

#[tokio::test]
async fn whole_sources_and_three_subjects_use_the_same_hybrid_path() -> Result<()> {
    let pool = tests::pool().await?;
    let (id, sources) = collection_fixture(&pool).await?;
    assert!(sources.iter().any(|source| source.excerpt.len() > 64_000));
    let urls = sqlx::query("SELECT requested_url,resolved_url FROM learning_source_versions WHERE program_id=? AND title='Reference 0'").bind(&id).fetch_one(&pool).await.map_err(db)?;
    assert_eq!(
        urls.get::<String, _>("requested_url"),
        "https://example.org/guide"
    );
    assert_eq!(
        urls.get::<String, _>("resolved_url"),
        "https://example.org/v1/guide"
    );
    let embedding = embedder("fixture-v1");
    let collection = ReferenceCollection::load(&pool, &id, &sources, Some(&embedding)).await?;
    for (query, expected) in [
        ("ownership borrowing", "Borrowing gives access"),
        ("pastry crust", "Keep butter cold"),
        ("photosynthesis", "chloroplasts"),
        ("sunlight conversion", "chloroplasts"),
    ] {
        let hits = collection.retrieve(query, 2).await?;
        assert!(hits[0].text.contains(expected), "{query}: {hits:?}");
        assert_eq!(hits[0].retrieval_kind, "hybrid");
        let original = sources.iter().find(|s| s.id == hits[0].source_id).unwrap();
        assert_eq!(
            &original.excerpt[hits[0].start_byte..hits[0].end_byte],
            hits[0].text
        );
    }
    let calls = embedding.calls.load(Ordering::SeqCst);
    ReferenceCollection::load(&pool, &id, &sources, Some(&embedding)).await?;
    assert_eq!(calls, embedding.calls.load(Ordering::SeqCst));
    let switched = embedder("fixture-v2");
    ReferenceCollection::load(&pool, &id, &sources, Some(&switched)).await?;
    assert!(switched.calls.load(Ordering::SeqCst) > 0);
    Ok(())
}

#[tokio::test]
async fn fallback_scope_and_interrupted_index_are_explicit() -> Result<()> {
    let pool = tests::pool().await?;
    let (id, sources) = collection_fixture(&pool).await?;
    let (other_id, other_sources) = collection_fixture(&pool).await?;
    let embedding = embedder("fixture-v1");
    ReferenceCollection::load(&pool, &other_id, &other_sources, Some(&embedding)).await?;
    let lexical = ReferenceCollection::load(&pool, &id, &sources, None).await?;
    assert!(lexical.retrieve("sunlight conversion", 3).await?.is_empty());
    let hits = lexical.retrieve("photosynthesis", 3).await?;
    assert!(!hits.is_empty());
    assert!(hits
        .iter()
        .all(|h| sources.iter().any(|s| s.id == h.source_id)));
    assert!(hits.iter().all(|h| h.retrieval_kind == "lexical_fallback"));
    let failing = Embedder {
        fail: true,
        ..embedder("fixture-v1")
    };
    assert!(
        ReferenceCollection::load(&pool, &id, &sources, Some(&failing))
            .await
            .is_err()
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM learning_source_retrieval_index WHERE program_id=?",
    )
    .bind(&id)
    .fetch_one(&pool)
    .await
    .map_err(db)?;
    assert_eq!(count, 0);
    ReferenceCollection::load(&pool, &id, &sources, Some(&embedding)).await?;
    // A partial/corrupt cache is rebuilt, not treated as a finished version.
    sqlx::query(
        "DELETE FROM learning_source_retrieval_index WHERE program_id=? AND chunk_ordinal=0",
    )
    .bind(&id)
    .execute(&pool)
    .await
    .map_err(db)?;
    let before = embedding.calls.load(Ordering::SeqCst);
    ReferenceCollection::load(&pool, &id, &sources, Some(&embedding)).await?;
    assert!(embedding.calls.load(Ordering::SeqCst) > before);
    Ok(())
}

#[tokio::test]
#[ignore = "Downloads the public Rust Book through the production safe web reader"]
async fn live_rust_book_is_captured_beyond_chat_limit() -> Result<()> {
    use crate::features::web::traits::WebServiceTrait;
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
