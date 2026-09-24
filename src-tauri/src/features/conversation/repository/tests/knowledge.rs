use super::*;
use crate::features::conversation::knowledge_dto::KnowledgeRequestDto;
use crate::features::conversation::repository::ConversationRepository;

async fn setup() -> (SqlitePool, ConversationRepository) {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    sqlx::query("INSERT INTO conversation_spaces(id,name) VALUES ('other','Other')")
        .execute(&pool)
        .await
        .unwrap();
    for (id, space) in [
        ("one", "space_general"),
        ("two", "space_general"),
        ("three", "other"),
    ] {
        sqlx::query(
            "INSERT INTO conversations(id,title,model_name,space_id) VALUES (?,?, 'test',?)",
        )
        .bind(id)
        .bind(id)
        .bind(space)
        .execute(&pool)
        .await
        .unwrap();
    }
    (pool.clone(), ConversationRepository::new(pool))
}
fn request(conversation: &str, action: &str) -> KnowledgeRequestDto {
    KnowledgeRequestDto {
        conversation_id: conversation.into(),
        action: action.into(),
        item_id: None,
        text: None,
        scope: None,
        kind: None,
        valid_from: None,
        valid_until: None,
        offset: None,
    }
}
async fn remember(repo: &ConversationRepository, scope: &str) -> String {
    let mut r = request("one", "remember");
    r.text = Some("The project budget is $120.".into());
    r.scope = Some(scope.into());
    repo.knowledge(&r).await.unwrap().items[0].id.clone()
}
#[tokio::test]
async fn explicit_scope_is_enforced_and_shared_reads_do_not_grant_edits() {
    let (_, repo) = setup().await;
    let id = remember(&repo, "conversation").await;
    assert!(repo
        .list_knowledge("two", 0, true)
        .await
        .unwrap()
        .items
        .is_empty());
    let mut r = request("one", "scope");
    r.item_id = Some(id.clone());
    r.scope = Some("space".into());
    repo.knowledge(&r).await.unwrap();
    assert_eq!(
        repo.list_knowledge("two", 0, true)
            .await
            .unwrap()
            .items
            .len(),
        1
    );
    assert!(repo
        .list_knowledge("three", 0, true)
        .await
        .unwrap()
        .items
        .is_empty());
    r.conversation_id = "two".into();
    r.action = "forget".into();
    assert!(repo.knowledge(&r).await.is_err());
    r.conversation_id = "one".into();
    r.action = "scope".into();
    r.scope = Some("personal".into());
    repo.knowledge(&r).await.unwrap();
    assert_eq!(
        repo.list_knowledge("three", 0, true)
            .await
            .unwrap()
            .items
            .len(),
        1
    );
}
#[tokio::test]
async fn corrections_preserve_exact_evidence_and_inherit_scope() {
    let (_, repo) = setup().await;
    let old = remember(&repo, "personal").await;
    let mut r = request("one", "correct");
    r.item_id = Some(old.clone());
    r.text = Some("Correction: the budget is $240, including tax.".into());
    repo.knowledge(&r).await.unwrap();
    let visible = repo.list_knowledge("two", 0, true).await.unwrap();
    assert_eq!(visible.items.len(), 1);
    assert_ne!(visible.items[0].id, old);
    assert_eq!(visible.items[0].scope, "personal");
    assert_eq!(visible.items[0].evidence[0].text, r.text);
    let history = repo.list_knowledge("one", 0, false).await.unwrap();
    assert!(history
        .items
        .iter()
        .any(|i| i.id == old && i.state == "superseded"));
    assert!(
        repo.knowledge(&r).await.is_err(),
        "cannot correct an already superseded item"
    );
}
#[tokio::test]
async fn time_bounds_forgetting_and_source_deletion_remove_prompt_evidence() {
    let (pool, repo) = setup().await;
    let id = remember(&repo, "personal").await;
    let mut r = request("one", "dates");
    r.item_id = Some(id.clone());
    r.valid_until = Some("2020-01-01T00:00:00Z".into());
    repo.knowledge(&r).await.unwrap();
    assert!(repo
        .prepare_knowledge("two", "budget")
        .await
        .unwrap()
        .optional
        .is_empty());
    r.valid_until = None;
    r.valid_from = Some("2999-01-01T00:00:00Z".into());
    repo.knowledge(&r).await.unwrap();
    assert!(repo
        .prepare_knowledge("two", "budget")
        .await
        .unwrap()
        .optional
        .is_empty());
    r.valid_from = None;
    repo.knowledge(&r).await.unwrap();
    assert_eq!(
        repo.prepare_knowledge("two", "budget")
            .await
            .unwrap()
            .optional
            .len(),
        1
    );
    r.action = "forget".into();
    repo.knowledge(&r).await.unwrap();
    assert!(repo
        .prepare_knowledge("one", "budget")
        .await
        .unwrap()
        .optional
        .is_empty());
    let id = remember(&repo, "personal").await;
    sqlx::query("DELETE FROM conversation_messages WHERE id IN (SELECT message_id FROM conversation_memory_evidence WHERE item_id=?)").bind(id).execute(&pool).await.unwrap();
    assert!(repo
        .prepare_knowledge("two", "budget")
        .await
        .unwrap()
        .optional
        .is_empty());
}
#[tokio::test]
async fn old_requirements_are_resolved_without_recent_source_messages() {
    let (_, repo) = setup().await;
    let mut r = request("one", "remember");
    r.kind = Some("constraint".into());
    r.text = Some("Never deploy without my approval.".into());
    r.scope = Some("space".into());
    repo.knowledge(&r).await.unwrap();
    let memory = repo.prepare_knowledge("two", "What next?").await.unwrap();
    assert_eq!(memory.mandatory.len(), 1);
    assert!(memory.mandatory[0].contains("Never deploy without my approval."));
    assert!(memory.mandatory[0].contains("source_conversation"));
}
#[tokio::test]
async fn invalid_dates_and_missing_evidence_fail_without_mutation() {
    let (pool, repo) = setup().await;
    let mut r = request("one", "remember");
    r.text = Some("fact".into());
    r.valid_from = Some("2026-02-01T00:00:00Z".into());
    r.valid_until = Some("2026-01-01T00:00:00Z".into());
    assert!(repo.knowledge(&r).await.is_err());
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM conversation_memory_items")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    r.valid_from = None;
    r.valid_until = None;
    r.text = None;
    assert!(repo.knowledge(&r).await.is_err());
}

struct Embedder;
#[async_trait::async_trait]
impl crate::application::ports::EmbeddingPort for Embedder {
    async fn embed_single(&self, _: &str) -> crate::shared::error::Result<Vec<f32>> {
        Ok(vec![1.0, 0.0])
    }
    async fn embed_batch(&self, texts: &[String]) -> crate::shared::error::Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| vec![1.0, 0.0]).collect())
    }
    fn dimension(&self) -> usize {
        2
    }
    fn model_identity(&self) -> String {
        "test-space".into()
    }
    async fn is_ready(&self) -> crate::shared::error::Result<bool> {
        Ok(true)
    }
}
#[tokio::test]
async fn semantic_recall_finds_paraphrases_but_never_foreign_or_incompatible_vectors() {
    use crate::application::ports::conversation_memory::ConversationMemoryReadPort;
    let (pool, repo) = setup().await;
    for (id, conversation, model, text) in [
        ("dog", "one", "test-space", "The dog likes the park."),
        ("foreign", "two", "test-space", "Private puppy details."),
        ("incompatible", "one", "other-model", "The hound is asleep."),
    ] {
        seed_message(
            &pool,
            conversation,
            id,
            "user",
            text,
            10,
            "2026-01-01T00:00:00Z",
        )
        .await;
        sqlx::query("INSERT INTO conversation_memory_vectors(id,conversation_id,message_id,role,content,embedding,dimension,embedding_model) VALUES (?,?,?,'user',?,?,2,?)")
            .bind(id).bind(conversation).bind(id).bind(text).bind(crate::features::embedding::encoding::encode_embedding(&[1.0,0.0])).bind(model).execute(&pool).await.unwrap();
    }
    let repo = repo.with_memory_embedding(Some(std::sync::Arc::new(Embedder)));
    let found = repo
        .search_source_messages("one", "canine", &[], 10)
        .await
        .unwrap();
    assert!(found.semantic_ran);
    assert_eq!(found.candidates.len(), 1);
    assert_eq!(found.candidates[0].message_id, "dog");
    assert_eq!(found.candidates[0].excerpt, "The dog likes the park.");
    sqlx::query("UPDATE conversation_messages SET content='Removed' WHERE id='dog'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(repo
        .search_source_messages("one", "canine", &[], 10)
        .await
        .unwrap()
        .candidates
        .is_empty());
}

#[tokio::test]
async fn forgetting_survives_reconstruction_of_the_same_source_assertion() {
    let (pool, repo) = setup().await;
    let id = remember(&repo, "personal").await;
    let mut r = request("one", "forget");
    r.item_id = Some(id.clone());
    repo.knowledge(&r).await.unwrap();
    sqlx::query("INSERT INTO conversation_memory_items(id,conversation_id,kind,label,created_at_sequence,changed_at_sequence) SELECT 'rebuilt',conversation_id,kind,label,created_at_sequence,changed_at_sequence FROM conversation_memory_items WHERE id=?").bind(&id).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO conversation_memory_evidence(item_id,ordinal,message_id,sequence,role,start_byte,end_byte,content_digest,purpose) SELECT 'rebuilt',ordinal,message_id,sequence,role,start_byte,end_byte,content_digest,purpose FROM conversation_memory_evidence WHERE item_id=?").bind(&id).execute(&pool).await.unwrap();
    let forgotten: i64 = sqlx::query_scalar(
        "SELECT forgotten FROM conversation_memory_attributes WHERE item_id='rebuilt'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(forgotten, 1);
    assert!(repo
        .prepare_knowledge("one", "budget")
        .await
        .unwrap()
        .optional
        .is_empty());
}
