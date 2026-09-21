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
