use super::implementation::SqliteDocumentRepository as DocumentRepository;
use crate::shared::error::AppError;

use crate::application::ports::DocumentRepositoryPort;
use crate::application::ports::RepositoryPort;
use sqlx::sqlite::SqlitePoolOptions;

async fn repository() -> DocumentRepository {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    DocumentRepository::new(pool)
}

/// Insert a row directly: these queries read one column and must not
/// depend on chunks existing.
async fn insert(repo: &DocumentRepository, id: &str, path: &str, checksum: &str) {
    sqlx::query(
        "INSERT INTO documents \
             (id, file_path, file_name, file_type, mime_type, size_bytes, \
              modified_at, indexed_at, checksum, status) \
             VALUES (?1, ?2, ?3, 'txt', 'text/plain', 4, '2026-09-25T00:00:00Z', \
                     '2026-09-25T00:00:00Z', ?4, 'indexed')",
    )
    .bind(id)
    .bind(path)
    .bind(id)
    .bind(checksum)
    .execute(&repo.pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn count_by_checksum_counts_every_row_that_shares_the_content() {
    let repo = repository().await;
    let shared = "a".repeat(64);
    let lonely = "b".repeat(64);

    insert(&repo, "doc-1", "/lib/a/one.txt", &shared).await;
    insert(&repo, "doc-2", "/lib/a/two.txt", &shared).await;
    insert(&repo, "doc-3", "/lib/b/three.txt", &lonely).await;

    assert_eq!(repo.count_by_checksum(&shared).await.unwrap(), 2);
    assert_eq!(repo.count_by_checksum(&lonely).await.unwrap(), 1);
    assert_eq!(repo.count_by_checksum(&"c".repeat(64)).await.unwrap(), 0);
}

#[tokio::test]
async fn find_metadata_by_ids_reads_only_the_asked_rows() {
    let repo = repository().await;
    insert(&repo, "doc-1", "/lib/a/one.txt", &"a".repeat(64)).await;
    insert(&repo, "doc-2", "/lib/b/two.txt", &"b".repeat(64)).await;
    insert(&repo, "doc-3", "/lib/c/three.txt", &"c".repeat(64)).await;

    let mut found: Vec<String> = repo
        .find_metadata_by_ids(&["doc-3".into(), "doc-1".into(), "missing".into()])
        .await
        .unwrap()
        .into_iter()
        .map(|doc| doc.id().as_str().to_owned())
        .collect();
    found.sort();

    assert_eq!(found, vec!["doc-1", "doc-3"]);
    assert!(repo.find_metadata_by_ids(&[]).await.unwrap().is_empty());
}

#[tokio::test]
async fn count_by_checksum_of_an_empty_table_is_zero() {
    let repo = repository().await;

    assert_eq!(repo.count_by_checksum(&"a".repeat(64)).await.unwrap(), 0);
}

#[tokio::test]
async fn list_checksums_is_the_distinct_referenced_set() {
    let repo = repository().await;
    let shared = "a".repeat(64);
    let lonely = "b".repeat(64);

    insert(&repo, "doc-1", "/lib/a/one.txt", &shared).await;
    insert(&repo, "doc-2", "/lib/a/two.txt", &shared).await;
    insert(&repo, "doc-3", "/lib/b/three.txt", &lonely).await;
    // A row with no checksum must not put an empty string in the set:
    // the sweep would then treat "" as a referenced blob name.
    insert(&repo, "doc-4", "/lib/c/four.txt", "").await;

    let mut checksums = repo.list_checksums().await.unwrap();
    checksums.sort();

    assert_eq!(checksums, vec![shared, lonely]);
}

#[tokio::test]
async fn list_checksums_of_an_empty_table_is_empty() {
    let repo = repository().await;

    assert!(repo.list_checksums().await.unwrap().is_empty());
}

#[tokio::test]
async fn find_checksum_by_id_reads_one_column() {
    let repo = repository().await;
    let checksum = "d".repeat(64);
    insert(&repo, "doc-1", "/lib/d/one.txt", &checksum).await;

    assert_eq!(
        repo.find_checksum_by_id("doc-1").await.unwrap(),
        checksum,
        "a document with no chunks still yields its checksum"
    );
    assert!(matches!(
        repo.find_checksum_by_id("missing").await,
        Err(AppError::NotFound(_))
    ));
}

#[tokio::test]
async fn deleting_a_document_drops_it_from_the_referenced_set() {
    let repo = repository().await;
    let checksum = "e".repeat(64);
    insert(&repo, "doc-1", "/lib/e/one.txt", &checksum).await;
    insert(&repo, "doc-2", "/lib/e/two.txt", &checksum).await;

    DocumentRepositoryPort::delete(&repo, "doc-1")
        .await
        .unwrap();
    assert_eq!(repo.count_by_checksum(&checksum).await.unwrap(), 1);
    assert_eq!(repo.list_checksums().await.unwrap(), vec![checksum.clone()]);

    DocumentRepositoryPort::delete(&repo, "doc-2")
        .await
        .unwrap();
    assert_eq!(repo.count_by_checksum(&checksum).await.unwrap(), 0);
    assert!(repo.list_checksums().await.unwrap().is_empty());
}

async fn aggregate(pool: &sqlx::SqlitePool, name: &str) -> crate::domain::entities::Document {
    use crate::domain::{
        entities::{chunk::Chunk, Document},
        value_objects::{
            source_context::{SourceContext, SourceGroup, StructureMode},
            Checksum,
        },
    };
    let metadata = Document::new(
        crate::shared::types::ValidatedFilePath::new(std::path::PathBuf::from(format!(
            "/{name}.txt"
        )))
        .unwrap(),
        name.into(),
        "text/plain".into(),
        12,
        Checksum::new("a".repeat(64)).unwrap(),
    );
    let chunk = Chunk::new(metadata.id().clone(), "durable chunk".into(), 0);
    let tag = crate::features::tags::repository::TagRepository::new(pool.clone())
        .get_or_create("contract", None)
        .await
        .unwrap();
    let mut document = metadata
        .from_parts(vec![chunk], vec![tag.id().clone()])
        .unwrap();
    document.set_source_context(Some(SourceContext {
        group: SourceGroup {
            id: uuid::Uuid::new_v4().to_string(),
            title: "Sources".into(),
            edition: None,
            description: None,
            ordered: true,
            structure: StructureMode::Sections,
        },
        position: 2,
    }));
    document
}

async fn assert_contract(
    repository: &dyn DocumentRepositoryPort,
    document: &crate::domain::entities::Document,
) {
    repository
        .save_batch(std::slice::from_ref(document))
        .await
        .unwrap();
    let loaded = repository
        .find_by_id(document.id().as_str())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded.chunks().len(), 1);
    assert_eq!(loaded.chunks()[0].content(), "durable chunk");
    assert_eq!(loaded.tags(), document.tags());
    assert_eq!(loaded.source_context(), document.source_context());
    repository.save(&loaded).await.unwrap();
    let filter = crate::application::ports::DocumentFilter {
        path_pattern: None,
        status: None,
        limit: Some(10),
    };
    let filtered = repository.find_by_filter(&filter).await.unwrap();
    assert_eq!(filtered[0].chunks().len(), 1);
    assert_eq!(filtered[0].source_context(), document.source_context());
    repository
        .rename(document.id().as_str(), "Renamed")
        .await
        .unwrap();
    assert_eq!(
        repository
            .find_by_id(document.id().as_str())
            .await
            .unwrap()
            .unwrap()
            .chunks()
            .len(),
        1
    );
    assert!(repository.list_metadata().await.unwrap()[0]
        .chunks()
        .is_empty());
}

#[tokio::test]
async fn standalone_and_transactional_adapters_share_the_aggregate_contract() {
    let repository = repository().await;
    let document = aggregate(&repository.pool, "standalone").await;
    assert_contract(&repository, &document).await;
    let pool = repository.pool.clone();
    let transaction = std::sync::Arc::new(tokio::sync::Mutex::new(pool.begin().await.unwrap()));
    let adapter = super::SqliteDocumentRepositoryTx::new(transaction.clone());
    assert_contract(&adapter, &document).await;
    drop(adapter);
    std::sync::Arc::try_unwrap(transaction)
        .unwrap()
        .into_inner()
        .commit()
        .await
        .unwrap();
}

#[tokio::test]
async fn failed_aggregate_save_preserves_existing_children() {
    let repository = repository().await;
    let document = aggregate(&repository.pool, "rollback").await;
    repository.save(&document).await.unwrap();
    sqlx::query("CREATE TRIGGER reject_chunk BEFORE INSERT ON text_chunks BEGIN SELECT RAISE(ABORT, 'injected child failure'); END").execute(&repository.pool).await.unwrap();
    assert!(repository.save(&document).await.is_err());
    let saved = repository
        .find_by_id(document.id().as_str())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(saved.chunks().len(), 1);
    assert_eq!(saved.tags(), document.tags());
}
