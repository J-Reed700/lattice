//! The hint ladder, for every kind of work.

use crate::practice::session::kind::{Hint, HintAsk, Kind};
use crate::practice::session::observation::Observation;
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::practice::work::{Exercise, Rung};
use lattice_core::shared::error::{AppError, Result};
use tokio_util::sync::CancellationToken;

#[cfg(test)]
mod tests;

/// The rung after the highest one given: Nudge first, and Point stays Point.
pub fn next_rung(given: impl IntoIterator<Item = Rung>) -> Rung {
    Rung::after(given.into_iter().max())
}

/// The hint of `rung`, or of the next rung the kind can make when it has none of `rung`. The hint carries the rung
/// it is.
pub async fn give(
    llm: &dyn LLMPort,
    kind: &Kind,
    exercise: &Exercise,
    observation: &Observation,
    rung: Rung,
    in_role: bool,
    cancel: CancellationToken,
) -> Result<Hint> {
    let observed = observation.render(kind)?;
    let mut rung = rung;
    loop {
        let ask = HintAsk {
            llm,
            exercise,
            observation,
            observed: &observed,
            rung,
            in_role,
            cancel: cancel.clone(),
        };
        if let Some(hint) = kind.hinting.hint(ask).await? {
            return Ok(Hint { rung, ..hint });
        }
        let above = next_rung([rung]);
        if above == rung {
            return Err(AppError::InvalidState(
                "No hint can be made for this exercise.".into(),
            ));
        }
        rung = above;
    }
}
