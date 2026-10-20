//! Bringing a course's index in line with the course, in the three steps
//! every run is made of; the manager takes the locks around them.
//!
//! - Rows ([`sync_rows`], under the index's `writing` lock): the course's
//!   active versions are the truth. A stored version that is no longer
//!   active (replaced by a refresh, an adoption or a reimport, or its source
//!   deleted) leaves with its passages and their vectors; an active version
//!   the store lacks is chunked in, and the full-text index follows by
//!   trigger. Fast, and needs no model, so a user's command can run it.
//! - Embedding ([`embed`], under the `embedding` lock): whatever is not
//!   embedded with the active model is embedded, under the source's title.
//!   The store is the checkpoint: a version is marked embedded only after a
//!   save that holds all of its passages, so a run cut short resumes where
//!   it stopped. Rows may change meanwhile; a vector for a passage that left
//!   is never a hit, since a hit is read back through the rows.
//! - Settling ([`settle`]): the mode the index is in, and, once every
//!   passage is embedded, the course's revision recorded in the store's
//!   meta, which is how the startup backfill tells a current index.

use super::{IndexMode, CHUNK_CHARS, STEP_CHARS};
use crate::features::learning::persistence::hash_text;
use crate::features::learning::source_library::{ActiveVersion, LearningSourceLibraryRepository};
use crate::infrastructure::passage_index::build::{self, BATCH_SIZE, SAVE_EVERY};
use crate::infrastructure::passage_index::chunking::spans::span_chunks;
use crate::infrastructure::passage_index::{
    embed_pending, EmbedTurns, Embedding, Engine, LiveIndex, Passage, PassageStore, Progress,
};
use lattice_core::shared::Result;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// What indexing tells whoever waits for it: a lesson job's stage text, a
/// build job's saved progress.
pub trait CourseProgress: Send {
    /// A background build holds the index; this run waits for it to finish
    /// the same work.
    fn waiting(&mut self) {}
    /// `references` active versions are new to the index and being chunked.
    fn indexing(&mut self, references: usize) {
        let _ = references;
    }
    /// `done` of the `total` passages this run embeds are embedded.
    fn embedded(&mut self, done: usize, total: usize) {
        let _ = (done, total);
    }
}

/// Tells no one: for a run with nobody watching (a user's command, a test).
pub(super) struct Silent;

impl CourseProgress for Silent {}

/// The store's meta key for the stamp (see [`stamp`]) of the last run that
/// left every passage embedded.
pub(super) const META_REVISION: &str = "revision";
/// The store's meta key for the program the index belongs to.
const META_PROGRAM: &str = "program";

/// The course's revision: which versions are active. A build queued for a
/// revision is one job however often it is asked for.
pub(super) fn revision(versions: &[ActiveVersion]) -> String {
    let ids: Vec<&str> = versions.iter().map(|version| version.id.as_str()).collect();
    hash_text(&ids.join("\n"))
}

/// What an index holds when it is current: the course's `revision`,
/// embedded by the model `identity`. A new model is a new stamp, so an index
/// made by the old one is not current, and a build for the new one is a new
/// build.
pub(super) fn stamp(revision: &str, identity: &str) -> String {
    hash_text(&format!("{revision}\n{identity}"))
}

/// Chunks a version's text into the store under its id. Shared with the
/// reference collection's in-memory store, so a passage ranked there is the
/// same span a course's index ranks.
pub(in crate::features::learning) async fn write_version(
    store: &PassageStore,
    version_id: &str,
    content_hash: &str,
    text: &str,
) -> Result<Vec<i64>> {
    let passages = span_chunks(text, CHUNK_CHARS, STEP_CHARS);
    // Versions are immutable, so there is no stamp to compare; the hash says
    // what was chunked.
    store.replace(version_id, "", content_hash, &passages).await
}

/// Brings `index`'s rows in line with `program`'s active versions, dropping
/// the vectors of passages that leave from `engine` (or the vectors this
/// session last loaded). The caller holds `writing`. Returns the active
/// versions.
pub(super) async fn sync_rows(
    repo: &LearningSourceLibraryRepository,
    program: &str,
    index: &LiveIndex,
    engine: Option<&Engine>,
    progress: &mut dyn CourseProgress,
) -> Result<Vec<ActiveVersion>> {
    let store = &index.store;
    let active = repo.active_versions(program).await?;
    let stored = store.documents("").await?;
    let wanted: HashSet<&str> = active.iter().map(|version| version.id.as_str()).collect();
    let mut leaving: Vec<String> = stored
        .keys()
        .filter(|key| !wanted.contains(key.as_str()))
        .cloned()
        .collect();
    let arriving: Vec<_> = active
        .iter()
        .filter(|version| {
            stored
                .get(&version.id)
                .is_none_or(|document| document.content_hash != version.content_sha256)
        })
        .collect();
    leaving.extend(
        arriving
            .iter()
            .filter(|version| stored.contains_key(&version.id))
            .map(|version| version.id.clone()),
    );
    let removed = store.remove(&leaving).await?;
    // Vectors on disk for passages that are gone are never hits; the ones in
    // memory go now.
    if let Some(engine) = engine.cloned().or_else(|| index.engine()) {
        engine.vectors.remove(&removed)?;
    }
    if !arriving.is_empty() {
        progress.indexing(arriving.len());
    }
    for version in &arriving {
        let Some(text) = repo.version_text(program, &version.id).await? else {
            continue;
        };
        write_version(store, &version.id, &version.content_sha256, &text).await?;
    }
    if !leaving.is_empty() || !arriving.is_empty() {
        info!(
            program,
            left = leaving.len(),
            arrived = arriving.len(),
            "Course index: brought the references in line with the course"
        );
    }
    Ok(active)
}

/// Embeds every pending passage with `engine`, each under its version's
/// title. The caller holds `embedding`. A model that fails part-way leaves
/// the rest pending, to be found by text until the next run embeds it.
pub(super) async fn embed(
    store: &PassageStore,
    engine: &Engine,
    turns: &Arc<EmbedTurns>,
    versions: &[ActiveVersion],
    program: &str,
    cancel: &CancellationToken,
    progress: &mut dyn CourseProgress,
) -> Result<()> {
    let titles: HashMap<String, String> = versions
        .iter()
        .map(|version| (version.id.clone(), version.title.clone()))
        .collect();
    let embedding = Embedding {
        embedder: Arc::clone(&engine.embedder),
        batch_size: BATCH_SIZE,
        save_every: SAVE_EVERY,
        // The title says what a page is about where its passage does not.
        embed_text: Arc::new(
            move |passage: &Passage| match titles.get(&passage.document) {
                Some(title) if !title.trim().is_empty() => format!("{title}\n{}", passage.text),
                _ => passage.text.clone(),
            },
        ),
        turns: Arc::clone(turns),
    };
    let before = store.counts("").await?;
    let mut told = Told {
        progress,
        done: 0,
        total: before
            .passages_total
            .saturating_sub(before.passages_embedded) as usize,
    };
    if let Err(error) =
        embed_pending(store, &engine.vectors, &embedding, "", cancel, &mut told).await
    {
        warn!(%error, program, "Course index: embedding stopped part-way");
        if let Err(error) = build::save(store, &engine.vectors, &mut Vec::new()).await {
            warn!(%error, program, "Course index: could not save the vectors embedded so far");
        }
    }
    Ok(())
}

/// The mode `store` is searched in with `engine`. Once every passage is
/// embedded, the course's stamp is recorded, so the startup backfill and the
/// status read know the index is current.
pub(super) async fn settle(
    store: &PassageStore,
    engine: Option<&Engine>,
    program: &str,
    versions: &[ActiveVersion],
) -> Result<IndexMode> {
    if engine.is_none() {
        return Ok(IndexMode::Lexical);
    }
    let counts = store.counts("").await?;
    if counts.passages_embedded < counts.passages_total {
        return Ok(IndexMode::Partial {
            embedded: counts.passages_embedded,
            total: counts.passages_total,
        });
    }
    let stamp = engine.map_or_else(String::new, |engine| {
        stamp(&revision(versions), &engine.embedder.model_identity())
    });
    if store.meta(META_REVISION).await?.as_deref() != Some(stamp.as_str()) {
        store.set_meta(META_PROGRAM, program).await?;
        store.set_meta(META_REVISION, &stamp).await?;
    }
    Ok(IndexMode::Hybrid)
}

/// The embedding loop's progress, counted for the caller.
struct Told<'a> {
    progress: &'a mut dyn CourseProgress,
    done: usize,
    total: usize,
}

impl Progress for Told<'_> {
    fn embedded(&mut self, count: usize) {
        self.done += count;
        self.progress.embedded(self.done, self.total.max(self.done));
    }

    fn saved(&mut self, _documents: usize) {}
}
