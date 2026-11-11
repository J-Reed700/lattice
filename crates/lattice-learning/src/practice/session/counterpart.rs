//! The counterpart's reply, for every kind of work: asked for, checked, asked again once with the reason, or
//! refused.

use crate::practice::session::kind::Kind;
use crate::practice::session::observation::Observation;
use crate::practice::session::policy::{Cue, Policy};
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::practice::work::{Exercise, PartRef};
use lattice_core::shared::error::{AppError, Result};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;

#[cfg(test)]
mod tests;

/// A reply that passed every check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub content: String,
    /// The parts of the learner's work the reply points at.
    pub parts: Vec<PartRef>,
}

/// The counterpart's checked reply on `cue`, or None when it chose silence (only on a checkpoint).
pub async fn respond(
    _llm: &dyn LLMPort,
    _kind: &Kind,
    _exercise: &Exercise,
    _policy: &Policy,
    _cue: &Cue,
    _observation: &Observation,
    _cancel: CancellationToken,
) -> Result<Option<Reply>> {
    Err(AppError::InvalidState(
        "The counterpart's reply is not built yet.".into(),
    ))
}

/// The kind's reply fields read as `T`.
pub fn fields_as<T: DeserializeOwned>(_fields: &Map<String, Value>) -> Result<T> {
    Err(AppError::InvalidState(
        "The reply fields are not read yet.".into(),
    ))
}
