//! One conversation's scope for one turn, and the one place that keeps what
//! the turn minted.

use super::shown::{Record, Scope};
use crate::application::ports::conversation_names::ConversationNames;
use crate::shared::error::Result;
#[cfg(any(test, feature = "test-support"))]
use crate::shared::error::AppError;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};

#[cfg(any(test, feature = "test-support"))]
pub use self::memory::MemoryNames;

#[cfg(test)]
mod tests;

struct State {
    scope: Scope,
    /// Names that were kept before the turn opened.
    loaded: usize,
    /// Names minted this turn that are kept already.
    saved: usize,
}

/// The scope of one conversation for one turn: what the conversation has been
/// shown so far, and what this turn adds. Everything in the turn mints and
/// resolves through it.
pub struct TurnNames {
    store: Option<(Arc<dyn ConversationNames>, String)>,
    state: Mutex<State>,
    /// Held for the whole of a save, so a position is written once.
    saving: tokio::sync::Mutex<()>,
}

/// Every conversation's one live scope in this process.
pub struct KeptNames {
    store: Arc<dyn ConversationNames>,
    open: Mutex<HashMap<String, Weak<TurnNames>>>,
}

impl KeptNames {
    pub fn new(store: Arc<dyn ConversationNames>) -> Self {
        Self {
            store,
            open: Mutex::new(HashMap::new()),
        }
    }

    fn live(&self, conversation_id: &str) -> Option<Arc<TurnNames>> {
        let open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        open.get(conversation_id).and_then(Weak::upgrade)
    }

    /// The conversation's one scope: the live one while any turn holds it, else as kept.
    pub async fn open(&self, conversation_id: &str) -> Result<Arc<TurnNames>> {
        if let Some(live) = self.live(conversation_id) {
            return Ok(live);
        }
        let mine = Arc::new(TurnNames::open(Arc::clone(&self.store), conversation_id).await?);
        let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(live) = open.get(conversation_id).and_then(Weak::upgrade) {
            return Ok(live);
        }
        open.insert(conversation_id.to_owned(), Arc::downgrade(&mine));
        Ok(mine)
    }
}

impl TurnNames {
    /// The conversation's names as they are kept.
    async fn open(store: Arc<dyn ConversationNames>, conversation_id: &str) -> Result<Self> {
        let records = store.names(conversation_id).await?;
        let loaded = records.len();
        let scope = Scope::stored(records)?;
        Ok(Self {
            store: Some((store, conversation_id.to_owned())),
            state: Mutex::new(State {
                scope,
                loaded,
                saved: 0,
            }),
            saving: tokio::sync::Mutex::new(()),
        })
    }

    /// A turn with no conversation: its names last as long as it does.
    pub fn unstored() -> Self {
        Self {
            store: None,
            state: Mutex::new(State {
                scope: Scope::for_question(),
                loaded: 0,
                saved: 0,
            }),
            saving: tokio::sync::Mutex::new(()),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs `f` on the scope, under its lock.
    pub fn with<R>(&self, f: impl FnOnce(&mut Scope) -> R) -> R {
        f(&mut self.state().scope)
    }

    /// Keeps the names minted since the last save.
    pub async fn save(&self) -> Result<()> {
        let Some((store, conversation_id)) = &self.store else {
            return Ok(());
        };
        let _saving = self.saving.lock().await;
        let (saved, first, fresh) = {
            let state = self.state();
            let Some(new) = state.scope.fresh().get(state.saved..) else {
                return Ok(());
            };
            (state.saved, state.loaded + state.saved, new.to_vec())
        };
        if fresh.is_empty() {
            return Ok(());
        }
        store.keep(conversation_id, first, &fresh).await?;
        let mut state = self.state();
        state.saved = state.saved.max(saved + fresh.len());
        Ok(())
    }
}

impl std::fmt::Debug for TurnNames {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TurnNames")
            .field("kept", &self.store.is_some())
            .finish()
    }
}

#[cfg(any(test, feature = "test-support"))]
mod memory {
    use super::*;
    use async_trait::async_trait;
    use std::collections::BTreeMap;

    /// Names kept in memory, by position, for tests.
    #[derive(Default)]
    pub struct MemoryNames {
        kept: Mutex<BTreeMap<String, BTreeMap<usize, Record>>>,
        writes: Mutex<usize>,
    }

    impl MemoryNames {
        pub fn new() -> Self {
            Self::default()
        }

        /// How many `keep` calls wrote at least one new position.
        pub fn writes(&self) -> usize {
            *self.writes.lock().unwrap_or_else(PoisonError::into_inner)
        }

        /// Copies one conversation's names to another, as a fork does.
        pub fn copy(&self, from: &str, to: &str) {
            let mut kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
            let copied = kept.get(from).cloned().unwrap_or_default();
            kept.insert(to.to_owned(), copied);
        }
    }

    #[async_trait]
    impl ConversationNames for MemoryNames {
        async fn names(&self, conversation_id: &str) -> Result<Vec<Record>> {
            let kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
            Ok(kept
                .get(conversation_id)
                .map(|names| names.values().cloned().collect())
                .unwrap_or_default())
        }

        async fn keep(&self, conversation_id: &str, first: usize, fresh: &[Record]) -> Result<()> {
            let mut kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
            let names = kept.entry(conversation_id.to_owned()).or_default();
            if (first..first + fresh.len()).any(|position| names.contains_key(&position)) {
                return Err(AppError::InvalidData(
                    "A name's position was written twice.".into(),
                ));
            }
            for (offset, record) in fresh.iter().enumerate() {
                names.insert(first + offset, record.clone());
            }
            if !fresh.is_empty() {
                *self.writes.lock().unwrap_or_else(PoisonError::into_inner) += 1;
            }
            Ok(())
        }
    }
}
