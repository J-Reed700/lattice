//! What the activity view reads: every part of the app that runs long work
//! or holds a shared resource, each through its own read, and the signals
//! that move when one of them changes with no event of its own.

use super::Container;
use crate::infrastructure::passage_index::EmbedTurns;
use lattice_core::application::ports::ActivitySource;
use std::sync::Arc;
use tokio::sync::watch;

impl Container {
    /// Every source of the activity view. Each is a read of state its
    /// feature already holds; building them costs a few handles.
    pub fn activity_sources(&self) -> Vec<Arc<dyn ActivitySource>> {
        let pool = self.db_pool().clone();
        let mut sources: Vec<Arc<dyn ActivitySource>> = vec![
            Arc::new(crate::features::learning::activity::LearningActivity::new(
                pool.clone(),
                Arc::clone(&self.jobs),
                self.course_indexes.clone(),
                crate::features::learning::outline_progress::OutlineRunRegistry::clone(
                    &self.outline_run_registry,
                ),
            )),
            Arc::new(
                crate::features::conversation::activity::ConversationActivity::new(
                    pool,
                    Arc::clone(&self.jobs),
                ),
            ),
            Arc::new(crate::features::batch::activity::ImportActivity::new(
                Arc::clone(&self.jobs),
            )),
            Arc::clone(&self.web_page_snapshots) as Arc<dyn ActivitySource>,
            Arc::new(crate::features::llm::activity::ModelActivity::new(
                Arc::clone(&self.llm_instances),
            )),
            Arc::new(
                crate::features::embedding::activity::EmbeddingActivity::new(Arc::clone(
                    &self.embedding_runtime,
                )),
            ),
            Arc::new(crate::features::download::activity::DownloadActivity::new(
                Arc::clone(self.ai.download_manager()),
            )),
            Arc::new(crate::features::indexing::activity::LibraryActivity::new(
                Arc::clone(self.indexing.indexing_state()),
            )),
        ];
        if let Some(folders) = &self.folder_index {
            sources.push(Arc::new(
                crate::features::explorer::index::activity::FolderActivity::new(Arc::clone(
                    folders,
                )),
            ));
        }
        sources
    }

    /// The turn order the indexes embed in; `None` in test fixtures.
    pub fn index_turns(&self) -> Option<&Arc<EmbedTurns>> {
        self.index_turns.as_ref()
    }

    /// Signals that move when something the activity view shows changes
    /// without a job status or an event of its own: model requests joining
    /// or leaving a queue, models and the embedding model loading, outline
    /// runs, the index turn order.
    pub fn activity_changes(&self) -> Vec<watch::Receiver<u64>> {
        let mut changes = vec![
            self.llm_runtime.schedulers.changes(),
            self.llm_instances.changes(),
            self.embedding_runtime.changes(),
            self.outline_run_registry.changes(),
        ];
        if let Some(turns) = &self.index_turns {
            changes.push(turns.changes());
        }
        changes
    }
}
