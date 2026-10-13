//! Tests that replacing a document's tags is atomic.

use super::*;

#[tokio::test]
async fn replacement_rolls_back_deletions_and_new_tags_when_an_insert_fails() {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    sqlx::query("INSERT INTO documents (id, file_path, file_name, mime_type, size_bytes, modified_at, indexed_at, checksum, status) VALUES ('doc', '/doc.txt', 'doc.txt', 'text/plain', 1, '2026-10-05', '2026-10-05', 'checksum', 'pending')").execute(&pool).await.unwrap();
    let repository = TagRepository::new(pool.clone());
    repository
        .replace_document_tags("doc", vec!["old".into()])
        .await
        .unwrap();
    sqlx::query("CREATE TRIGGER reject_broken_tag BEFORE INSERT ON document_tags WHEN NEW.tag_id = (SELECT id FROM tags WHERE name = 'broken') BEGIN SELECT RAISE(ABORT, 'injected failure'); END").execute(&pool).await.unwrap();
    assert!(repository
        .replace_document_tags("doc", vec!["new".into(), "broken".into()])
        .await
        .is_err());
    let tags = repository.get_tags_for_document("doc").await.unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name().as_str(), "old");
    assert!(repository.find_by_name("new").await.unwrap().is_none());
    assert!(repository
        .replace_document_tags("doc", vec![" ".into()])
        .await
        .is_err());
    assert_eq!(
        repository.get_tags_for_document("doc").await.unwrap().len(),
        1
    );
    repository
        .replace_document_tags("doc", vec![" Rust ".into(), "RUST".into()])
        .await
        .unwrap();
    let tags = repository.get_tags_for_document("doc").await.unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name().as_str(), "rust");
    repository
        .replace_document_tags("doc", vec![])
        .await
        .unwrap();
    assert!(repository
        .get_tags_for_document("doc")
        .await
        .unwrap()
        .is_empty());
}
