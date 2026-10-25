use super::*;
use crate::tests::common::query_counts::count_reads;

async fn fixture() -> ConversationRepository {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::raw_sql(
        "CREATE TABLE documents (id TEXT PRIMARY KEY, file_name TEXT, file_path TEXT, source_context TEXT);
         CREATE TABLE text_chunks (id TEXT PRIMARY KEY, document_id TEXT, content TEXT,
            chunk_index INTEGER, section TEXT, page_number INTEGER, from_ocr INTEGER);
         CREATE INDEX chunks_order ON text_chunks(document_id,chunk_index);
         INSERT INTO documents VALUES ('first','First','/first',NULL),
            ('second','Second','/second',NULL),('foreign','Private','/foreign',NULL),
            ('empty','Empty','/empty',NULL);
         INSERT INTO text_chunks VALUES ('secret','foreign','Private text',0,'Private',1,0);",
    ).execute(&pool).await.unwrap();
    for document in ["first", "second"] {
        for index in 0..28 {
            sqlx::query("INSERT INTO text_chunks VALUES (?,?,?,?,?,?,?)")
                .bind(format!("{document}-{index}"))
                .bind(document)
                .bind(format!("Opening {document} {index}"))
                // Sparse indices catch implementations that confuse LIMIT K
                // with chunk_index < K.
                .bind(index * 7)
                .bind(format!("Section {index}"))
                .bind(index + 1)
                .bind(index == 0)
                .execute(&pool)
                .await
                .unwrap();
        }
    }
    ConversationRepository::new(pool)
}

#[tokio::test]
async fn catalog_batches_headings_caps_each_document_and_accepts_large_scopes() {
    let repo = fixture().await;
    let mut allowed: HashSet<String> = (0..33_000).map(|id| format!("missing-{id}")).collect();
    allowed.extend(["first".into(), "second".into(), "empty".into()]);
    let (catalog, reads) = count_reads(repo.retrieval_catalog(&allowed)).await;
    let catalog = catalog.unwrap();
    assert_eq!(reads, 1);
    assert_eq!(
        catalog
            .iter()
            .map(|doc| doc.id.as_str())
            .collect::<Vec<_>>(),
        ["empty", "first", "second"]
    );
    assert!(catalog[0].sections.is_empty());
    assert_eq!(catalog[0].opening, "");
    for doc in &catalog[1..] {
        assert_eq!(
            doc.sections,
            (0..24).map(|i| format!("Section {i}")).collect::<Vec<_>>()
        );
        assert_eq!(doc.opening, format!("Opening {} 0", doc.id));
    }
    let (empty, reads) = count_reads(repo.retrieval_catalog(&HashSet::new())).await;
    assert!(empty.unwrap().is_empty());
    assert_eq!(reads, 0);
}

#[tokio::test]
async fn openings_batch_without_losing_selection_order_scope_or_sparse_indices() {
    let repo = fixture().await;
    let allowed = [
        "first".into(),
        "second".into(),
        "empty".into(),
        "missing".into(),
    ]
    .into();
    let selected = ["second", "first", "second", "foreign", "empty", "missing"].map(str::to_owned);
    let (passages, reads) = count_reads(repo.retrieval_openings(&selected, &allowed, 2)).await;
    let passages = passages.unwrap();
    assert_eq!(reads, 1);
    assert_eq!(
        passages.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
        ["second-0", "second-1", "first-0", "first-1"]
    );
    assert_eq!(passages[1].chunk_index, 7);
    assert_eq!(passages[0].page_number, Some(1));
    assert!(passages[0].from_ocr);
    assert!(!passages[1].from_ocr);
    let (empty, reads) = count_reads(repo.retrieval_openings(&selected, &allowed, 0)).await;
    assert!(empty.unwrap().is_empty());
    assert_eq!(reads, 0);
}

#[tokio::test]
async fn section_lookup_accepts_large_scopes_without_widening_them() {
    let repo = fixture().await;
    sqlx::query(
        "UPDATE text_chunks SET section='Section 1.2' WHERE id='first-1' OR document_id='foreign'",
    )
    .execute(&repo.pool)
    .await
    .unwrap();
    let mut allowed: HashSet<String> = (0..33_000).map(|id| format!("missing-{id}")).collect();
    allowed.insert("first".into());
    let (passages, reads) =
        count_reads(repo.retrieval_section_passages(&["1.2".into()], &allowed)).await;
    let passages = passages.unwrap();
    assert_eq!(reads, 1);
    assert_eq!(passages.len(), 1);
    assert_eq!(passages[0].id, "first-1");
    assert_eq!(passages[0].page_number, Some(2));
    let (empty, reads) =
        count_reads(repo.retrieval_section_passages(&["1.2".into()], &HashSet::new())).await;
    assert!(empty.unwrap().is_empty());
    assert_eq!(reads, 0);
}
