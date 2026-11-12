use super::*;
use crate::features::conversation::repository::ConversationRepository;
use crate::features::conversation::space_dto::ListConversationsExplorerQueryDto;
use crate::features::conversation::tangent_dto::CreateTangentRequestDto;
use lattice_core::shared::persistence::Database;

fn request(conversation_id: &str, message_id: &str) -> CreateTangentRequestDto {
    CreateTangentRequestDto {
        conversation_id: conversation_id.into(),
        message_id: message_id.into(),
        selected_text: "first answer".into(),
    }
}

#[tokio::test]
async fn tangent_captures_context_without_changing_parent_or_entering_lists() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let parent = seed_thread(&pool).await;
    let db = Database::new(pool.clone());
    let repo = ConversationRepository::new(db.clone());
    let before = repo.find_by_id(&parent).await.unwrap().unwrap();
    let tangent = repo.create_tangent(request(&parent, "m2")).await.unwrap();
    assert_eq!(tangent.parent_conversation_id, parent);
    assert_eq!(tangent.context_message_count, 2);
    let messages = repo.get_messages(&tangent.conversation_id).await.unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].content, "first question");
    assert!(messages[1].content.contains("selected this passage"));
    assert!(messages[1].content.ends_with("> first answer"));
    assert_eq!(
        message_ids(&pool, &parent).await,
        vec!["m1", "m2", "m3", "m4"]
    );
    let after = repo.find_by_id(&parent).await.unwrap().unwrap();
    assert_eq!(before.updated_at, after.updated_at);
    assert_eq!((after.message_count, after.total_tokens), (4, 15));
    assert_eq!(
        repo.get_messages(&parent).await.unwrap()[1].content,
        "first answer"
    );
    assert_eq!(repo.find_all(None, None).await.unwrap().len(), 1);
    assert_eq!(repo.count().await.unwrap(), 1);
    let listed = repo
        .list_conversations_explorer(
            serde_json::from_value::<ListConversationsExplorerQueryDto>(serde_json::json!({}))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(listed.conversations.len(), 1);
    let reopened = ConversationRepository::new(db.clone());
    assert_eq!(
        reopened.list_tangents(&parent).await.unwrap()[0].selected_text,
        "first answer"
    );
}

#[tokio::test]
async fn tangent_promotion_keeps_transcript_and_survives_parent_deletion() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let parent = seed_thread(&pool).await;
    let db = Database::new(pool.clone());
    let repo = ConversationRepository::new(db.clone());
    let promoted = repo.create_tangent(request(&parent, "m2")).await.unwrap();
    let child = repo.create_tangent(request(&parent, "m4")).await.unwrap();
    seed_message(
        &pool,
        &promoted.conversation_id,
        "tangent-question",
        "user",
        "Why?",
        3,
        "2026-10-06T01:00:00Z",
    )
    .await;
    let ids = message_ids(&pool, &promoted.conversation_id).await;
    repo.promote_tangent(&promoted.conversation_id)
        .await
        .unwrap();
    repo.promote_tangent(&promoted.conversation_id)
        .await
        .unwrap();
    assert_eq!(repo.find_all(None, None).await.unwrap().len(), 2);
    assert_eq!(repo.list_tangents(&parent).await.unwrap().len(), 1);
    assert_eq!(message_ids(&pool, &promoted.conversation_id).await, ids);
    repo.delete(&parent).await.unwrap();
    assert!(repo
        .find_by_id(&child.conversation_id)
        .await
        .unwrap()
        .is_none());
    assert!(repo
        .get_messages(&child.conversation_id)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(message_ids(&pool, &promoted.conversation_id).await, ids);
}

#[tokio::test]
async fn tangent_anchor_deletion_retains_quote_and_nested_tangents_share_parent() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let parent = seed_thread(&pool).await;
    let db = Database::new(pool.clone());
    let repo = ConversationRepository::new(db.clone());
    let tangent = repo.create_tangent(request(&parent, "m2")).await.unwrap();
    seed_message(
        &pool,
        &tangent.conversation_id,
        "followup",
        "assistant",
        "A tangent answer",
        4,
        "2026-10-06T01:00:00Z",
    )
    .await;
    let nested = repo
        .create_tangent(request(&tangent.conversation_id, "followup"))
        .await
        .unwrap();
    assert_eq!(nested.parent_conversation_id, parent);
    assert_eq!(nested.source_conversation_id, tangent.conversation_id);
    repo.truncate_after(&parent, "m1", false).await.unwrap();
    assert_eq!(repo.list_tangents(&parent).await.unwrap().len(), 2);
    assert_eq!(
        repo.list_tangents(&parent)
            .await
            .unwrap()
            .iter()
            .find(|t| t.conversation_id == tangent.conversation_id)
            .unwrap()
            .selected_text,
        "first answer"
    );
}

#[tokio::test]
async fn a_tangent_can_start_from_a_passage_of_any_length() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let parent = seed_thread(&pool).await;
    let db = Database::new(pool.clone());
    let repo = ConversationRepository::new(db.clone());
    let passage = "A long passage of the reply, quoted whole. ".repeat(300);
    let passage = passage.trim().to_string();

    let tangent = repo
        .create_tangent(CreateTangentRequestDto {
            selected_text: passage.clone(),
            ..request(&parent, "m2")
        })
        .await
        .unwrap();

    assert_eq!(tangent.selected_text, passage);
    let messages = repo.get_messages(&tangent.conversation_id).await.unwrap();
    assert!(messages[1].content.ends_with(&format!("> {passage}")));
}

#[tokio::test]
async fn invalid_tangent_requests_cannot_create_orphan_conversations() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let parent = seed_thread(&pool).await;
    let db = Database::new(pool.clone());
    let repo = ConversationRepository::new(db.clone());
    let other = repo.create_conversation("Other", None).await.unwrap();
    let cases = [
        request(&parent, "unknown"),
        request(&parent, "m1"),
        request(&other.id.to_string(), "m2"),
        CreateTangentRequestDto {
            selected_text: "  ".into(),
            ..request(&parent, "m2")
        },
    ];
    for case in cases {
        assert!(repo.create_tangent(case).await.is_err());
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM conversations")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
    sqlx::query("CREATE TRIGGER reject_tangent_copy BEFORE INSERT ON conversation_messages BEGIN SELECT RAISE(ABORT, 'copy failed'); END").execute(&pool).await.unwrap();
    assert!(repo.create_tangent(request(&parent, "m2")).await.is_err());
    assert!(repo.list_tangents(&parent).await.unwrap().is_empty());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM conversations")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
}

/// A tutor chat bound to a course and one of its lessons.
async fn seed_tutor_chat(pool: &SqlitePool) -> String {
    let parent = seed_thread(pool).await;
    sqlx::query("INSERT INTO learning_programs(id,title,goal,status,revision,prior_knowledge,minutes_per_session,model_name,created_at) VALUES('course','Search','Learn','active',0,'',20,'m',0)")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO learning_modules(id,program_id,ordinal,title,summary,outcomes_json) VALUES('module','course',0,'Ranking','Summary','[]')")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO learning_lessons(id,program_id,module_id,ordinal,title,objective,estimated_minutes,preparation) VALUES('lesson','course','module',0,'Fusion','Explain',20,'outline')")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE conversations SET program_id='course', lesson_id='lesson' WHERE id=?")
        .bind(&parent)
        .execute(pool)
        .await
        .unwrap();
    parent
}

async fn projected_binding(
    repo: &ConversationRepository,
    id: &str,
) -> (Option<String>, Option<String>) {
    let conversation = repo.find_by_id(id).await.unwrap().unwrap();
    let projected = repo.project_conversation(&conversation).await.unwrap();
    (projected.program_id, projected.lesson_id)
}

/// A branch of a tutor chat, and the chat a handoff continues it in, teach from
/// the same course; without it they would surface in Chat as plain chats.
#[tokio::test]
async fn a_tutor_chat_s_branches_stay_on_its_course() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let parent = seed_tutor_chat(&pool).await;
    let db = Database::new(pool.clone());
    let repo = ConversationRepository::new(db.clone());
    let bound = (Some("course".to_string()), Some("lesson".to_string()));
    assert_eq!(projected_binding(&repo, &parent).await, bound);

    repo.fork(&parent, None, "branch", "Branch").await.unwrap();
    assert_eq!(projected_binding(&repo, "branch").await, bound);

    repo.create_continuation(&parent, "next", "Next", "Summary", "{}")
        .await
        .unwrap();
    assert_eq!(projected_binding(&repo, "next").await, bound);

    let tangent = repo.create_tangent(request(&parent, "m2")).await.unwrap();
    assert_eq!(
        projected_binding(&repo, &tangent.conversation_id).await,
        bound
    );
}

/// The Tutor tab lists only its own course's chats.
#[tokio::test]
async fn the_conversation_list_narrows_to_one_course() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let tutor = seed_tutor_chat(&pool).await;
    sqlx::query("INSERT INTO conversations (id, title, space_id, created_at, updated_at) VALUES ('plain', 'Plain', 'space_general', '2026-09-01T10:00:00Z', '2026-09-01T10:00:00Z')")
        .execute(&pool)
        .await
        .unwrap();
    let db = Database::new(pool.clone());
    let repo = ConversationRepository::new(db.clone());
    let list = |program_id: Option<&str>| {
        let repo = repo.clone();
        let program_id = program_id.map(str::to_string);
        async move {
            repo.list_conversations_explorer(ListConversationsExplorerQueryDto {
                space_id: None,
                program_id,
                query: None,
                saved_only: None,
                bookmarked_only: None,
                pinned_only: None,
                has_message_bookmarks: None,
                include_archived: None,
                limit: None,
                offset: None,
            })
            .await
            .unwrap()
            .conversations
        }
    };

    let course = list(Some("course")).await;
    assert_eq!(course.len(), 1);
    assert_eq!(course[0].id, tutor);
    assert_eq!(course[0].program_id.as_deref(), Some("course"));
    assert_eq!(course[0].lesson_id.as_deref(), Some("lesson"));
    assert_eq!(list(None).await.len(), 2);
    assert!(list(Some("another-course")).await.is_empty());
}

#[tokio::test]
async fn explorer_tangent_preserves_folder_prompt_and_sources() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let parent = seed_thread(&pool).await;
    sqlx::query("UPDATE conversations SET explorer_root = '/project', system_prompt = 'Keep answers concise' WHERE id = ?").bind(&parent).execute(&pool).await.unwrap();
    seed_documents(&pool, &["source"]).await;
    let db = Database::new(pool.clone());
    let repo = ConversationRepository::new(db.clone());
    repo.add_document_reference(&parent, "source", None, Some(0.9))
        .await
        .unwrap();
    let tangent = repo.create_tangent(request(&parent, "m2")).await.unwrap();
    let detail = repo
        .find_by_id(&tangent.conversation_id)
        .await
        .unwrap()
        .unwrap();
    let projected = repo.project_conversation(&detail).await.unwrap();
    assert_eq!(projected.explorer_root.as_deref(), Some("/project"));
    assert_eq!(
        projected.tangent_parent_id.as_deref(),
        Some(parent.as_str())
    );
    assert_eq!(
        projected.system_prompt.as_deref(),
        Some("Keep answers concise")
    );
    assert_eq!(
        repo.get_document_references(&tangent.conversation_id)
            .await
            .unwrap()
            .len(),
        1
    );
    let counts = crate::features::explorer::repository::thread_counts(&db)
        .await
        .unwrap();
    assert_eq!(counts.get("/project"), Some(&1));
    assert_eq!(
        crate::features::explorer::repository::folder_threads(&db, "/project")
            .await
            .unwrap(),
        vec![parent]
    );
    repo.promote_tangent(&tangent.conversation_id)
        .await
        .unwrap();
    let counts = crate::features::explorer::repository::thread_counts(&db)
        .await
        .unwrap();
    assert_eq!(counts.get("/project"), Some(&2));
}
