//! What the container gives a turn: a tutor chat its course, a folder's thread
//! its folder, any other chat nothing.
use super::Container;
use crate::features::conversation::chat::ports::ChatStorage;
use crate::features::learning::tutor::{bind_conversation, SetConversationCourseRequestDto};
use crate::infrastructure::persistence::database::{initialize_database, DatabaseConnection};
use lattice_core::application::contracts::tools::FunctionCall;
use lattice_core::shared::persistence::driver::DriverPool;
use lattice_core::shared::persistence::Database;
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn a_turn_gets_the_course_or_folder_its_chat_is_bound_to() -> anyhow::Result<()> {
    let data = tempfile::tempdir()?;
    let database = Arc::new(DatabaseConnection::new(data.path().join("app.db")).await?);
    initialize_database(database.database()).await?;
    let pool = database.database().sqlx_pool().clone();
    let container = Container::new(
        pool.clone().into(),
        database,
        None,
        "http://127.0.0.1:1",
        "test-model",
        data.path().to_path_buf(),
        Arc::default(),
    )
    .await?;
    let program = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO learning_programs(id,title,goal,status,revision,prior_knowledge,minutes_per_session,model_name,created_at) VALUES(?,'Bound course','Learn it','active',1,'',30,'test',1)")
        .bind(&program)
        .execute(&pool)
        .await?;
    let folder = data.path().to_string_lossy().into_owned();
    for (id, root) in [("plain", None), ("tutor", None), ("folder", Some(&folder))] {
        sqlx::query("INSERT INTO conversations (id,title,explorer_root) VALUES (?,?,?)")
            .bind(id)
            .bind(id)
            .bind(root)
            .execute(&pool)
            .await?;
    }
    bind_conversation(
        &Database::from(pool.clone()),
        &SetConversationCourseRequestDto {
            conversation_id: "tutor".into(),
            program_id: program,
            lesson_id: None,
        },
    )
    .await?;

    assert!(container.turn_source("plain", None).await.is_none());

    let folder = container
        .turn_source("folder", None)
        .await
        .ok_or_else(|| anyhow::anyhow!("a folder's thread reads its folder"))?;
    assert!(!folder.owns_tool("search_course"));

    let course = container
        .turn_source("tutor", None)
        .await
        .ok_or_else(|| anyhow::anyhow!("a tutor chat reads its course"))?;
    for tool in [
        "search_course",
        "read_course_source",
        "course_outline",
        "due_reviews",
        "record_recall",
        "save_card",
        "save_note",
    ] {
        assert!(course.owns_tool(tool), "{tool}");
    }
    let outline = course
        .execute_tool(FunctionCall::new("1", "course_outline", json!({})), 2_000)
        .await?;
    assert!(outline.success, "{:?}", outline.error_message);
    assert!(
        outline
            .data
            .and_then(|data| data.as_str().map(str::to_owned))
            .is_some_and(|text| text.contains("Bound course")),
        "it reads the course from the container's database"
    );
    Ok(())
}
