//! One course's index as a reader holds it, and what a search reads from it.
//!
//! A [`CourseSearch`] is held for as long as its reader reads: a lesson job
//! for its whole run, a claim review or a source search for one command.
//! Each [`CourseSearch::ensure_current`] brings the index in line with the
//! course and leaves an [`IndexView`]: the store, the one hybrid search over
//! its vectors and full text when anything is embedded with the active model,
//! the mode and the embedding identity. A collection opened on the search
//! takes the view as it is then, so one collection is one mode.
//!
//! While any reader holds a course, a capture does not queue a background
//! build of it: the reader's next `ensure_current` picks the change up.

use super::manager::CourseIndexes;
use super::sync::CourseProgress;
use super::IndexMode;
use crate::infrastructure::passage_index::{
    Engine, LiveIndex, PassageStore, PassageTextSearch, WordMatch,
};
use lattice_core::application::ports::hybrid_search::{HybridSearchFactory, HybridSearchTrait};
use lattice_core::shared::Result;
use parking_lot::RwLock;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// A course's index as one run of `ensure_current` left it.
#[derive(Clone)]
pub struct IndexView {
    pub store: Arc<PassageStore>,
    /// Dense and full-text search fused by the library's orchestrator; `None`
    /// when nothing can be searched by meaning (no embedding model).
    pub hybrid: Option<Arc<dyn HybridSearchTrait>>,
    pub mode: IndexMode,
    /// The embedding model's identity, when the vectors are searched.
    pub identity: Option<String>,
}

impl IndexView {
    /// The view of `store` after a run in `mode` that left `engine`, searched
    /// through the hybrid search `searches` builds over the engine's vectors
    /// and the store's full text.
    pub(super) fn of(
        store: &Arc<PassageStore>,
        mode: IndexMode,
        engine: Option<Engine>,
        searches: &dyn HybridSearchFactory,
    ) -> Self {
        let identity = engine
            .as_ref()
            .map(|engine| engine.embedder.model_identity());
        let hybrid = engine.map(|engine| {
            searches.hybrid(
                engine.embedder,
                engine.vectors.port(),
                // Whole words, as the library and the claim checker's
                // keyword ranking have always matched: a source must not
                // enter a claim's evidence because one of its words begins
                // with one of the claim's.
                Arc::new(PassageTextSearch::new(Arc::clone(store)).matching(WordMatch::Whole)),
            )
        });
        Self {
            store: Arc::clone(store),
            hybrid,
            mode,
            identity,
        }
    }
}

/// One course's index, held by a reader.
pub struct CourseSearch {
    courses: Arc<CourseIndexes>,
    program: String,
    index: Arc<LiveIndex>,
    /// The reader's own stop: a lesson job's cancellation, so a build it
    /// runs inline stops with it.
    cancel: CancellationToken,
    view: RwLock<IndexView>,
}

impl CourseSearch {
    pub(super) fn new(
        courses: Arc<CourseIndexes>,
        program: String,
        index: Arc<LiveIndex>,
        cancel: CancellationToken,
        view: IndexView,
    ) -> Self {
        courses.hold(&program);
        Self {
            courses,
            program,
            index,
            cancel,
            view: RwLock::new(view),
        }
    }

    pub fn program(&self) -> &str {
        &self.program
    }

    /// The index as the last `ensure_current` left it. Before the first, it
    /// is searched by text over whatever the store holds.
    pub fn view(&self) -> IndexView {
        self.view.read().clone()
    }

    pub fn mode(&self) -> IndexMode {
        self.view.read().mode
    }

    /// Brings the index in line with the course, embedding what is pending,
    /// and keeps the view it leaves. Waits for a background build that holds
    /// the index, which is doing the same work.
    pub async fn ensure_current(&self, progress: &mut dyn CourseProgress) -> Result<IndexMode> {
        let view = self
            .courses
            .bring_current(&self.index, &self.program, &self.cancel, progress)
            .await?;
        let mode = view.mode;
        *self.view.write() = view;
        Ok(mode)
    }
}

impl CourseSearch {
    /// Brings the index's rows in line with the course without embedding
    /// anything, and keeps the view it leaves: for a user's command, which
    /// must never wait for a course to embed. A build is queued for whatever
    /// is not embedded yet.
    pub async fn bring_text_current(&self) -> Result<IndexMode> {
        let view = self
            .courses
            .bring_text_current(&self.index, &self.program)
            .await?;
        let mode = view.mode;
        *self.view.write() = view;
        Ok(mode)
    }
}

impl Drop for CourseSearch {
    fn drop(&mut self) {
        self.courses.let_go(&self.program, &self.index);
    }
}
