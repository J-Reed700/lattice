//! The hint ladder, for every kind of work.

use crate::practice::session::kind::{Hint, Kind};
use crate::practice::session::observation::Observation;
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::practice::work::{Exercise, PartRef, Rung};
use lattice_core::shared::error::{AppError, Result};
use tokio_util::sync::CancellationToken;

#[cfg(test)]
mod tests;

/// The rung after the highest one given: Nudge first, and Point stays Point.
pub fn next_rung(_given: impl IntoIterator<Item = Rung>) -> Rung {
    Rung::Nudge
}

/// The hint of `rung`, or of the next rung the kind can make when it has none of `rung`.
pub async fn give(
    _llm: &dyn LLMPort,
    _kind: &Kind,
    _exercise: &Exercise,
    _observation: &Observation,
    _rung: Rung,
    _in_role: bool,
    _cancel: CancellationToken,
) -> Result<Hint> {
    Err(AppError::InvalidState(
        "The hint ladder is not built yet.".into(),
    ))
}
