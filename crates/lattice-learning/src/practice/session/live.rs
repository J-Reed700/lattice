//! The ports a session runs on in the app: the model, the debrief and the clock.

use crate::practice::session::counterpart::{self, Reply};
use crate::practice::session::debrief::{self, Debrief};
use crate::practice::session::hints;
use crate::practice::session::kind::{Hint, Kind};
use crate::practice::session::observation::Observation;
use crate::practice::session::policy::{Cue, Policy};
use crate::practice::session::ports::{Aids, Clock, Debriefer};
use crate::practice::session::record::DebriefSession;
use async_trait::async_trait;
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::practice::work::{Exercise, Rung};
use lattice_core::shared::error::Result;
use tokio_util::sync::CancellationToken;

/// The model calls of a session, made on `llm`.
pub struct LiveAids<'a> {
    pub llm: &'a dyn LLMPort,
    pub cancel: CancellationToken,
}

#[async_trait]
impl Aids for LiveAids<'_> {
    fn model_name(&self) -> &str {
        self.llm.model_name()
    }

    async fn hint(
        &self,
        kind: &Kind,
        exercise: &Exercise,
        observation: &Observation,
        rung: Rung,
        in_role: bool,
    ) -> Result<Hint> {
        hints::give(
            self.llm,
            kind,
            exercise,
            observation,
            rung,
            in_role,
            self.cancel.clone(),
        )
        .await
    }

    async fn reply(
        &self,
        kind: &Kind,
        exercise: &Exercise,
        policy: &Policy,
        cue: &Cue,
        observation: &Observation,
    ) -> Result<Option<Reply>> {
        counterpart::respond(
            self.llm,
            kind,
            exercise,
            policy,
            cue,
            observation,
            self.cancel.clone(),
        )
        .await
    }
}

/// The debrief, written by `llm`.
pub struct LiveDebriefer<'a> {
    pub llm: &'a dyn LLMPort,
}

#[async_trait]
impl Debriefer for LiveDebriefer<'_> {
    async fn debrief(&self, kind: &Kind, session: &DebriefSession) -> Result<Debrief> {
        debrief::debrief(self.llm, kind, session).await
    }
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> i64 {
        crate::persistence::now()
    }
}
