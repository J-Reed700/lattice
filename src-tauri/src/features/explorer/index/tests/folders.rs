//! The folders list: its table against the real migrations, adoption of
//! index directories from before it, what each row says about its index,
//! and removal with and without the folder's threads.

use super::*;
use crate::domain::conversation::MessageRole;
use crate::features::conversation::repository::ConversationRepository;
use crate::features::explorer::{folders, repository};
use crate::shared::AppError;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

async fn database() -> SqlitePool {
    let pool = SqlitePoolOptions::new().connect(":memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

/// A conversation with one message, bound to `root` when given.
async fn thread(pool: &SqlitePool, root: Option<&Path>, title: &str) -> String {
    let conversations = ConversationRepository::new(pool.clone());
    let conversation = conversations
        .create_conversation(title, "model", None)
        .await
        .unwrap();
    let id = conversation.id.to_string();
    conversations
        .add_message(&id, MessageRole::User, "Where does it start?", 4, None)
        .await
        .unwrap();
    if let Some(root) = root {
        repository::set_conversation_root(pool, &id, Some(&text(root)))
            .await
            .unwrap();
    }
    id
}

#[tokio::test]
async fn the_table_lists_pinned_first_then_the_most_recently_opened() {
    let pool = database().await;
    repository::record_folder_opened(&pool, "/w/a", "a", "2026-10-01T10:00:00.000Z")
        .await
        .unwrap();
    repository::record_folder_opened(&pool, "/w/b", "b", "2026-10-01T11:00:00.000Z")
        .await
        .unwrap();
    repository::record_folder_opened(&pool, "/w/c", "c", "2026-10-01T12:00:00.000Z")
        .await
        .unwrap();
    // Opened again: it moves up and keeps its name and when it was added.
    repository::record_folder_opened(&pool, "/w/a", "other", "2026-10-02T09:00:00.000Z")
        .await
        .unwrap();
    repository::set_folder_pinned(&pool, "/w/b", true)
        .await
        .unwrap();

    let rows = repository::list_folders(&pool).await.unwrap();
    let order: Vec<&str> = rows.iter().map(|row| row.root.as_str()).collect();
    assert_eq!(order, vec!["/w/b", "/w/a", "/w/c"]);
    let a = &rows[1];
    assert_eq!(
        (
            a.name.as_str(),
            a.added_at.as_str(),
            a.last_opened_at.as_str()
        ),
        ("a", "2026-10-01T10:00:00.000Z", "2026-10-02T09:00:00.000Z")
    );
    assert!(rows[0].pinned && !a.pinned);

    // Adopting never overrides a row; renaming does.
    repository::adopt_folder(&pool, "/w/a", "adopted", "2020-01-01T00:00:00.000Z")
        .await
        .unwrap();
    repository::rename_folder(&pool, "/w/c", "Client work")
        .await
        .unwrap();
    let names: Vec<String> = repository::list_folders(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.name)
        .collect();
    assert_eq!(names, vec!["b", "a", "Client work"]);
    assert!(matches!(
        repository::rename_folder(&pool, "/w/missing", "x").await,
        Err(AppError::NotFound(_))
    ));
    assert!(matches!(
        repository::set_folder_pinned(&pool, "/w/missing", true).await,
        Err(AppError::NotFound(_))
    ));

    // An empty name goes back to the folder's own.
    folders::rename(&pool, "/w/c", "   ").await.unwrap();
    assert_eq!(repository::list_folders(&pool).await.unwrap()[2].name, "c");

    // Threads are counted per folder; ordinary chats are not.
    thread(&pool, Some(Path::new("/w/a")), "one").await;
    thread(&pool, Some(Path::new("/w/a")), "two").await;
    thread(&pool, None, "chat").await;
    let counts = repository::thread_counts(&pool).await.unwrap();
    assert_eq!(counts.len(), 1);
    assert_eq!(counts.get("/w/a"), Some(&2));
    assert_eq!(
        repository::folder_threads(&pool, "/w/a")
            .await
            .unwrap()
            .len(),
        2
    );

    repository::delete_folder(&pool, "/w/c").await.unwrap();
    assert_eq!(repository::list_folders(&pool).await.unwrap().len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_list_adopts_indexes_without_a_row_and_says_what_each_holds() {
    let rig = Rig::new();
    let pool = database().await;
    let embedder = FakeEmbedder::new("model-a");
    let manager = rig.manager(&embedder);

    // Indexed before the list existed: a directory and no row.
    let old = rig.project(
        "old",
        &[
            ("main.rs", "fn main() {}\n"),
            ("lib.rs", "pub fn lib() {}\n"),
        ],
    );
    open_settled(&manager, &old).await;
    // A folder, and one inside it that reuses its index.
    let parent = rig.project(
        "repo",
        &[
            ("README.md", "# Repo\n"),
            ("app/src/main.rs", "fn main() {}\n"),
        ],
    );
    let child = parent.join("app");
    for root in [&parent, &child] {
        open_settled(&manager, root).await;
        folders::record_open(&pool, &text(root)).await.unwrap();
    }
    // One whose folder has since gone from disk.
    let gone = rig.project("gone", &[("a.rs", "fn a() {}\n")]);
    open_settled(&manager, &gone).await;
    folders::record_open(&pool, &text(&gone)).await.unwrap();
    manager.close().await;
    std::fs::remove_dir_all(&gone).unwrap();
    // The home folder: picked, never indexed.
    folders::record_open(&pool, &text(&rig.home)).await.unwrap();
    thread(&pool, Some(child.as_path()), "about the app").await;

    let listed = folders::list(&pool, &manager).await.unwrap();
    assert_eq!(listed.home, Some(text(&rig.home)));
    assert_eq!(listed.folders.len(), 5);
    let find = |root: &Path| {
        listed
            .folders
            .iter()
            .find(|folder| folder.root == text(root))
            .unwrap()
    };

    let adopted = find(&old);
    assert_eq!(adopted.name, "old");
    assert!(adopted.exists);
    assert_eq!(adopted.index.state, FolderIndexSummaryState::Indexed);
    assert_eq!(
        (
            adopted.index.files_total,
            adopted.index.passages_embedded,
            adopted.index.passages_total
        ),
        (2, 2, 2)
    );
    assert!(adopted.index.bytes > 0);
    assert_eq!(adopted.index.index_root, None);

    let reused = find(&child);
    assert_eq!(reused.index.state, FolderIndexSummaryState::Indexed);
    assert_eq!(reused.index.index_root, Some(text(&parent)));
    assert_eq!(reused.index.bytes, 0, "no index of its own");
    assert_eq!(reused.index.files_total, 1, "counts cover the sub-folder");
    assert_eq!(reused.thread_count, 1);
    assert_eq!(find(&parent).thread_count, 0);

    assert_eq!(
        find(&rig.home).index.state,
        FolderIndexSummaryState::Refused
    );
    assert!(!find(&gone).exists);

    // Listing again adopts nothing twice.
    assert_eq!(
        folders::list(&pool, &manager).await.unwrap().folders.len(),
        5
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_open_folder_lists_live_and_a_stopped_run_as_paused() {
    let rig = Rig::new();
    let pool = database().await;
    let embedder = FakeEmbedder::new("model-a");
    embedder.delay_ms.store(40, Ordering::SeqCst);
    let manager = rig.manager(&embedder);
    let files: Vec<(String, String)> = (0..40)
        .map(|n| {
            (
                format!("src/m{n:02}.rs"),
                format!("pub fn item_{n}() {{}}\n"),
            )
        })
        .collect();
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    let root = rig.project("slow", &refs);
    manager.open(&text(&root)).await.unwrap();
    folders::record_open(&pool, &text(&root)).await.unwrap();
    for _ in 0..500 {
        if embedder.batches.load(Ordering::SeqCst) >= 3 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    let live = folders::list(&pool, &manager).await.unwrap().folders;
    assert_eq!(live[0].index.state, FolderIndexSummaryState::Indexing);
    assert_eq!(live[0].index.passages_total, 40);

    manager.close().await;
    let paused = folders::list(&pool, &manager)
        .await
        .unwrap()
        .folders
        .remove(0)
        .index;
    assert_eq!(paused.state, FolderIndexSummaryState::Partial);
    assert!(
        paused.passages_embedded > 0 && paused.passages_embedded < paused.passages_total,
        "{paused:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn removing_a_folder_keeps_its_threads_unless_asked() {
    let container = crate::tests::common::setup_test_container().await.unwrap();
    let pool = container.db_pool().clone();
    let rig = Rig::new();
    let embedder = FakeEmbedder::new("model-a");
    let manager = rig.manager(&embedder);
    let root = rig.project("music", &[("synth.rs", "pub fn voice() {}\n")]);
    open_settled(&manager, &root).await;
    folders::record_open(&pool, &text(&root)).await.unwrap();
    let voices = thread(&pool, Some(root.as_path()), "voices").await;
    let filters = thread(&pool, Some(root.as_path()), "filters").await;
    let chat = thread(&pool, None, "ordinary").await;

    // Kept: the index and the row go, closed first; the threads stay bound.
    let deleted = folders::remove(&container, &manager, &text(&root), false)
        .await
        .unwrap();
    assert_eq!(deleted, 0);
    assert!(rig.index_dirs().is_empty());
    assert!(manager.search_for(&root).await.is_none());
    assert!(folders::list(&pool, &manager)
        .await
        .unwrap()
        .folders
        .is_empty());
    assert_eq!(
        repository::folder_threads(&pool, &text(&root))
            .await
            .unwrap()
            .len(),
        2
    );

    // Added again, they are back.
    folders::record_open(&pool, &text(&root)).await.unwrap();
    assert_eq!(
        folders::list(&pool, &manager).await.unwrap().folders[0].thread_count,
        2
    );

    // Deleted: through the conversation delete, messages and all.
    let deleted = folders::remove(&container, &manager, &text(&root), true)
        .await
        .unwrap();
    assert_eq!(deleted, 2);
    assert!(repository::folder_threads(&pool, &text(&root))
        .await
        .unwrap()
        .is_empty());
    for id in [&voices, &filters] {
        let messages: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM conversation_messages WHERE conversation_id = ?",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(messages, 0, "{id}");
    }
    let kept: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM conversations WHERE id = ?")
        .bind(&chat)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(kept, 1, "an ordinary chat is not the folder's");
    assert!(folders::list(&pool, &manager)
        .await
        .unwrap()
        .folders
        .is_empty());
}

async fn space_of(pool: &SqlitePool, conversation_id: &str) -> String {
    sqlx::query_scalar("SELECT space_id FROM conversations WHERE id = ?")
        .bind(conversation_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn add_space(pool: &SqlitePool, id: &str, archived: bool) {
    sqlx::query("INSERT INTO conversation_spaces (id, name, is_archived) VALUES (?, ?, ?)")
        .bind(id)
        .bind(id)
        .bind(i64::from(archived))
        .execute(pool)
        .await
        .unwrap();
}

/// A folder's space and instructions are its own, its threads follow its
/// space, and General is stored as no space at all.
#[tokio::test]
async fn a_folders_settings_carry_its_threads_into_its_space() {
    let pool = database().await;
    add_space(&pool, "space_music", false).await;
    let root = "/Users/me/MusicVST";
    folders::record_open(&pool, root).await.unwrap();
    let first = thread(&pool, None, "First").await;
    folders::bind_thread(&pool, &first, Some(root))
        .await
        .unwrap();
    assert_eq!(space_of(&pool, &first).await, "space_general");

    let moved = folders::set_settings(&pool, root, "  JUCE, C++20.  ", "space_music")
        .await
        .unwrap();
    assert_eq!(moved, 1);
    assert_eq!(space_of(&pool, &first).await, "space_music");
    let row = repository::list_folders(&pool).await.unwrap().remove(0);
    assert_eq!(row.instructions.as_deref(), Some("JUCE, C++20."));
    assert_eq!(row.space_id.as_deref(), Some("space_music"));

    // A thread started while the Chat sidebar had another space selected
    // still lands in the folder's.
    let second = thread(&pool, None, "Second").await;
    folders::bind_thread(&pool, &second, Some(root))
        .await
        .unwrap();
    assert_eq!(space_of(&pool, &second).await, "space_music");

    let moved = folders::set_settings(&pool, root, "   ", "space_general")
        .await
        .unwrap();
    assert_eq!(moved, 2);
    let row = repository::list_folders(&pool).await.unwrap().remove(0);
    assert_eq!(row.instructions, None);
    assert_eq!(row.space_id, None);
    assert_eq!(space_of(&pool, &second).await, "space_general");
}

#[tokio::test]
async fn a_folder_cannot_take_a_missing_or_archived_space() {
    let pool = database().await;
    add_space(&pool, "space_old", true).await;
    let root = "/Users/me/project";
    folders::record_open(&pool, root).await.unwrap();
    for space in ["space_nowhere", "space_old"] {
        assert!(matches!(
            folders::set_settings(&pool, root, "", space).await,
            Err(AppError::NotFound(_))
        ));
    }
    assert!(matches!(
        folders::set_settings(&pool, root, &"x".repeat(20_001), "space_general").await,
        Err(AppError::InvalidInput(_))
    ));
    assert!(matches!(
        folders::set_settings(&pool, "/Users/me/unlisted", "", "space_general").await,
        Err(AppError::NotFound(_))
    ));
}

async fn last_thread(pool: &SqlitePool, root: &str) -> Option<String> {
    repository::list_folders(pool)
        .await
        .unwrap()
        .into_iter()
        .find(|row| row.root == root)
        .and_then(|row| row.last_thread_id)
}

/// A folder reopens on the thread it last showed, kept on its row; a thread
/// of another folder is refused, and deleting the thread forgets it.
#[tokio::test]
async fn a_folder_remembers_its_last_thread_until_the_thread_is_deleted() {
    let pool = database().await;
    let root = "/Users/me/notes";
    let first = thread(&pool, Some(Path::new(root)), "First").await;
    let second = thread(&pool, Some(Path::new(root)), "Second").await;
    let elsewhere = thread(&pool, Some(Path::new("/Users/me/other")), "Other").await;
    let chat = thread(&pool, None, "Chat").await;

    // A thread used before the open was recorded lists the folder.
    folders::remember_thread(&pool, root, &first).await.unwrap();
    assert_eq!(
        last_thread(&pool, root).await.as_deref(),
        Some(first.as_str())
    );
    assert_eq!(
        repository::list_folders(&pool).await.unwrap()[0].name,
        "notes"
    );

    // Opening again keeps it; using another thread replaces it.
    folders::record_open(&pool, root).await.unwrap();
    assert_eq!(
        last_thread(&pool, root).await.as_deref(),
        Some(first.as_str())
    );
    folders::remember_thread(&pool, root, &second)
        .await
        .unwrap();
    assert_eq!(
        last_thread(&pool, root).await.as_deref(),
        Some(second.as_str())
    );

    for foreign in [&elsewhere, &chat] {
        assert!(matches!(
            folders::remember_thread(&pool, root, foreign).await,
            Err(AppError::InvalidInput(_))
        ));
    }
    assert_eq!(
        last_thread(&pool, root).await.as_deref(),
        Some(second.as_str())
    );

    sqlx::query("DELETE FROM conversations WHERE id = ?")
        .bind(&second)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(last_thread(&pool, root).await, None);
}
