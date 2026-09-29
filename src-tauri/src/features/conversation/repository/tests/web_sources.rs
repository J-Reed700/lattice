//! Web pages a turn cited, kept as context for the turns that follow.

use super::*;
use crate::application::ports::conversation_context::ConversationContextPort;
use crate::features::conversation::repository::ConversationRepository;
use crate::infrastructure::conversation_context::SqliteConversationContext;

/// The citation archive outlives the page cache and the page itself: the text
/// a conversation read is stored beside the citation, the first text read is
/// the one kept, and citing the page again with a short excerpt does not
/// clobber the archive.
#[tokio::test]
async fn a_read_page_is_archived_beside_its_citation_permanently() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());
    let url = "https://example.com/purple-sprouting".to_string();

    repo.store_conversation_web_source_snapshot(
        conversation_id.clone(),
        url.clone(),
        Some("Purple sprouting broccoli".to_string()),
        "The full article text, as first read.".to_string(),
        false,
    )
    .await
    .unwrap();

    let archived = repo
        .conversation_web_source_snapshot(conversation_id.clone(), url.clone())
        .await
        .unwrap()
        .expect("the snapshot survived the round trip");
    assert_eq!(archived.content, "The full article text, as first read.");
    assert_eq!(archived.title.as_deref(), Some("Purple sprouting broccoli"));
    assert!(!archived.truncated);
    assert!(archived.fetched_at.is_some());

    // A later read of the same page is new information; it must not rewrite
    // the text earlier answers were based on.
    repo.store_conversation_web_source_snapshot(
        conversation_id.clone(),
        url.clone(),
        None,
        "The article, quietly rewritten since.".to_string(),
        false,
    )
    .await
    .unwrap();
    let archived = repo
        .conversation_web_source_snapshot(conversation_id.clone(), url.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(archived.content, "The full article text, as first read.");

    // And the turn-finalization path, which only knows an excerpt, leaves the
    // archive alone when the citation is refreshed.
    repo.add_conversation_web_source(
        conversation_id.clone(),
        url.clone(),
        Some("Purple sprouting broccoli".to_string()),
        Some("Very frost hardy.".to_string()),
        Some(0.9),
    )
    .await
    .unwrap();
    let archived = repo
        .conversation_web_source_snapshot(conversation_id.clone(), url.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(archived.content, "The full article text, as first read.");

    // A page that was cited but never read has no snapshot, and the reader
    // says so plainly rather than inventing one.
    repo.add_conversation_web_source(
        conversation_id.clone(),
        "https://example.com/never-read".to_string(),
        None,
        Some("Snippet only.".to_string()),
        Some(0.5),
    )
    .await
    .unwrap();
    assert!(repo
        .conversation_web_source_snapshot(
            conversation_id.clone(),
            "https://example.com/never-read".to_string(),
        )
        .await
        .unwrap()
        .is_none());
}

/// The loop this closes, in one test.
///
/// Web research used to end with the turn that did it. The citations survived
/// in the answer's metadata and the sources panel counted them, but the next
/// turn assembled its context from `conversation_web_sources`, which nothing
/// wrote — so a chat could list thirty-seven sources in its sidebar and still
/// start the next turn with no grounded context at all.
///
/// Writer and reader are both the real ones: the repository call that turn
/// finalization makes, and the port the context builder reads through.
#[tokio::test]
async fn a_turns_citations_are_there_for_the_next_turn() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());

    for (url, title, excerpt) in [
        (
            "https://example.com/purple-sprouting",
            "Purple sprouting broccoli",
            "Very frost hardy; overwinters in zone 6 and up.",
        ),
        (
            "https://example.com/broccolini",
            "Broccolini basics",
            "Tolerates light frost down to −3°C.",
        ),
    ] {
        repo.add_conversation_web_source(
            conversation_id.clone(),
            url.to_string(),
            Some(title.to_string()),
            Some(excerpt.to_string()),
            Some(0.5),
        )
        .await
        .unwrap();
    }

    let carried = SqliteConversationContext::new(pool.clone())
        .linked_sources(&conversation_id, 10)
        .await
        .unwrap();

    assert_eq!(carried.len(), 2, "{carried:?}");
    let urls: Vec<&str> = carried.iter().map(|source| source.url.as_str()).collect();
    assert!(urls.contains(&"https://example.com/broccolini"), "{urls:?}");
    assert!(
        carried
            .iter()
            .any(|source| source.excerpt.as_deref() == Some("Tolerates light frost down to −3°C.")),
        "the excerpt is what makes the entry worth carrying:\n{carried:?}"
    );
}

/// Turns cite the same page repeatedly. The store is keyed by URL, so the
/// second citation refreshes the row rather than adding a rival copy that
/// would spend the next turn's budget saying the same thing twice.
#[tokio::test]
async fn citing_a_page_again_refreshes_it_rather_than_duplicating_it() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_thread(&pool).await;
    let repo = ConversationRepository::new(pool.clone());

    for excerpt in ["first reading", "a later, better reading"] {
        repo.add_conversation_web_source(
            conversation_id.clone(),
            "https://example.com/broccolini".to_string(),
            Some("Broccolini basics".to_string()),
            Some(excerpt.to_string()),
            Some(0.5),
        )
        .await
        .unwrap();
    }

    let carried = SqliteConversationContext::new(pool.clone())
        .linked_sources(&conversation_id, 10)
        .await
        .unwrap();

    assert_eq!(carried.len(), 1, "{carried:?}");
    assert_eq!(
        carried[0].excerpt.as_deref(),
        Some("a later, better reading")
    );
}

/// Sources belong to the conversation that cited them. A second chat reading
/// the first one's pages would be the space leak this app already fixed once.
#[tokio::test]
async fn another_conversation_does_not_inherit_them() {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let conversation_id = seed_thread(&pool).await;
    ConversationRepository::new(pool.clone())
        .add_conversation_web_source(
            conversation_id.clone(),
            "https://example.com/broccolini".to_string(),
            Some("Broccolini basics".to_string()),
            None,
            None,
        )
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO conversations (id, title, model_name, space_id, created_at, updated_at) \
         VALUES ('conv-2', 'Other', 'model', 'space_general', \
                 '2026-09-01T11:00:00Z', '2026-09-01T11:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let carried = SqliteConversationContext::new(pool.clone())
        .linked_sources("conv-2", 10)
        .await
        .unwrap();

    assert!(carried.is_empty(), "{carried:?}");
}
