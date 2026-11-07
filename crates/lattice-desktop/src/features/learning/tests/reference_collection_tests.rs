//! Stands in for `lattice_learning::lessons::reference_collection::tests`, which declared this test.

use lattice_core::shared::error::Result;
use lattice_learning::lessons::reference_collection::test_support::{collection_fixture, source_of};
use lattice_learning::reference_collection::ReferenceCollection;
use super::course_index::tests::indexes;
use super::pool;

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
async fn document_and_web_snapshots_rank_through_the_course_index() -> Result<()> {
    use lattice_learning::references::course_index::IndexMode;
    use crate::infrastructure::passage_index::testing::{FakeEmbedder, FixedEmbedders};
    let pool = pool().await?;
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
