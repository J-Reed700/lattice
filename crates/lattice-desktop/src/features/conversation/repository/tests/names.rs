//! The names a conversation has given the things it showed its model.

use super::*;
use crate::features::conversation::repository::ConversationRepository;
use lattice_core::application::ports::conversation_names::ConversationNames;
use lattice_core::application::services::model_contract::shown::Record;
use lattice_core::shared::persistence::Database;
use serde_json::json;

fn record(prefix: &str, ordinal: u32, reference: &str) -> Record {
    Record {
        prefix: prefix.into(),
        ordinal,
        under: None,
        reference: json!(reference),
    }
}

#[tokio::test]
async fn names_come_back_in_the_order_they_were_given_with_their_parent() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_thread(&pool).await;
    let repo = ConversationRepository::new(Database::new(pool.clone()));
    let option = Record {
        prefix: "o".into(),
        ordinal: 1,
        under: Some(("q".into(), 1)),
        reference: json!({ "index": 0 }),
    };

    repo.keep(
        &conversation_id,
        0,
        &[record("s", 1, "alpha"), record("q", 1, "beta"), option],
    )
    .await
    .unwrap();
    repo.keep(&conversation_id, 3, &[record("s", 2, "gamma")])
        .await
        .unwrap();

    let names = repo.names(&conversation_id).await.unwrap();
    let ordinals: Vec<(&str, u32)> = names
        .iter()
        .map(|name| (name.prefix.as_str(), name.ordinal))
        .collect();
    assert_eq!(ordinals, vec![("s", 1), ("q", 1), ("o", 1), ("s", 2)]);
    assert_eq!(names[2].under, Some(("q".to_owned(), 1)));
    assert_eq!(names[3].reference, json!("gamma"));
}

#[tokio::test]
async fn a_position_already_written_is_an_error_and_keeps_what_it_holds() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_thread(&pool).await;
    let repo = ConversationRepository::new(Database::new(pool.clone()));

    repo.keep(&conversation_id, 0, &[record("s", 1, "alpha")])
        .await
        .unwrap();
    let again = repo
        .keep(
            &conversation_id,
            0,
            &[record("s", 1, "other"), record("s", 2, "beta")],
        )
        .await;

    assert!(again.is_err());
    let names = repo.names(&conversation_id).await.unwrap();
    assert_eq!(names.len(), 1, "the failed write left nothing behind");
    assert_eq!(names[0].reference, json!("alpha"));
}

#[tokio::test]
async fn a_fork_has_the_names_of_its_parent_and_a_continuation_too() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_thread(&pool).await;
    let repo = ConversationRepository::new(Database::new(pool.clone()));
    repo.keep(
        &conversation_id,
        0,
        &[record("s", 1, "alpha"), record("s", 2, "beta")],
    )
    .await
    .unwrap();

    repo.fork(
        &conversation_id,
        Some("m2"),
        "conv-branch",
        "Thread · branch",
    )
    .await
    .unwrap();
    repo.create_continuation(
        &conversation_id,
        "conv-next",
        "Thread · next",
        "Summary",
        "{}",
    )
    .await
    .unwrap();

    for copy in ["conv-branch", "conv-next"] {
        let names = repo.names(copy).await.unwrap();
        assert_eq!(names, repo.names(&conversation_id).await.unwrap(), "{copy}");
    }
}

#[tokio::test]
async fn a_deleted_message_takes_no_name_with_it() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_thread(&pool).await;
    let repo = ConversationRepository::new(Database::new(pool.clone()));
    repo.keep(&conversation_id, 0, &[record("s", 1, "alpha")])
        .await
        .unwrap();

    repo.delete_messages(&conversation_id, &["m2".to_string()])
        .await
        .unwrap();

    assert_eq!(repo.names(&conversation_id).await.unwrap().len(), 1);
}

#[tokio::test]
async fn deleting_the_conversation_deletes_its_names() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_thread(&pool).await;
    let repo = ConversationRepository::new(Database::new(pool.clone()));
    repo.keep(&conversation_id, 0, &[record("s", 1, "alpha")])
        .await
        .unwrap();

    sqlx::query("DELETE FROM conversations WHERE id = ?")
        .bind(&conversation_id)
        .execute(&pool)
        .await
        .unwrap();

    assert!(repo.names(&conversation_id).await.unwrap().is_empty());
}
