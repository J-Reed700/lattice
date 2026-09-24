//! Document references linked to a conversation.

use super::*;
use crate::features::conversation::repository::ConversationRepository;

#[tokio::test]
async fn test_add_document_reference() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    seed_documents(&pool, &["doc-123"]).await;
    let repo = ConversationRepository::new(pool);

    let conversation = repo
        .create_conversation("Test", "model", None)
        .await
        .unwrap();

    repo.add_document_reference(
        &conversation.id.to_string(),
        "doc-123",
        Some("chunk-456"),
        Some(0.95),
    )
    .await
    .unwrap();

    let refs = repo
        .get_document_references(&conversation.id.to_string())
        .await
        .unwrap();

    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].document_id, "doc-123");
    assert_eq!(refs[0].chunk_id, Some("chunk-456".to_string()));
    assert_eq!(refs[0].relevance_score, Some(0.95));
}

/// A passage a later turn finds again belongs to that later turn: the
/// follow-up picker reads the newest turn's references, and a chunk stuck at
/// its first-seen time would drop out of it.
#[tokio::test]
async fn referencing_a_chunk_again_moves_it_to_the_latest_turn() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    seed_documents(&pool, &["doc-a", "doc-b"]).await;
    let repo = ConversationRepository::new(pool);
    let id = repo
        .create_conversation("Test", "model", None)
        .await
        .unwrap()
        .id
        .to_string();

    repo.add_document_reference(&id, "doc-a", Some("a1"), Some(0.9))
        .await
        .unwrap();
    repo.add_document_reference(&id, "doc-b", Some("b1"), Some(0.5))
        .await
        .unwrap();
    repo.add_document_reference(&id, "doc-a", Some("a1"), None)
        .await
        .unwrap();

    let refs = repo.get_document_references(&id).await.unwrap();
    assert_eq!(refs.len(), 2);
    assert_eq!(refs[1].chunk_id.as_deref(), Some("a1"));
    assert_eq!(refs[1].relevance_score, Some(0.9));
}
