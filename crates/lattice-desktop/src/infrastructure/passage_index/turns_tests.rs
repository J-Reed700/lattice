#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
//! The turn order: who waits for whom, and an embedding run that holds at
//! its next batch while an index ranked above it embeds, then carries on.

use super::build::{self, Embedding, Progress};
use super::chunking::NewPassage;
use super::store::PassageStore;
use super::testing::{FakeEmbedder, DIM};
use super::turns::{Claim, EmbedTurns, Priority, Running, Standing};
use super::vectors::Vectors;
use lattice_core::application::ports::EmbeddingPort;
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

fn dir(name: &str) -> PathBuf {
    PathBuf::from("/indexes").join(name)
}

#[test]
fn a_claim_alone_holds_nobody_back() {
    let turns = Arc::new(EmbedTurns::default());
    let _course = turns.claim(&dir("course"), Priority::Awaited);
    let _folder = turns.running(&dir("folder"));
    assert!(!turns.behind(&dir("folder")));
}

#[test]
fn an_index_waits_only_while_one_ranked_above_it_embeds() {
    let turns = Arc::new(EmbedTurns::default());
    let (course, open, other) = (dir("course"), dir("open-folder"), dir("other"));
    let awaited = turns.claim(&course, Priority::Awaited);
    let _opened = turns.claim(&open, Priority::Open);
    let embedding = turns.running(&course);

    // Below an awaited course: an open folder and an index nobody claims.
    assert!(turns.behind(&open));
    assert!(turns.behind(&other));
    assert!(!turns.behind(&course));
    // Another awaited index shares the model with it.
    let lesson = turns.claim(&other, Priority::Awaited);
    assert!(!turns.behind(&other));
    drop(lesson);

    // The course stops being waited on: it ranks with the rest.
    drop(awaited);
    assert!(!turns.behind(&open));
    assert!(!turns.behind(&other));
    drop(embedding);

    // An open folder embedding goes ahead of every index nobody claims.
    let _folder = turns.running(&open);
    assert!(turns.behind(&other));
    assert!(turns.behind(&course));
    assert!(!turns.behind(&open));
}

#[test]
fn standing_names_the_indexes_each_run_waits_for() {
    let turns = Arc::new(EmbedTurns::default());
    let (course, open, other) = (dir("course"), dir("open-folder"), dir("other"));
    let mut changes = turns.changes();
    let _awaited = turns.claim(&course, Priority::Awaited);
    let _opened = turns.claim(&open, Priority::Open);
    assert!(changes.has_changed().unwrap());
    changes.mark_unchanged();
    let _course = turns.running(&course);
    let _folder = turns.running(&open);
    let _other = turns.running(&other);
    assert!(changes.has_changed().unwrap());

    assert_eq!(
        turns.standing(),
        vec![
            Standing {
                dir: course.clone(),
                ahead: vec![],
            },
            Standing {
                dir: open.clone(),
                ahead: vec![course.clone()],
            },
            Standing {
                dir: other,
                ahead: vec![course, open],
            },
        ]
    );
}

#[tokio::test]
async fn a_waiting_run_carries_on_when_the_index_ahead_finishes() {
    let turns = Arc::new(EmbedTurns::default());
    let _awaited = turns.claim(&dir("course"), Priority::Awaited);
    let embedding = turns.running(&dir("course"));
    let waiting = {
        let turns = Arc::clone(&turns);
        tokio::spawn(async move {
            turns
                .wait_turn(&dir("folder"), &CancellationToken::new())
                .await
        })
    };
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
    assert!(!waiting.is_finished());
    drop(embedding);
    assert!(waiting.await.unwrap());
}

#[tokio::test]
async fn a_waiting_run_stops_when_cancelled() {
    let turns = Arc::new(EmbedTurns::default());
    let _awaited = turns.claim(&dir("course"), Priority::Awaited);
    let _embedding = turns.running(&dir("course"));
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(!turns.wait_turn(&dir("folder"), &cancel).await);
}

/// What the loop told, and a signal when it paused.
#[derive(Default)]
struct Told {
    embedded: Vec<usize>,
    paused: Vec<bool>,
}

struct Watch {
    told: Arc<Mutex<Told>>,
    pausing: Arc<Notify>,
    /// Runs after the first batch: someone starts waiting on another index.
    after_first: Option<Box<dyn FnOnce() + Send>>,
}

impl Progress for Watch {
    fn embedded(&mut self, count: usize) {
        self.told.lock().embedded.push(count);
        if let Some(then) = self.after_first.take() {
            then();
        }
    }
    fn saved(&mut self, _documents: usize) {}
    fn paused(&mut self, paused: bool) {
        self.told.lock().paused.push(paused);
        if paused {
            self.pausing.notify_one();
        }
    }
}

async fn store_of(dir: &Path, passages: usize) -> PassageStore {
    let store = PassageStore::open(dir).await.unwrap();
    let passages: Vec<NewPassage> = (0..passages)
        .map(|i| NewPassage {
            start: i,
            end: i + 1,
            text: format!("passage number {i}"),
        })
        .collect();
    store.replace("doc.md", "1", "h", &passages).await.unwrap();
    store
}

#[tokio::test]
async fn a_build_holds_at_its_next_batch_while_an_awaited_index_embeds() {
    let temp = tempfile::tempdir().unwrap();
    let store = store_of(&temp.path().join("folder"), 6).await;
    let vectors = Vectors::open(&store, "model-a", DIM).await.unwrap();
    let turns = Arc::new(EmbedTurns::default());
    let model = FakeEmbedder::new("model-a");
    let embedding = Embedding {
        embedder: Arc::clone(&model) as Arc<dyn EmbeddingPort>,
        batch_size: 2,
        save_every: 100,
        embed_text: Arc::new(|passage| passage.text.clone()),
        turns: Arc::clone(&turns),
    };
    // A course opens after the build's first batch, and starts embedding.
    let course = temp.path().join("course");
    let ahead: Arc<Mutex<Option<(Claim, Running)>>> = Arc::default();
    let pausing = Arc::new(Notify::new());
    let told = Arc::new(Mutex::new(Told::default()));
    let mut watch = Watch {
        told: Arc::clone(&told),
        pausing: Arc::clone(&pausing),
        after_first: Some(Box::new({
            let (turns, ahead, course) = (Arc::clone(&turns), Arc::clone(&ahead), course.clone());
            move || {
                *ahead.lock() = Some((
                    turns.claim(&course, Priority::Awaited),
                    turns.running(&course),
                ));
            }
        })),
    };

    let run = async {
        build::embed_pending(
            &store,
            &vectors,
            &embedding,
            "",
            &CancellationToken::new(),
            &mut watch,
        )
        .await
        .unwrap()
    };
    let release = async {
        pausing.notified().await;
        // Held after exactly the batch it was in.
        assert_eq!(model.embedded().len(), 2);
        assert_eq!(told.lock().paused, vec![true]);
        ahead.lock().take();
    };
    let (round, ()) = tokio::join!(run, release);

    assert!(!round.cancelled);
    assert_eq!(round.passages, 6);
    assert_eq!(model.embedded().len(), 6);
    assert_eq!(told.lock().embedded, vec![2, 2, 2]);
    assert_eq!(told.lock().paused, vec![true, false]);
    store.close().await;
}

#[tokio::test]
async fn a_build_stopped_while_it_holds_saves_what_it_embedded() {
    let temp = tempfile::tempdir().unwrap();
    let store = store_of(&temp.path().join("folder"), 4).await;
    let vectors = Vectors::open(&store, "model-a", DIM).await.unwrap();
    let turns = Arc::new(EmbedTurns::default());
    let course = temp.path().join("course");
    let _awaited = turns.claim(&course, Priority::Awaited);
    let _embedding = turns.running(&course);
    let model = FakeEmbedder::new("model-a");
    let embedding = Embedding {
        embedder: Arc::clone(&model) as Arc<dyn EmbeddingPort>,
        batch_size: 2,
        save_every: 100,
        embed_text: Arc::new(|passage| passage.text.clone()),
        turns: Arc::clone(&turns),
    };
    let cancel = CancellationToken::new();
    let pausing = Arc::new(Notify::new());
    let mut watch = Watch {
        told: Arc::default(),
        pausing: Arc::clone(&pausing),
        after_first: None,
    };
    let run = build::embed_pending(&store, &vectors, &embedding, "", &cancel, &mut watch);
    let stop = async {
        pausing.notified().await;
        cancel.cancel();
    };
    let (round, ()) = tokio::join!(run, stop);
    let round = round.unwrap();
    assert!(round.cancelled);
    assert_eq!(round.passages, 0);
    assert!(model.embedded().is_empty());
    assert_eq!(store.pending("").await.unwrap(), vec!["doc.md".to_string()]);
    store.close().await;
}
