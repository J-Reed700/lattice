use super::build::{self, Embedding, Progress};
use super::chunking::spans::span_chunks;
use super::chunking::NewPassage;
use super::live::LiveIndexes;
use super::registry;
use super::store::{PassageStore, DB_FILE, META_IDENTITY};
use super::text_search::{match_expression, PassageTextSearch};
use super::vectors::{vectors_path, Vectors};
use async_trait::async_trait;
use lattice_core::application::ports::{EmbeddingPort, TextSearchPort};
use lattice_core::shared::Result;
use parking_lot::Mutex;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

const DIM: usize = 8;

/// Bag-of-words vectors, and every text it was asked to embed.
struct Words {
    identity: String,
    embedded: Mutex<Vec<String>>,
}

impl Words {
    fn new(identity: &str) -> Arc<Self> {
        Arc::new(Self {
            identity: identity.to_string(),
            embedded: Mutex::new(Vec::new()),
        })
    }

    fn vector(text: &str) -> Vec<f32> {
        let mut vector = [0.01f32; DIM];
        for word in text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
        {
            let bucket = word.bytes().fold(7usize, |hash, byte| {
                hash.wrapping_mul(31).wrapping_add(byte as usize)
            }) % DIM;
            vector[bucket] += 1.0;
        }
        let norm = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        vector.iter().map(|x| x / norm).collect()
    }
}

#[async_trait]
impl EmbeddingPort for Words {
    async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        Ok(Self::vector(text))
    }
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        self.embedded.lock().extend(texts.iter().cloned());
        Ok(texts.iter().map(|text| Self::vector(text)).collect())
    }
    fn dimension(&self) -> usize {
        DIM
    }
    fn model_identity(&self) -> String {
        self.identity.clone()
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

fn passage(start: usize, end: usize, text: &str) -> NewPassage {
    NewPassage {
        start,
        end,
        text: text.to_string(),
    }
}

/// A store in a fresh directory, with `documents` each holding the passages
/// given as texts.
async fn store_with(dir: &Path, documents: &[(&str, &[&str])]) -> PassageStore {
    let store = PassageStore::open(dir).await.unwrap();
    for (key, texts) in documents {
        let passages: Vec<NewPassage> = texts
            .iter()
            .enumerate()
            .map(|(at, text)| passage(at, at + 1, text))
            .collect();
        store
            .replace(key, "stamp", "hash", &passages)
            .await
            .unwrap();
    }
    store
}

fn set(keys: &[&str]) -> HashSet<String> {
    keys.iter().map(|key| key.to_string()).collect()
}

mod store {
    use super::*;
    use lattice_core::shared::runtime::jobs::JobFailure;

    #[test]
    fn a_busy_store_makes_its_job_wait_rather_than_fail() {
        let busy = super::super::store::db_error("read documents")(sqlx::Error::PoolTimedOut);
        assert!(JobFailure::is_transient(&busy));
        let broken = super::super::store::db_error("read documents")(sqlx::Error::RowNotFound);
        assert!(!JobFailure::is_transient(&broken));
    }

    #[tokio::test]
    async fn a_rebuild_removes_only_the_index_files() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("chunks.db"), b"an earlier layout").unwrap();
        std::fs::write(vectors_path(temp.path(), "model-a"), b"old vectors").unwrap();
        std::fs::write(temp.path().join("notes.txt"), b"not the index's").unwrap();
        let _store = PassageStore::open(temp.path()).await.unwrap();
        assert!(!temp.path().join("chunks.db").exists());
        assert!(!vectors_path(temp.path(), "model-a").exists());
        assert!(
            temp.path().join("notes.txt").exists(),
            "a file the index did not write survives a rebuild"
        );
    }

    #[tokio::test]
    async fn replace_returns_what_it_replaced_and_marks_for_embedding() {
        let temp = tempfile::tempdir().unwrap();
        let store = PassageStore::open(temp.path()).await.unwrap();
        let first = store
            .replace(
                "a.md",
                "1:1",
                "h1",
                &[passage(1, 3, "alpha"), passage(4, 6, "beta")],
            )
            .await
            .unwrap();
        assert!(first.is_empty());
        let rows = store.passages_of("a.md").await.unwrap();
        assert_eq!(
            rows.iter()
                .map(|row| (row.start, row.end, row.text.as_str()))
                .collect::<Vec<_>>(),
            vec![(1, 3, "alpha"), (4, 6, "beta")]
        );
        assert_eq!(store.pending("").await.unwrap(), vec!["a.md"]);

        let mut replaced = store
            .replace("a.md", "2:2", "h2", &[passage(1, 2, "gamma")])
            .await
            .unwrap();
        replaced.sort();
        let mut old: Vec<i64> = rows.iter().map(|row| row.id).collect();
        old.sort();
        assert_eq!(replaced, old);
        let new = store.passages_of("a.md").await.unwrap();
        assert!(new[0].id > old[1], "ids are never handed out twice");
        let document = &store.documents("").await.unwrap()["a.md"];
        assert_eq!(
            (document.stamp.as_str(), document.content_hash.as_str()),
            ("2:2", "h2")
        );

        // A document with no passages has nothing to embed.
        store.replace("empty.md", "0:0", "h", &[]).await.unwrap();
        assert_eq!(store.pending("").await.unwrap(), vec!["a.md"]);
        store.mark_embedded(&["a.md".to_string()]).await.unwrap();
        assert!(store.pending("").await.unwrap().is_empty());
        store.reset_embedded().await.unwrap();
        assert_eq!(store.pending("").await.unwrap(), vec!["a.md"]);
        store.touch("a.md", "3:3").await.unwrap();
        assert_eq!(store.documents("a.md").await.unwrap()["a.md"].stamp, "3:3");
        store.close().await;
    }

    #[tokio::test]
    async fn a_prefix_takes_a_folder_and_not_its_namesake() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_with(
            temp.path(),
            &[
                ("src/a.rs", &["one"]),
                ("src/b/c.rs", &["two"]),
                ("src2/d.rs", &["three"]),
            ],
        )
        .await;
        assert_eq!(
            store.keys_under("src").await.unwrap(),
            vec!["src/a.rs", "src/b/c.rs"]
        );
        assert_eq!(
            store.keys_under("src/a.rs").await.unwrap(),
            vec!["src/a.rs"]
        );
        assert_eq!(store.keys_under("").await.unwrap().len(), 3);
        let counts = store.counts("src").await.unwrap();
        assert_eq!((counts.documents_total, counts.passages_total), (2, 2));
        assert_eq!(counts.passages_embedded, 0);
        assert!(store.has_passages("src2").await.unwrap());
        assert!(!store.has_passages("docs").await.unwrap());

        let removed = store
            .remove(&store.keys_under("src").await.unwrap())
            .await
            .unwrap();
        assert_eq!(removed.len(), 2);
        assert_eq!(store.keys_under("").await.unwrap(), vec!["src2/d.rs"]);
        let found = store.full_text("\"one\"", None, 10).await.unwrap();
        assert!(found.is_empty(), "full text follows the passages out");
        store.close().await;
    }

    #[tokio::test]
    async fn full_text_keeps_to_its_scope_and_counts_terms_by_passage() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_with(
            temp.path(),
            &[
                ("a.md", &["retry with backoff", "nothing here"]),
                ("b.md", &["retry retry retry"]),
            ],
        )
        .await;
        let everywhere = store.full_text("\"retry\"", None, 10).await.unwrap();
        assert_eq!(everywhere.len(), 2);
        assert!(
            everywhere.windows(2).all(|pair| pair[0].1 <= pair[1].1),
            "best bm25 first"
        );
        let rows = store
            .by_ids(&everywhere.iter().map(|(id, _)| *id).collect::<Vec<_>>())
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);

        let scoped = store
            .full_text("\"retry\"", Some(&set(&["a.md"])), 10)
            .await
            .unwrap();
        assert_eq!(scoped.len(), 1);
        let row = &store.by_ids(&[scoped[0].0]).await.unwrap()[0];
        assert_eq!(row.document, "a.md");
        let none = store
            .full_text("\"retry\"", Some(&HashSet::new()), 10)
            .await
            .unwrap();
        assert!(none.is_empty(), "an empty scope holds nothing");
        assert_eq!(
            store.full_text("\"retry\"", None, 1).await.unwrap().len(),
            1
        );

        let terms = store
            .term_counts(&[
                "retry".to_string(),
                "nothing".to_string(),
                "absent".to_string(),
            ])
            .await
            .unwrap();
        assert_eq!(terms.get("retry"), Some(&2), "passages, not occurrences");
        assert_eq!(terms.get("nothing"), Some(&1));
        assert_eq!(terms.get("absent"), None);
        store.close().await;
    }

    #[tokio::test]
    async fn another_schema_version_rebuilds_from_an_empty_directory() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_with(temp.path(), &[("a.md", &["alpha"])]).await;
        store.set_meta("version", "1").await.unwrap();
        store.set_meta(META_IDENTITY, "model-a").await.unwrap();
        store.close().await;
        std::fs::write(vectors_path(temp.path(), "model-a"), b"old vectors").unwrap();

        let store = PassageStore::open(temp.path()).await.unwrap();
        assert!(store.keys_under("").await.unwrap().is_empty());
        assert_eq!(store.meta(META_IDENTITY).await.unwrap(), None);
        assert!(!vectors_path(temp.path(), "model-a").exists());
        store.close().await;

        // Same version: nothing is touched.
        let store = store_with(temp.path(), &[("b.md", &["beta"])]).await;
        store.close().await;
        let store = PassageStore::open(temp.path()).await.unwrap();
        assert_eq!(store.keys_under("").await.unwrap(), vec!["b.md"]);
        store.close().await;
    }

    #[tokio::test]
    async fn a_directory_without_a_store_is_emptied_first() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("index");
        std::fs::create_dir_all(dir.join("kept")).unwrap();
        std::fs::write(dir.join("chunks.db"), b"an earlier layout").unwrap();
        std::fs::write(vectors_path(&dir, "model-a"), b"vectors of other ids").unwrap();
        let store = PassageStore::open(&dir).await.unwrap();
        assert!(dir.join(DB_FILE).exists());
        assert!(!dir.join("chunks.db").exists());
        assert!(!vectors_path(&dir, "model-a").exists());
        assert!(dir.join("kept").is_dir(), "only files directly in it go");
        store.close().await;
    }
}

mod chunking {
    use super::*;

    /// Every passage is its text's bytes, starts on a sentence, and
    /// consecutive passages leave nothing but whitespace between them.
    fn assert_spans(text: &str, passages: &[NewPassage], chunk_chars: usize) {
        assert!(!passages.is_empty());
        let mut covered = vec![false; text.len()];
        for passage in passages {
            assert_eq!(&text[passage.start..passage.end], passage.text);
            assert!(
                passage.text.chars().count() <= chunk_chars,
                "{:?}",
                passage.text
            );
            assert_eq!(passage.text.trim(), passage.text);
            covered[passage.start..passage.end].fill(true);
        }
        for pair in passages.windows(2) {
            assert!(pair[1].start > pair[0].start);
            assert!(text[pair[0].end..pair[1].start.max(pair[0].end)]
                .trim()
                .is_empty());
        }
        for (at, c) in text.char_indices() {
            assert!(c.is_whitespace() || covered[at], "byte {at} left out");
        }
    }

    #[test]
    fn passages_are_whole_sentences_that_overlap_and_cover_the_text() {
        let text = "The first sentence is here. A second one follows it! Is this the third? \
                    Yes, and a fourth. Then a fifth sentence closes the paragraph.\n\
                    A heading line\nAnd the last sentence of all.";
        let passages = span_chunks(text, 60, 40);
        assert_spans(text, &passages, 60);
        for passage in &passages {
            let last = passage.text.chars().last().unwrap();
            assert!(matches!(last, '.' | '!' | '?' | 'e'), "{:?}", passage.text);
            let before = text[..passage.start].trim_end();
            assert!(
                before.is_empty() || before.ends_with(['.', '!', '?', 'e']),
                "starts mid-sentence: {:?}",
                passage.text
            );
        }
        assert!(
            passages.windows(2).any(|pair| pair[1].start < pair[0].end),
            "neighbours overlap by whole sentences"
        );
    }

    #[test]
    fn a_full_stop_inside_a_number_ends_nothing() {
        let passages = span_chunks("Pi is 3.14 today. Next one.", 20, 10);
        let texts: Vec<&str> = passages.iter().map(|p| p.text.as_str()).collect();
        assert_eq!(texts, vec!["Pi is 3.14 today.", "Next one."]);
    }

    #[test]
    fn a_run_on_longer_than_the_window_is_cut_at_its_edge() {
        let text = format!("{}. Tail.", "x".repeat(100));
        let passages = span_chunks(&text, 40, 30);
        assert_spans(&text, &passages, 40);
        assert_eq!(passages[0].text.len(), 40);
        assert!(passages.last().unwrap().text.ends_with("x. Tail."));
    }

    #[test]
    fn offsets_are_bytes_on_character_boundaries() {
        let text = "Ça va très bien. Où est-il? 日本語の文。次の文。終わり。";
        let passages = span_chunks(text, 12, 8);
        assert_spans(text, &passages, 12);
        assert!(
            passages
                .iter()
                .any(|passage| passage.text == "次の文。終わり。"),
            "a full-width stop ends a sentence with no space after it: {passages:?}"
        );
        assert!(span_chunks("   \n  ", 10, 5).is_empty());
    }
}

mod directories {
    use super::*;

    #[tokio::test]
    async fn the_registry_reads_meta_and_counts_without_opening() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path();
        let store = store_with(&base.join("one"), &[("src/a.rs", &["alpha", "beta"])]).await;
        store.set_meta("root", "/somewhere").await.unwrap();
        std::fs::create_dir_all(base.join("two")).unwrap();
        std::fs::write(base.join("stray.txt"), b"not an index").unwrap();

        let mut entries = registry::list(base).await;
        entries.sort_by(|a, b| a.dir.cmp(&b.dir));
        assert_eq!(entries.len(), 2, "directories only");
        assert_eq!(
            entries[0].meta.get("root").map(String::as_str),
            Some("/somewhere")
        );
        assert!(entries[1].meta.is_empty(), "no store, no meta");

        let inspected = registry::inspect(&base.join("one"), "src")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(inspected.counts.passages_total, 2);
        assert!(inspected.meta.contains_key("root"));
        assert!(registry::inspect(&base.join("two"), "")
            .await
            .unwrap()
            .is_none());
        assert!(registry::dir_bytes(&base.join("one")) > 0);
        assert_eq!(registry::dir_bytes(&base.join("missing")), 0);
        store.close().await;
    }
}

mod text_search {
    use super::*;

    #[test]
    fn full_text_queries_are_quoted_literals() {
        assert_eq!(
            match_expression("where is \"retry\" NEAR(x)?").as_deref(),
            Some("\"where\"* OR \"is\" OR \"retry\"* OR \"near\"*")
        );
        assert_eq!(match_expression("a ! ?"), None);
        assert_eq!(
            match_expression("handle_retry").as_deref(),
            Some("\"handle_retry\"*")
        );
    }

    #[tokio::test]
    async fn records_carry_their_document_and_writes_are_refused() {
        let temp = tempfile::tempdir().unwrap();
        let store = Arc::new(
            store_with(
                temp.path(),
                &[
                    ("a.md", &["retry with backoff"]),
                    ("b.md", &["retry retry"]),
                ],
            )
            .await,
        );
        let search = PassageTextSearch::new(Arc::clone(&store));
        let all = search.search_scoped("retry", 10, None, None).await.unwrap();
        assert_eq!(all.len(), 2);
        assert!((all[0].score - 1.0).abs() < 1e-6, "the best scores one");
        assert!(all
            .iter()
            .all(|record| record.chunk_id.parse::<i64>().is_ok()));
        let scoped = search
            .search_scoped("retry", 10, Some("ignored"), Some(&set(&["a.md"])))
            .await
            .unwrap();
        assert_eq!(scoped.len(), 1);
        assert_eq!(scoped[0].doc_id, "a.md");
        assert_eq!(scoped[0].content, "retry with backoff");
        assert!(search
            .search_scoped("retry", 10, None, Some(&HashSet::new()))
            .await
            .unwrap()
            .is_empty());
        assert!(search.search("! ?", 10).await.unwrap().is_empty());
        assert_eq!(search.count().await.unwrap(), 2);
        assert!(search.index_document("x", "y").await.is_err());
        assert!(search.clear().await.is_err());
        store.close().await;
    }
}

mod embedding {
    use super::*;

    #[derive(Default)]
    struct Told {
        started: Option<usize>,
        embedded: Vec<usize>,
        saved: Vec<usize>,
    }

    impl Progress for Told {
        fn started(&mut self, documents: usize) {
            self.started = Some(documents);
        }
        fn embedded(&mut self, count: usize) {
            self.embedded.push(count);
        }
        fn saved(&mut self, documents: usize) {
            self.saved.push(documents);
        }
    }

    fn embedding(words: &Arc<Words>) -> Embedding {
        Embedding {
            embedder: Arc::clone(words) as Arc<dyn EmbeddingPort>,
            batch_size: 2,
            save_every: 2,
            embed_text: Arc::new(|passage| format!("{}\n{}", passage.document, passage.text)),
        }
    }

    #[tokio::test]
    async fn documents_are_marked_at_saves_and_a_second_round_has_nothing_to_do() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_with(
            temp.path(),
            &[
                ("a.md", &["alpha one", "alpha two", "alpha three"]),
                ("b.md", &["beta"]),
                ("c.md", &[]),
            ],
        )
        .await;
        let words = Words::new("model-a");
        let vectors = Vectors::open(&store, "model-a", DIM).await.unwrap();
        let mut told = Told::default();
        let round = build::embed_pending(
            &store,
            &vectors,
            &embedding(&words),
            "",
            &CancellationToken::new(),
            &mut told,
        )
        .await
        .unwrap();
        assert_eq!(round.passages, 4);
        assert!(!round.cancelled);
        assert_eq!(
            told.started,
            Some(2),
            "the empty document was never pending"
        );
        assert_eq!(told.embedded, vec![2, 2]);
        assert_eq!(told.saved.iter().sum::<usize>(), 2);
        assert_eq!(vectors.count(), 4);
        assert!(vectors_path(temp.path(), "model-a").exists(), "saved");
        assert!(store.pending("").await.unwrap().is_empty());
        assert!(
            words
                .embedded
                .lock()
                .iter()
                .any(|text| text == "a.md\nalpha one"),
            "the owner's embed text is what the model reads"
        );

        let mut again = Told::default();
        let round = build::embed_pending(
            &store,
            &vectors,
            &embedding(&words),
            "",
            &CancellationToken::new(),
            &mut again,
        )
        .await
        .unwrap();
        assert_eq!(round.passages, 0);
        assert_eq!(again.started, None);
        store.close().await;
    }

    #[tokio::test]
    async fn a_cancelled_round_leaves_its_documents_pending() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_with(temp.path(), &[("a.md", &["alpha"]), ("b.md", &["beta"])]).await;
        let words = Words::new("model-a");
        let vectors = Vectors::open(&store, "model-a", DIM).await.unwrap();
        let cancel = CancellationToken::new();
        cancel.cancel();
        let round = build::embed_pending(
            &store,
            &vectors,
            &embedding(&words),
            "",
            &cancel,
            &mut Told::default(),
        )
        .await
        .unwrap();
        assert!(round.cancelled);
        assert_eq!(round.passages, 0);
        assert_eq!(store.pending("").await.unwrap(), vec!["a.md", "b.md"]);
        store.close().await;
    }

    #[tokio::test]
    async fn a_new_identity_drops_the_vectors_and_keeps_the_passages() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_with(temp.path(), &[("a.md", &["alpha"]), ("b.md", &["beta"])]).await;
        let first = Words::new("model-a");
        let vectors = Vectors::open(&store, "model-a", DIM).await.unwrap();
        build::embed_pending(
            &store,
            &vectors,
            &embedding(&first),
            "",
            &CancellationToken::new(),
            &mut Told::default(),
        )
        .await
        .unwrap();
        assert!(store.pending("").await.unwrap().is_empty());

        let again = Vectors::open(&store, "model-a", DIM).await.unwrap();
        assert_eq!(again.count(), 2, "the same model keeps its vectors");
        assert!(store.pending("").await.unwrap().is_empty());

        let other = Vectors::open(&store, "model:b/2", DIM).await.unwrap();
        assert_eq!(other.count(), 0);
        assert!(!vectors_path(temp.path(), "model-a").exists());
        assert_eq!(store.pending("").await.unwrap(), vec!["a.md", "b.md"]);
        assert_eq!(store.counts("").await.unwrap().passages_total, 2);
        assert_eq!(
            store.meta(META_IDENTITY).await.unwrap().as_deref(),
            Some("model:b/2")
        );
        store.close().await;
    }
}

mod live {
    use super::*;

    #[tokio::test]
    async fn a_directory_is_opened_once_and_closed_by_its_last_user() {
        let temp = tempfile::tempdir().unwrap();
        let live = LiveIndexes::default();
        let first = live.acquire(temp.path()).await.unwrap();
        let second = live.acquire(temp.path()).await.unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert!(first.engine().is_none(), "nothing prepared yet");
        let engine = first.prepare(Words::new("model-a")).await.unwrap();
        assert_eq!(engine.vectors.count(), 0);
        assert!(second.engine().is_some(), "users share the engine");

        live.release(&first).await;
        assert!(second.store.meta("anything").await.is_ok(), "still held");
        live.release(&second).await;
        assert!(second.store.meta("anything").await.is_err(), "pool closed");
        let reopened = live.acquire(temp.path()).await.unwrap();
        assert!(!Arc::ptr_eq(&reopened, &first));
        live.release(&reopened).await;
    }
}
