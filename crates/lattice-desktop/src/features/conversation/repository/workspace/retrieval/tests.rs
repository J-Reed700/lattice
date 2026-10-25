use super::*;

mod batch_reads;

async fn scope_fixture() -> ConversationRepository {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::raw_sql("CREATE TABLE conversations (id TEXT, space_id TEXT);
            CREATE TABLE documents (id TEXT, owner_conversation_id TEXT);
            CREATE TABLE text_chunks (document_id TEXT);
            CREATE TABLE document_space_memberships (document_id TEXT, space_id TEXT);
            INSERT INTO conversations VALUES ('general', 'space_general'), ('patent', 'patent'), ('empty', 'empty');
            INSERT INTO documents VALUES ('filed_general', NULL), ('filed_patent', NULL), ('filed_both', NULL), ('unfiled', NULL), ('unindexed', NULL),
                ('attached_to_general', 'general'), ('attached_to_patent', 'patent');
            INSERT INTO text_chunks VALUES ('filed_general'), ('filed_patent'), ('filed_both'), ('unfiled'), ('attached_to_general'), ('attached_to_patent');
            INSERT INTO document_space_memberships VALUES
                ('filed_general', 'space_general'),
                ('filed_patent', 'patent'),
                ('filed_both', 'space_general'), ('filed_both', 'patent'),
                ('unindexed', 'patent');")
        .execute(&pool).await.unwrap();
    ConversationRepository::new(pool)
}

async fn scope_of(repo: &ConversationRepository, conversation: &str) -> HashSet<String> {
    repo.retrieval_document_scope(conversation)
        .await
        .unwrap()
        .unwrap()
        .1
}

/// The reported bug: documents filed into a named space were reachable from
/// a General chat, so a question about one subject was answered with another
/// subject's library.
#[tokio::test]
async fn a_document_filed_into_a_named_space_never_reaches_a_general_chat() {
    let repo = scope_fixture().await;
    let general = scope_of(&repo, "general").await;

    assert!(!general.contains("filed_patent"));
    assert!(general.contains("filed_general"));
}

/// Filing somewhere else must not hide a document from the space it is also
/// filed in: membership is additive, not exclusive.
#[tokio::test]
async fn a_document_in_two_spaces_is_visible_from_both() {
    let repo = scope_fixture().await;

    assert!(scope_of(&repo, "general").await.contains("filed_both"));
    assert!(scope_of(&repo, "patent").await.contains("filed_both"));
}

/// Closing the leak must not make an unassigned import vanish from every
/// search. Documents filed nowhere belong to General.
#[tokio::test]
async fn a_document_filed_nowhere_stays_searchable_from_general_only() {
    let repo = scope_fixture().await;

    assert!(scope_of(&repo, "general").await.contains("unfiled"));
    assert!(!scope_of(&repo, "patent").await.contains("unfiled"));
}

#[tokio::test]
async fn a_named_space_sees_only_its_own_members() {
    let repo = scope_fixture().await;

    // The space itself holds exactly what was filed into it.
    assert_eq!(
        repo.space_document_scope("patent").await.unwrap(),
        ["filed_patent".to_string(), "filed_both".to_string()].into()
    );
    // A chat in that space reaches the same documents plus the files
    // attached to it — which are in no space, and are why the two answers
    // are no longer identical.
    assert_eq!(
        scope_of(&repo, "patent").await,
        [
            "filed_patent".to_string(),
            "filed_both".to_string(),
            "attached_to_patent".to_string(),
        ]
        .into()
    );
}

/// A document with no extracted text cannot be evidence, whatever it is
/// filed under.
#[tokio::test]
async fn a_document_without_searchable_text_is_in_no_scope() {
    let repo = scope_fixture().await;

    assert!(!scope_of(&repo, "patent").await.contains("unindexed"));
}

/// The chat starters ask by space, with no conversation to ask through.
/// Both doors must open on the same room — on the filed documents, which
/// are the only ones a space has.
///
/// The one deliberate difference: a conversation additionally reaches the
/// files attached to it. Those belong to no space, so the space door cannot
/// show them and must not. Take them out and the two answers have to agree
/// exactly, which is what keeps the starters from drifting away from what a
/// chat in that space can actually read.
#[tokio::test]
async fn asking_by_space_gives_the_same_scope_as_asking_through_a_conversation() {
    let repo = scope_fixture().await;

    for (space, conversation, attachment) in [
        ("patent", "patent", "attached_to_patent"),
        ("space_general", "general", "attached_to_general"),
    ] {
        let mut through_conversation = scope_of(&repo, conversation).await;
        assert!(
            through_conversation.remove(attachment),
            "{conversation} should reach its own attachment"
        );
        assert_eq!(
            repo.space_document_scope(space).await.unwrap(),
            through_conversation
        );
    }
}

/// A file attached to one chat is that chat's alone. It is in no space, so
/// nothing else can reach it, and the owning chat can still retrieve from it
/// on every later turn rather than only on the turn that carried it in.
#[tokio::test]
async fn an_attachment_is_visible_only_to_the_conversation_that_owns_it() {
    let repo = scope_fixture().await;

    let general = scope_of(&repo, "general").await;
    assert!(general.contains("attached_to_general"));
    assert!(!general.contains("attached_to_patent"));

    let patent = scope_of(&repo, "patent").await;
    assert!(patent.contains("attached_to_patent"));
    assert!(!patent.contains("attached_to_general"));

    // Not even through the space the owning chat belongs to.
    assert!(!repo
        .space_document_scope("space_general")
        .await
        .unwrap()
        .contains("attached_to_general"));
}

#[tokio::test]
async fn an_empty_space_and_a_missing_conversation_are_distinguishable() {
    let repo = scope_fixture().await;

    assert!(scope_of(&repo, "empty").await.is_empty());
    assert!(repo
        .retrieval_document_scope("missing")
        .await
        .unwrap()
        .is_none());
}

/// The `@` picker reads whole document rows, not just ids, so it needs a
/// documents table with the columns it shows.
async fn picker_fixture() -> ConversationRepository {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::raw_sql(
        "CREATE TABLE documents (id TEXT, file_name TEXT, category TEXT, modified_at TEXT, owner_conversation_id TEXT);
            CREATE TABLE text_chunks (document_id TEXT);
            CREATE TABLE document_space_memberships (document_id TEXT, space_id TEXT);
            INSERT INTO documents VALUES
                ('general_old', 'Halvorsen 2019.pdf', 'PDF Document', '2019-01-01T00:00:00Z', NULL),
                ('general_new', 'HALVORSEN 2024.pdf', 'PDF Document', '2024-01-01T00:00:00Z', NULL),
                ('general_other', 'Thesis outline.md', 'Markdown', '2023-01-01T00:00:00Z', NULL),
                ('filed_movies', 'Halvorsen at the movies.pdf', 'PDF Document', '2026-01-01T00:00:00Z', NULL),
                ('unindexed', 'Halvorsen scan.pdf', 'PDF Document', '2026-06-01T00:00:00Z', NULL),
                ('attached', 'Halvorsen chat attachment.txt', 'Text', '2026-07-01T00:00:00Z', 'some-chat');
            INSERT INTO text_chunks VALUES
                ('general_old'), ('general_new'), ('general_other'), ('filed_movies'), ('attached');
            INSERT INTO document_space_memberships VALUES ('filed_movies', 'movies');",
    )
    .execute(&pool)
    .await
    .unwrap();
    ConversationRepository::new(pool)
}

/// The picker says "these are the documents this chat can use". It has to
/// be the same set retrieval would search, or the user pins a chat to a
/// file it then cannot answer from.
#[tokio::test]
async fn the_picker_offers_only_what_the_space_can_search() {
    let repo = picker_fixture().await;

    let general: Vec<_> = repo
        .space_documents(None, None, "halvorsen", 8)
        .await
        .unwrap()
        .into_iter()
        .map(|document| document.document_id)
        .collect();

    assert_eq!(general, vec!["general_new", "general_old"]);
    assert!(!general.contains(&"filed_movies".to_string()));
    // A file attached to some chat is not a library document, so it is not
    // on offer here either — even though its name matches and it is chunked.
    assert!(!general.contains(&"attached".to_string()));
}

/// Asked on a chat's behalf, the picker offers that chat's own attachments
/// too — otherwise a file you attached on turn one cannot be named on turn
/// three, though retrieval can still read it.
#[tokio::test]
async fn the_picker_offers_the_asking_chat_its_own_attachments() {
    let repo = picker_fixture().await;
    sqlx::raw_sql(
        "CREATE TABLE conversations (id TEXT, space_id TEXT);
             INSERT INTO conversations VALUES ('some-chat', 'space_general'), ('other-chat', 'space_general');",
    )
    .execute(&repo.pool)
    .await
    .unwrap();

    let own: Vec<_> = repo
        .space_documents(None, Some("some-chat"), "halvorsen", 8)
        .await
        .unwrap()
        .into_iter()
        .map(|document| document.document_id)
        .collect();
    assert!(own.contains(&"attached".to_string()));

    let other: Vec<_> = repo
        .space_documents(None, Some("other-chat"), "halvorsen", 8)
        .await
        .unwrap()
        .into_iter()
        .map(|document| document.document_id)
        .collect();
    assert!(!other.contains(&"attached".to_string()));
}

#[tokio::test]
async fn a_named_space_offers_only_its_own_documents() {
    let repo = picker_fixture().await;

    let movies: Vec<_> = repo
        .space_documents(Some("movies"), None, "", 8)
        .await
        .unwrap()
        .into_iter()
        .map(|document| document.document_id)
        .collect();

    assert_eq!(movies, vec!["filed_movies"]);
}

/// Case is not a filter. Typing "HALV" and typing "halv" name the same
/// files.
#[tokio::test]
async fn the_name_match_ignores_case() {
    let repo = picker_fixture().await;

    let upper = repo
        .space_documents(None, None, "HALVORSEN", 8)
        .await
        .unwrap();
    let lower = repo
        .space_documents(None, None, "halvorsen", 8)
        .await
        .unwrap();

    assert_eq!(upper.len(), 2);
    assert_eq!(
        upper.iter().map(|d| &d.document_id).collect::<Vec<_>>(),
        lower.iter().map(|d| &d.document_id).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn an_empty_query_offers_the_whole_space_newest_first() {
    let repo = picker_fixture().await;

    let all: Vec<_> = repo
        .space_documents(None, None, "   ", 8)
        .await
        .unwrap()
        .into_iter()
        .map(|document| document.document_id)
        .collect();

    assert_eq!(all, vec!["general_new", "general_other", "general_old"]);
}

/// A blank space id means General, matching the conversation default.
#[tokio::test]
async fn a_blank_space_is_general() {
    let repo = picker_fixture().await;

    assert_eq!(
        repo.space_documents(Some("   "), None, "halvorsen", 8)
            .await
            .unwrap()
            .len(),
        2
    );
}

/// A document with no extracted text cannot be evidence, so offering it
/// would pin a chat to a file it can never cite.
#[tokio::test]
async fn a_document_without_searchable_text_is_never_offered() {
    let repo = picker_fixture().await;

    assert!(repo
        .space_documents(None, None, "scan", 8)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn the_limit_is_honoured_and_clamped() {
    let repo = picker_fixture().await;

    assert_eq!(
        repo.space_documents(None, None, "", 1).await.unwrap().len(),
        1
    );
    // 5,000 asked for, 50 the most the command will ever return, 3 that
    // exist.
    assert_eq!(
        repo.space_documents(None, None, "", 5_000)
            .await
            .unwrap()
            .len(),
        3
    );
}

#[tokio::test]
async fn an_unknown_space_offers_nothing() {
    let repo = picker_fixture().await;

    assert!(repo
        .space_documents(Some("no-such-space"), None, "", 8)
        .await
        .unwrap()
        .is_empty());
}

/// A search hit learns it was read off a scan from the same lookup that
/// gives it its section and page, which is what the prompt header reads.
#[tokio::test]
async fn a_hit_location_carries_the_ocr_mark() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::raw_sql(
        "CREATE TABLE text_chunks (id TEXT, section TEXT, page_number INTEGER,
                from_ocr BOOLEAN NOT NULL DEFAULT 0);
            INSERT INTO text_chunks VALUES ('scan', 'Intro', 2, 1), ('typed', 'Intro', 3, 0);",
    )
    .execute(&pool)
    .await
    .unwrap();
    let repo = ConversationRepository::new(pool);

    let locations = repo
        .retrieval_locations(&["scan".to_string(), "typed".to_string()])
        .await
        .unwrap();

    assert_eq!(
        locations.get("scan"),
        Some(&(Some("Intro".to_string()), Some(2), true))
    );
    assert_eq!(
        locations.get("typed"),
        Some(&(Some("Intro".to_string()), Some(3), false))
    );
}
