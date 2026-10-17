//! How much prompt the corpus planner may be sent: what its model's window
//! holds beside the plan, less anything a server has shown about a window
//! smaller than the one its port reports.
//!
//! No clock sizes the prompt. The planner used to be held to a 20-second
//! timeout and its catalog to what the model was measured to read inside it,
//! which encoded one machine and one model; a slow model now gets its whole
//! window and the user's Stop ends a call they no longer want.

use std::collections::HashMap;
use std::sync::Mutex;

use lattice_core::application::ports::LLMPort;

/// What the planner's servers have shown about their real windows. One per
/// chat runtime, shared by every turn it runs, because a server's window is a
/// property of the server, not of any one conversation; keyed by model,
/// because a correction learned on one model says nothing about the next.
#[derive(Default)]
pub struct PlannerHealth {
    /// Per model, the most prompt the next attempt may send: half of the last
    /// prompt its server refused as larger than its window. Absent until a
    /// refusal, so the reported window is the only limit.
    ceilings: Mutex<HashMap<String, usize>>,
}

impl PlannerHealth {
    fn ceilings(&self) -> std::sync::MutexGuard<'_, HashMap<String, usize>> {
        self.ceilings
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The prompt size, in tokens, the next attempt on `llm` is built to: the
    /// room its window leaves after the plan's reservation, held under any
    /// ceiling a refusal taught.
    pub(super) fn prompt_tokens(&self, llm: &dyn LLMPort) -> usize {
        let room = super::prompt::window_room(llm);
        self.ceilings()
            .get(llm.model_name())
            .map_or(room, |ceiling| room.min(*ceiling))
    }

    /// The server refused the prompt as larger than its window. The prompt is
    /// counted against the window the port reports, so this means the report
    /// was wrong. Halving finds the real window within a few turns, and the
    /// ceiling holds for as long as the runtime does.
    pub(super) fn overflowed(&self, model: &str, sent_tokens: usize) {
        let mut ceilings = self.ceilings();
        let ceiling = ceilings.entry(model.to_string()).or_insert(usize::MAX);
        *ceiling = sent_tokens.min(*ceiling) / 2;
    }
}

/// One turn's use of the planner: the documents a local search ranked for the
/// request, searched once however many times the turn plans.
pub(in super::super) struct PlannerTurn<'h> {
    /// Document IDs best first, from searching the request. `None` until the
    /// search has run, so a turn searches once however many times it plans.
    pub(in super::super) candidates: Option<Vec<String>>,
    pub(super) health: &'h PlannerHealth,
}

impl<'h> PlannerTurn<'h> {
    pub(in super::super) fn with_health(health: &'h PlannerHealth) -> Self {
        Self {
            candidates: None,
            health,
        }
    }

    /// Tokens of conversation the planner's next prompt on `llm` carries:
    /// its share of the prompt that model's window holds.
    pub(in super::super) fn history_tokens(&self, llm: &dyn LLMPort) -> usize {
        super::prompt::history_tokens(self.health.prompt_tokens(llm))
    }

    pub(super) fn candidate_ids(&self) -> &[String] {
        self.candidates.as_deref().unwrap_or_default()
    }
}
