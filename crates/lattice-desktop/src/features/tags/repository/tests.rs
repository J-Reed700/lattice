//! Tests for tag CRUD, batches and filters against an in-memory SQLite.

use super::*;
use sqlx::sqlite::SqlitePoolOptions;

async fn create_test_pool() -> SqlitePool {
    SqlitePoolOptions::new().connect(":memory:").await.unwrap()
}

async fn setup_schema(pool: &SqlitePool) {
    sqlx::query(
        r#"
            CREATE TABLE tags (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                color TEXT NOT NULL DEFAULT '#6366f1',
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )
            "#,
    )
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn untagged_batches_filter_and_limit_before_loading_documents() {
    let pool = create_test_pool().await;
    sqlx::raw_sql(
        "CREATE TABLE documents (id TEXT PRIMARY KEY,indexed_at TEXT);
         CREATE TABLE document_tags (document_id TEXT,tag_id TEXT);
         INSERT INTO documents VALUES ('tagged','2026-10-24'),('old','2026-10-20'),
            ('new','2026-10-23'),('tie','2026-10-23');
         INSERT INTO document_tags VALUES ('tagged','a'),('tagged','b');",
    )
    .execute(&pool)
    .await
    .unwrap();
    let repo = TagRepository::new(pool);
    let (ids, reads) =
        crate::tests::common::query_counts::count_reads(repo.untagged_document_ids(1)).await;
    assert_eq!(ids.unwrap(), ["new"]);
    assert_eq!(reads, 1);
    assert_eq!(
        repo.untagged_document_ids(usize::MAX).await.unwrap(),
        ["new", "tie", "old"]
    );
    let (empty, reads) =
        crate::tests::common::query_counts::count_reads(repo.untagged_document_ids(0)).await;
    assert!(empty.unwrap().is_empty());
    assert_eq!(reads, 0);
}

#[tokio::test]
async fn test_save_and_find_by_id() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let repo = TagRepository::new(pool);

    let name = TagName::new("rust".to_string()).unwrap();
    let entity = TagEntity::new(name, "#ff5733".to_string());

    repo.save(&entity).await.unwrap();

    let found = repo.find_by_id(entity.id().as_str()).await.unwrap();
    assert!(found.is_some());

    let found_entity = found.unwrap();
    assert_eq!(found_entity.id().as_str(), entity.id().as_str());
    assert_eq!(found_entity.name().as_str(), "rust");
    assert_eq!(found_entity.color(), "#ff5733");
}

#[tokio::test]
async fn test_find_by_name() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let repo = TagRepository::new(pool);

    let name = TagName::new("python".to_string()).unwrap();
    let entity = TagEntity::new(name, "#6366f1".to_string());

    repo.save(&entity).await.unwrap();

    let found = repo.find_by_name("python").await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().name().as_str(), "python");

    // Case-insensitive search
    let found = repo.find_by_name("PYTHON").await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().name().as_str(), "python");
}

#[tokio::test]
async fn test_get_or_create() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let repo = TagRepository::new(pool);

    // First call creates the tag
    let tag1 = repo.get_or_create("rust", Some("#ff5733")).await.unwrap();
    assert_eq!(tag1.name().as_str(), "rust");
    assert_eq!(repo.count().await.unwrap(), 1);

    // Second call returns existing tag
    let tag2 = repo.get_or_create("rust", Some("#ff5733")).await.unwrap();
    assert_eq!(tag2.id().as_str(), tag1.id().as_str());
    assert_eq!(repo.count().await.unwrap(), 1);
}

#[tokio::test]
async fn test_find_all() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let repo = TagRepository::new(pool);

    let tag1 = TagEntity::new(
        TagName::new("rust".to_string()).unwrap(),
        "#ff5733".to_string(),
    );
    let tag2 = TagEntity::new(
        TagName::new("python".to_string()).unwrap(),
        "#6366f1".to_string(),
    );

    repo.save(&tag1).await.unwrap();
    repo.save(&tag2).await.unwrap();

    let all_tags = repo.find_all().await.unwrap();
    assert_eq!(all_tags.len(), 2);
    assert_eq!(all_tags[0].name().as_str(), "python");
    assert_eq!(all_tags[1].name().as_str(), "rust");
}

#[tokio::test]
async fn test_save_batch() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let repo = TagRepository::new(pool);

    let entities = vec![
        TagEntity::new(
            TagName::new("tag1".to_string()).unwrap(),
            "#ff0000".to_string(),
        ),
        TagEntity::new(
            TagName::new("tag2".to_string()).unwrap(),
            "#00ff00".to_string(),
        ),
        TagEntity::new(
            TagName::new("tag3".to_string()).unwrap(),
            "#0000ff".to_string(),
        ),
    ];

    repo.save_batch(&entities).await.unwrap();

    let count = repo.count().await.unwrap();
    assert_eq!(count, 3);
}

#[tokio::test]
async fn test_delete() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let repo = TagRepository::new(pool);

    let entity = TagEntity::new(
        TagName::new("rust".to_string()).unwrap(),
        "#ff5733".to_string(),
    );

    repo.save(&entity).await.unwrap();
    assert_eq!(repo.count().await.unwrap(), 1);

    repo.delete(entity.id().as_str()).await.unwrap();
    assert_eq!(repo.count().await.unwrap(), 0);
}

#[tokio::test]
async fn test_delete_batch() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let repo = TagRepository::new(pool);

    let entities = vec![
        TagEntity::new(
            TagName::new("tag1".to_string()).unwrap(),
            "#ff0000".to_string(),
        ),
        TagEntity::new(
            TagName::new("tag2".to_string()).unwrap(),
            "#00ff00".to_string(),
        ),
    ];

    repo.save_batch(&entities).await.unwrap();
    assert_eq!(repo.count().await.unwrap(), 2);

    let ids: Vec<&str> = entities.iter().map(|e| e.id().as_str()).collect();
    repo.delete_batch(&ids).await.unwrap();
    assert_eq!(repo.count().await.unwrap(), 0);
}

#[tokio::test]
async fn test_exists() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let repo = TagRepository::new(pool);

    let entity = TagEntity::new(
        TagName::new("rust".to_string()).unwrap(),
        "#ff5733".to_string(),
    );

    assert!(!repo.exists(entity.id().as_str()).await.unwrap());

    repo.save(&entity).await.unwrap();

    assert!(repo.exists(entity.id().as_str()).await.unwrap());
}

#[tokio::test]
async fn test_find_by_filter() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let repo = TagRepository::new(pool);

    let entities = vec![
        TagEntity::new(
            TagName::new("rust".to_string()).unwrap(),
            "#ff0000".to_string(),
        ),
        TagEntity::new(
            TagName::new("python".to_string()).unwrap(),
            "#00ff00".to_string(),
        ),
        TagEntity::new(
            TagName::new("ruby".to_string()).unwrap(),
            "#0000ff".to_string(),
        ),
    ];

    repo.save_batch(&entities).await.unwrap();

    let filter = TagFilter {
        name_pattern: Some("ru".to_string()),
        limit: None,
    };

    let results = repo.find_by_filter(&filter).await.unwrap();
    assert_eq!(results.len(), 2); // rust and ruby
    assert!(results.iter().any(|t| t.name().as_str() == "rust"));
    assert!(results.iter().any(|t| t.name().as_str() == "ruby"));
}
