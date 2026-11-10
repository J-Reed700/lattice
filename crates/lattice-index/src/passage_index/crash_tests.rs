use super::*;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::SqlitePool;

struct Quiet;

impl Progress for Quiet {
    fn embedded(&mut self, _count: usize) {}
    fn saved(&mut self, _documents: usize) {}
}

fn embedding(words: &Arc<Words>) -> Embedding {
    Embedding {
        embedder: Arc::clone(words) as Arc<dyn EmbeddingPort>,
        batch_size: 2,
        save_every: 2,
        embed_text: Arc::new(|passage| format!("{}\n{}", passage.document, passage.text)),
        turns: Arc::default(),
    }
}

async fn side_door(dir: &Path) -> SqlitePool {
    SqlitePool::connect_with(SqliteConnectOptions::new().filename(dir.join(DB_FILE)))
        .await
        .unwrap()
}

#[tokio::test]
async fn a_failed_mark_after_the_vectors_save_leaves_the_documents_pending_and_the_next_round_marks_them(
) {
    let temp = tempfile::tempdir().unwrap();
    let store = store_with(temp.path(), &[("a.md", &["alpha"]), ("b.md", &["beta"])]).await;
    let words = Words::new("model-a");
    let vectors = Vectors::open(&store, "model-a", DIM).await.unwrap();
    let door = side_door(temp.path()).await;
    sqlx::query(
        "CREATE TRIGGER refuse_marks BEFORE UPDATE OF embedded ON index_documents
         BEGIN SELECT RAISE(ABORT, 'injected'); END",
    )
    .execute(&door)
    .await
    .unwrap();

    let crashed = build::embed_pending(
        &store,
        &vectors,
        &embedding(&words),
        "",
        &CancellationToken::new(),
        &mut Quiet,
    )
    .await;
    assert!(crashed.is_err(), "the mark is refused");
    assert!(
        vectors_path(temp.path(), "model-a").exists(),
        "the vectors file is written before any document is marked"
    );
    assert_eq!(
        store.pending("").await.unwrap(),
        vec!["a.md", "b.md"],
        "no document is marked embedded that the store did not commit"
    );

    sqlx::query("DROP TRIGGER refuse_marks")
        .execute(&door)
        .await
        .unwrap();
    build::embed_pending(
        &store,
        &vectors,
        &embedding(&words),
        "",
        &CancellationToken::new(),
        &mut Quiet,
    )
    .await
    .unwrap();
    assert!(
        store.pending("").await.unwrap().is_empty(),
        "the next round repairs it"
    );
    door.close().await;
    store.close().await;
}
