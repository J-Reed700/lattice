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
        sqlx::query("INSERT INTO conversations(id,title,space_id) VALUES (?,?,?)")
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
async fn a_page_batches_evidence_and_keeps_each_facts_source_message() {
    let (_, repo) = setup().await;
    let (empty, reads) =
        crate::tests::common::query_counts::count_reads(repo.list_knowledge("three", 0, true))
            .await;
    assert!(empty.unwrap().items.is_empty());
    assert_eq!(reads, 3);
    let mut expected = std::collections::HashSet::new();
    for index in 0..12 {
        let mut r = request("one", "remember");
        let text = format!("The budget for project {index} is ${index}00.");
        expected.insert(text.clone());
        r.text = Some(text);
        r.scope = Some("personal".into());
        repo.knowledge(&r).await.unwrap();
    }
    let (response, reads) =
        crate::tests::common::query_counts::count_reads(repo.list_knowledge("three", 0, true))
            .await;
    let response = response.unwrap();
    assert_eq!(reads, 5, "evidence reads must not grow per fact");
    assert_eq!(response.items.len(), expected.len());
    let found: std::collections::HashSet<_> = response
        .items
        .iter()
        .map(|item| {
            assert_eq!(item.conversation_id, "one");
            assert_eq!(item.evidence.len(), 1);
            item.evidence[0].text.clone().unwrap()
        })
        .collect();
    assert_eq!(found, expected);
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
    r.scope = Some("pool".into());
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
    r.action = "dates".into();
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
async fn a_remembered_or_corrected_text_is_kept_whole_at_any_length() {
    let (_, repo) = setup().await;
    let long = "The project budget is $120 and it covers every phase of the work. ".repeat(200);
    let long = long.trim().to_string();
    let mut r = request("one", "remember");
    r.text = Some(long.clone());
    r.scope = Some("conversation".into());
    let remembered = repo.knowledge(&r).await.unwrap();
    assert_eq!(remembered.items[0].label, long);
    assert_eq!(
        remembered.items[0].evidence[0].text.as_deref(),
        Some(long.as_str())
    );

    let corrected = format!("Correction: {long}");
    let mut r = request("one", "correct");
    r.item_id = Some(remembered.items[0].id.clone());
    r.text = Some(corrected.clone());
    let response = repo.knowledge(&r).await.unwrap();
    let active = response
        .items
        .iter()
        .find(|item| item.state == "active")
        .unwrap();
    assert_eq!(active.label, corrected);
    assert_eq!(active.evidence[0].text.as_deref(), Some(corrected.as_str()));
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
    repo.knowledge(&r).await.unwrap();
    let memory = repo.prepare_knowledge("one", "What next?").await.unwrap();
    assert_eq!(memory.mandatory.len(), 1);
    assert!(memory.mandatory[0].contains("Never deploy without my approval."));
    assert!(memory.mandatory[0].contains("source_conversation"));
}
#[tokio::test]
async fn a_requirement_shared_by_hand_is_context_in_the_other_chats() {
    let (_, repo) = setup().await;
    let mut r = request("one", "remember");
    r.kind = Some("constraint".into());
    r.text = Some("Never deploy without my approval.".into());
    r.scope = Some("pool".into());
    repo.knowledge(&r).await.unwrap();

    let memory = repo
        .prepare_knowledge("two", "Can we deploy?")
        .await
        .unwrap();

    assert!(memory.mandatory.is_empty());
    assert_eq!(memory.optional.len(), 1);
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
impl lattice_core::application::ports::EmbeddingPort for Embedder {
    async fn embed_single(&self, _: &str) -> lattice_core::shared::error::Result<Vec<f32>> {
        Ok(vec![1.0, 0.0])
    }
    async fn embed_batch(
        &self,
        texts: &[String],
    ) -> lattice_core::shared::error::Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| vec![1.0, 0.0]).collect())
    }
    fn dimension(&self) -> usize {
        2
    }
    fn model_identity(&self) -> String {
        "test-space".into()
    }
    async fn is_ready(&self) -> lattice_core::shared::error::Result<bool> {
        Ok(true)
    }
}
#[tokio::test]
async fn semantic_recall_finds_paraphrases_but_never_foreign_or_incompatible_vectors() {
    use lattice_core::application::ports::conversation_memory::ConversationMemoryReadPort;
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
            .bind(id).bind(conversation).bind(id).bind(text).bind(lattice_core::shared::persistence::embedding_encoding::encode_embedding(&[1.0,0.0])).bind(model).execute(&pool).await.unwrap();
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

async fn said_in(pool: &SqlitePool, conversation: &str, kind: &str, text: &str) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let message = uuid::Uuid::new_v4().to_string();
    let digest = lattice_core::domain::conversation::memory::compute_digest(text);
    let sequence: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(sequence),0)+1 FROM conversation_messages WHERE conversation_id=?",
    )
    .bind(conversation)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO conversation_messages(id,conversation_id,role,content,tokens,status,sequence,content_digest) VALUES (?,?,'user',?,1,'completed',?,?)")
        .bind(&message).bind(conversation).bind(text).bind(sequence).bind(&digest).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO conversation_memory_items(id,conversation_id,kind,label,created_at_sequence,changed_at_sequence,created_at,updated_at) VALUES (?,?,?,?,?,?,'2026-11-01T00:00:00Z','2026-11-01T00:00:00Z')")
        .bind(&id).bind(conversation).bind(kind).bind(text).bind(sequence).bind(sequence).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO conversation_memory_evidence(item_id,ordinal,message_id,sequence,role,start_byte,end_byte,content_digest,purpose) VALUES (?,0,?,?,'user',0,?,?,'assertion')")
        .bind(&id).bind(&message).bind(sequence).bind(text.len() as i64).bind(&digest).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO conversation_memory_state(conversation_id,memory_revision) VALUES (?,1) ON CONFLICT(conversation_id) DO NOTHING")
        .bind(conversation).execute(pool).await.unwrap();
    id
}

async fn bind_folder(pool: &SqlitePool, conversation: &str, root: &str) {
    sqlx::query("UPDATE conversations SET explorer_root=? WHERE id=?")
        .bind(root)
        .bind(conversation)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT OR IGNORE INTO explorer_folders(root,name,added_at,last_opened_at) VALUES (?,?,'2026-11-01T00:00:00Z','2026-11-01T00:00:00Z')")
        .bind(root).bind(root).execute(pool).await.unwrap();
}

async fn visible(repo: &ConversationRepository, conversation: &str) -> Vec<String> {
    repo.list_knowledge(conversation, 0, true)
        .await
        .unwrap()
        .items
        .into_iter()
        .map(|item| item.id)
        .collect()
}

#[tokio::test]
async fn what_the_user_says_about_themselves_reaches_the_chats_in_their_space() {
    let (pool, repo) = setup().await;
    for kind in ["user_fact", "preference", "goal"] {
        let id = said_in(&pool, "one", kind, "I have an interview on Friday.").await;
        assert!(visible(&repo, "two").await.contains(&id), "{kind}");
        assert!(!visible(&repo, "three").await.contains(&id), "{kind}");
    }
}

#[tokio::test]
async fn a_chats_own_instructions_and_decisions_stay_in_it() {
    let (pool, repo) = setup().await;
    for kind in ["constraint", "decision", "open_question"] {
        let id = said_in(&pool, "one", kind, "No LINQ in this answer.").await;
        assert!(visible(&repo, "one").await.contains(&id), "{kind}");
        assert!(!visible(&repo, "two").await.contains(&id), "{kind}");
    }
}

#[tokio::test]
async fn a_shared_item_is_context_and_never_a_requirement() {
    let (pool, repo) = setup().await;
    said_in(
        &pool,
        "one",
        "goal",
        "I want to pass the interview on Friday.",
    )
    .await;

    let own = repo.prepare_knowledge("one", "anything").await.unwrap();
    assert_eq!((own.mandatory.len(), own.optional.len()), (1, 0));

    let shared = repo
        .prepare_knowledge("two", "how long is the interview?")
        .await
        .unwrap();
    assert_eq!((shared.mandatory.len(), shared.optional.len()), (0, 1));
}

#[tokio::test]
async fn a_shared_item_with_no_bearing_on_the_turn_stays_out_of_it() {
    let (pool, repo) = setup().await;
    said_in(&pool, "one", "user_fact", "I have an interview on Friday.").await;

    let shared = repo
        .prepare_knowledge("two", "rename this variable")
        .await
        .unwrap();
    assert!(shared.optional.is_empty());

    let own = repo
        .prepare_knowledge("one", "rename this variable")
        .await
        .unwrap();
    assert_eq!(own.optional.len(), 1);
}

#[tokio::test]
async fn a_word_inside_another_word_is_not_a_match() {
    let (pool, repo) = setup().await;
    said_in(&pool, "one", "user_fact", "I start the new job on Friday.").await;

    let shared = repo
        .prepare_knowledge("two", "rename the art folder")
        .await
        .unwrap();
    assert!(shared.optional.is_empty());
}

#[tokio::test]
async fn the_same_fact_said_in_two_chats_is_offered_once() {
    let (pool, repo) = setup().await;
    said_in(&pool, "one", "user_fact", "I have an interview on Friday.").await;
    said_in(&pool, "two", "user_fact", "I have an interview on Friday.").await;

    let prepared = repo
        .prepare_knowledge("two", "when is the interview?")
        .await
        .unwrap();
    assert_eq!(prepared.optional.len(), 1);
}

#[tokio::test]
async fn a_subfolder_chat_joins_its_parent_folders_pool() {
    let (pool, repo) = setup().await;
    bind_folder(&pool, "one", "/work/app").await;
    bind_folder(&pool, "two", "/work/app/web").await;
    bind_folder(&pool, "three", "/work/other").await;

    let parent = said_in(&pool, "one", "user_fact", "I deploy on Fridays.").await;
    let sub = said_in(&pool, "two", "user_fact", "I use pnpm here.").await;

    assert!(visible(&repo, "two").await.contains(&parent));
    assert!(visible(&repo, "one").await.contains(&sub));
    assert!(visible(&repo, "three").await.is_empty());
}

#[tokio::test]
async fn a_folder_that_only_shares_a_name_prefix_is_another_pool() {
    let (pool, repo) = setup().await;
    bind_folder(&pool, "one", "/work/app").await;
    bind_folder(&pool, "two", "/work/app-old").await;

    said_in(&pool, "one", "user_fact", "I deploy on Fridays.").await;

    assert!(visible(&repo, "two").await.is_empty());
}

#[tokio::test]
async fn a_folder_chat_and_a_space_chat_are_different_pools() {
    let (pool, repo) = setup().await;
    bind_folder(&pool, "one", "/work/app").await;

    said_in(&pool, "one", "user_fact", "I deploy on Fridays.").await;
    assert!(visible(&repo, "two").await.is_empty());

    said_in(&pool, "two", "user_fact", "I live in Leeds.").await;
    assert_eq!(visible(&repo, "one").await.len(), 1);
}

#[tokio::test]
async fn sharing_a_decision_by_hand_reaches_the_folders_pool() {
    let (pool, repo) = setup().await;
    bind_folder(&pool, "one", "/work/app").await;
    bind_folder(&pool, "two", "/work/app/web").await;
    bind_folder(&pool, "three", "/work/other").await;
    let id = said_in(&pool, "one", "decision", "We chose Postgres.").await;
    assert!(!visible(&repo, "two").await.contains(&id));

    let mut share = request("one", "scope");
    share.item_id = Some(id.clone());
    share.scope = Some("pool".into());
    let response = repo.knowledge(&share).await.unwrap();

    assert_eq!(response.pool, "folder");
    assert!(visible(&repo, "two").await.contains(&id));
    assert!(!visible(&repo, "three").await.contains(&id));
}

#[tokio::test]
async fn an_item_kept_to_one_chat_is_not_shared() {
    let (pool, repo) = setup().await;
    let id = said_in(&pool, "one", "user_fact", "I have an interview on Friday.").await;
    let mut keep = request("one", "scope");
    keep.item_id = Some(id);
    keep.scope = Some("conversation".into());
    repo.knowledge(&keep).await.unwrap();

    assert!(visible(&repo, "two").await.is_empty());
}

#[tokio::test]
async fn checking_a_fact_does_not_stop_it_being_shared() {
    let (pool, repo) = setup().await;
    let id = said_in(&pool, "one", "user_fact", "I have an interview on Friday.").await;
    let mut check = request("one", "verify");
    check.item_id = Some(id.clone());
    repo.knowledge(&check).await.unwrap();

    assert_eq!(visible(&repo, "two").await, [id]);
}

#[tokio::test]
async fn a_correction_in_another_chat_replaces_the_shared_item_in_both() {
    let (pool, repo) = setup().await;
    let old = said_in(&pool, "one", "user_fact", "I have an interview on Friday.").await;
    let before: i64 = sqlx::query_scalar(
        "SELECT memory_revision FROM conversation_memory_state WHERE conversation_id='one'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let mut correct = request("two", "correct");
    correct.item_id = Some(old.clone());
    correct.text = Some("My interview moved to Monday.".into());
    repo.knowledge(&correct).await.unwrap();

    for chat in ["one", "two"] {
        let items = repo.list_knowledge(chat, 0, true).await.unwrap().items;
        assert_eq!(items.len(), 1, "{chat}");
        assert_eq!(items[0].label, "My interview moved to Monday.", "{chat}");
    }
    let after: i64 = sqlx::query_scalar(
        "SELECT memory_revision FROM conversation_memory_state WHERE conversation_id='one'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        after > before,
        "the source chat's in-flight work must not survive"
    );
}

#[tokio::test]
async fn forgetting_in_another_chat_removes_the_item_from_both() {
    let (pool, repo) = setup().await;
    let id = said_in(&pool, "one", "user_fact", "I have an interview on Friday.").await;
    sqlx::query("INSERT INTO conversation_summaries(id,conversation_id,summary_text,up_to_message_id,original_message_count,original_tokens,summary_tokens,compression_ratio,created_at,memory_revision) SELECT 's','one','Interview on Friday.',id,1,1,1,1.0,'2026-11-01T00:00:00Z',1 FROM conversation_messages WHERE conversation_id='one' LIMIT 1")
        .execute(&pool).await.unwrap();

    let mut forget = request("two", "forget");
    forget.item_id = Some(id);
    repo.knowledge(&forget).await.unwrap();

    assert!(visible(&repo, "one").await.is_empty());
    assert!(visible(&repo, "two").await.is_empty());
    let summaries: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversation_summaries WHERE conversation_id='one'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(summaries, 0, "a summary can repeat what was forgotten");
}

#[tokio::test]
async fn a_chat_outside_the_pool_cannot_change_a_shared_item() {
    let (pool, repo) = setup().await;
    let id = said_in(&pool, "one", "user_fact", "I have an interview on Friday.").await;

    for action in ["correct", "forget"] {
        let mut change = request("three", action);
        change.item_id = Some(id.clone());
        change.text = Some("My interview moved to Monday.".into());
        assert!(repo.knowledge(&change).await.is_err(), "{action}");
    }
    assert_eq!(visible(&repo, "one").await, [id]);
}

#[tokio::test]
async fn only_the_source_chat_changes_a_shared_items_sharing_or_dates() {
    let (pool, repo) = setup().await;
    let id = said_in(&pool, "one", "user_fact", "I have an interview on Friday.").await;

    for action in ["scope", "dates", "verify"] {
        let mut change = request("two", action);
        change.item_id = Some(id.clone());
        change.scope = Some("personal".into());
        assert!(repo.knowledge(&change).await.is_err(), "{action}");
    }
}

async fn add_course(pool: &SqlitePool, id: &str) {
    sqlx::query("INSERT INTO learning_programs(id,title,goal,status,revision,prior_knowledge,minutes_per_session,model_name,created_at) VALUES(?,'Course','Learn','active',0,'',20,'m',0)")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
}

async fn bind_course(pool: &SqlitePool, conversation: &str, course: &str) {
    sqlx::query("UPDATE conversations SET program_id=? WHERE id=?")
        .bind(course)
        .bind(conversation)
        .execute(pool)
        .await
        .unwrap();
}

async fn share_with_pool(repo: &ConversationRepository, conversation: &str, id: &str) {
    let mut share = request(conversation, "scope");
    share.item_id = Some(id.into());
    share.scope = Some("pool".into());
    repo.knowledge(&share).await.unwrap();
}

async fn delete_course(pool: &SqlitePool, id: &str) {
    sqlx::query("DELETE FROM learning_programs WHERE id=?")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
}

/// (scope_kind, scope_key, valid_until) of an item's row.
async fn attributes(
    pool: &SqlitePool,
    id: &str,
) -> Option<(Option<String>, Option<String>, Option<String>)> {
    sqlx::query_as(
        "SELECT scope_kind,scope_key,valid_until FROM conversation_memory_attributes WHERE item_id=?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn a_tutor_chat_shares_with_its_course_and_not_with_its_space() {
    let (pool, repo) = setup().await;
    add_course(&pool, "c1").await;
    bind_course(&pool, "one", "c1").await;
    bind_course(&pool, "three", "c1").await;

    let course = said_in(&pool, "one", "user_fact", "I have an interview on Friday.").await;
    let space = said_in(&pool, "two", "user_fact", "I live in Leeds.").await;

    assert!(visible(&repo, "three").await.contains(&course));
    assert!(!visible(&repo, "two").await.contains(&course));
    assert!(!visible(&repo, "one").await.contains(&space));
    let listed = repo.list_knowledge("one", 0, false).await.unwrap();
    assert_eq!(listed.pool, "course");
    assert_eq!(listed.items[0].scope, "course");
}

#[tokio::test]
async fn a_lesson_does_not_split_its_courses_pool() {
    let (pool, repo) = setup().await;
    add_course(&pool, "c1").await;
    sqlx::query("INSERT INTO learning_modules(id,program_id,ordinal,title,summary,outcomes_json) VALUES('m1','c1',0,'Module','Summary','[]')")
        .execute(&pool).await.unwrap();
    for (lesson, ordinal, chat) in [("l1", 0, "one"), ("l2", 1, "two")] {
        sqlx::query("INSERT INTO learning_lessons(id,program_id,module_id,ordinal,title,objective,estimated_minutes,preparation) VALUES(?,'c1','m1',?,'Lesson','Explain',20,'outline')")
            .bind(lesson).bind(ordinal).execute(&pool).await.unwrap();
        sqlx::query("UPDATE conversations SET program_id='c1',lesson_id=? WHERE id=?")
            .bind(lesson)
            .bind(chat)
            .execute(&pool)
            .await
            .unwrap();
    }
    bind_course(&pool, "three", "c1").await;

    let id = said_in(&pool, "one", "user_fact", "I have an interview on Friday.").await;

    assert!(visible(&repo, "two").await.contains(&id));
    assert!(visible(&repo, "three").await.contains(&id));
}

#[tokio::test]
async fn two_courses_are_two_pools() {
    let (pool, repo) = setup().await;
    add_course(&pool, "c1").await;
    add_course(&pool, "c2").await;
    bind_course(&pool, "one", "c1").await;
    bind_course(&pool, "two", "c2").await;

    said_in(&pool, "one", "user_fact", "I have an interview on Friday.").await;

    assert!(visible(&repo, "two").await.is_empty());
}

#[tokio::test]
async fn sharing_a_decision_by_hand_reaches_the_courses_pool() {
    let (pool, repo) = setup().await;
    add_course(&pool, "c1").await;
    bind_course(&pool, "one", "c1").await;
    bind_course(&pool, "three", "c1").await;
    let id = said_in(&pool, "one", "decision", "We chose Postgres.").await;
    assert!(!visible(&repo, "three").await.contains(&id));

    share_with_pool(&repo, "one", &id).await;

    assert!(visible(&repo, "three").await.contains(&id));
    assert!(!visible(&repo, "two").await.contains(&id));
    let (kind, key, _) = attributes(&pool, &id).await.unwrap();
    assert_eq!(
        (kind.as_deref(), key.as_deref()),
        (Some("course"), Some("c1"))
    );
}

#[tokio::test]
async fn a_correction_or_a_forget_in_one_tutor_chat_reaches_the_course() {
    let (pool, repo) = setup().await;
    add_course(&pool, "c1").await;
    bind_course(&pool, "one", "c1").await;
    bind_course(&pool, "two", "c1").await;
    let old = said_in(&pool, "one", "user_fact", "I have an interview on Friday.").await;

    let mut correct = request("two", "correct");
    correct.item_id = Some(old.clone());
    correct.text = Some("My interview moved to Monday.".into());
    let new = repo.knowledge(&correct).await.unwrap().items[0].id.clone();

    assert_eq!(visible(&repo, "one").await, std::slice::from_ref(&new));

    let mut forget = request("one", "forget");
    forget.item_id = Some(new);
    repo.knowledge(&forget).await.unwrap();

    assert!(visible(&repo, "one").await.is_empty());
    assert!(visible(&repo, "two").await.is_empty());
}

#[tokio::test]
async fn a_chat_on_a_folder_and_a_course_shares_with_the_folder_a_turn_reads() {
    let (pool, repo) = setup().await;
    add_course(&pool, "c1").await;
    bind_course(&pool, "one", "c1").await;
    bind_folder(&pool, "one", "/work/app").await;
    bind_course(&pool, "two", "c1").await;
    bind_folder(&pool, "three", "/work/app").await;

    let id = said_in(&pool, "one", "user_fact", "I deploy on Fridays.").await;

    assert!(visible(&repo, "three").await.contains(&id));
    assert!(!visible(&repo, "two").await.contains(&id));
    let listed = repo.list_knowledge("one", 0, false).await.unwrap();
    assert_eq!(listed.pool, "folder");
}

#[tokio::test]
async fn a_deleted_course_leaves_ordinary_chats_that_share_with_their_space() {
    let (pool, repo) = setup().await;
    add_course(&pool, "c1").await;
    bind_course(&pool, "one", "c1").await;
    let fact = said_in(&pool, "one", "user_fact", "I have an interview on Friday.").await;
    let decision = said_in(&pool, "one", "decision", "We chose Postgres.").await;
    share_with_pool(&repo, "one", &decision).await;
    assert!(!visible(&repo, "two").await.contains(&fact));

    delete_course(&pool, "c1").await;

    let listed = repo.list_knowledge("one", 0, false).await.unwrap();
    assert_eq!(listed.pool, "space");
    assert_eq!(listed.items.len(), 2, "the chat keeps its own items");
    let seen = visible(&repo, "two").await;
    assert!(seen.contains(&fact));
    assert!(
        !seen.contains(&decision),
        "the sharing went with the course"
    );
    let named: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversation_memory_attributes WHERE scope_kind='course'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(named, 0);
}

/// A row says the user wrote or changed an item, and carries its dates. Only
/// the sharing goes with the course.
#[tokio::test]
async fn a_deleted_course_leaves_the_rest_of_what_the_user_set_on_an_item() {
    let (pool, repo) = setup().await;
    add_course(&pool, "c1").await;
    bind_course(&pool, "one", "c1").await;
    let id = said_in(&pool, "one", "user_fact", "I have an interview on Friday.").await;
    share_with_pool(&repo, "one", &id).await;
    let mut dates = request("one", "dates");
    dates.item_id = Some(id.clone());
    dates.valid_until = Some("2030-01-01T00:00:00Z".into());
    repo.knowledge(&dates).await.unwrap();

    delete_course(&pool, "c1").await;

    let (kind, key, until) = attributes(&pool, &id).await.expect("the row stays");
    assert_eq!((kind, key), (None, None));
    assert!(until.is_some());
}

#[tokio::test]
async fn a_deleted_space_takes_its_sharing_and_nothing_else_with_it() {
    let (pool, repo) = setup().await;
    let id = said_in(&pool, "three", "decision", "We chose Postgres.").await;
    share_with_pool(&repo, "three", &id).await;
    let mut dates = request("three", "dates");
    dates.item_id = Some(id.clone());
    dates.valid_until = Some("2030-01-01T00:00:00Z".into());
    repo.knowledge(&dates).await.unwrap();

    sqlx::query("UPDATE conversations SET space_id='space_general' WHERE id='three'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM conversation_spaces WHERE id='other'")
        .execute(&pool)
        .await
        .unwrap();

    let (kind, key, until) = attributes(&pool, &id).await.expect("the row stays");
    assert_eq!((kind, key), (None, None));
    assert!(until.is_some());
}

#[tokio::test]
async fn a_pool_of_more_than_a_page_does_not_stop_a_turn() {
    let (pool, repo) = setup().await;
    let wanted = said_in(&pool, "one", "user_fact", "My interview is on Friday.").await;
    for index in 0..1005 {
        said_in(&pool, "one", "user_fact", &format!("Note number {index}.")).await;
    }

    let prepared = repo
        .prepare_knowledge("two", "when is the interview?")
        .await
        .unwrap();

    assert_eq!(prepared.optional.len(), 1);
    assert!(prepared.optional[0].contains(&wanted));
}

#[tokio::test]
async fn a_chat_is_shown_the_shared_items_that_bear_on_what_it_read() {
    let (pool, repo) = setup().await;
    let bearing = said_in(&pool, "one", "user_fact", "I have an interview on Friday.").await;
    said_in(&pool, "one", "user_fact", "I live in Leeds.").await;
    said_in(&pool, "two", "user_fact", "My interview coach is Sam.").await;

    let items = repo
        .shared_memory_items("two", "The interview moved to Monday.")
        .await
        .unwrap();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id.as_str(), bearing);
    assert_eq!(items[0].conversation_id, "one");
    assert_eq!(items[0].evidence.len(), 1);
    assert_eq!(items[0].changed_at_sequence, 0);
}
