//! What a learner may see of a session now.

use crate::practice::session::record::SessionRow;
use lattice_core::domain::learning::practice::work::Hidden;

#[cfg(test)]
mod tests;

/// What the learner may see of a session now. The engine decides it; a kind never reads the setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seen {
    pub failing_case: bool,
    pub uncaught_misreading: bool,
    pub reference: bool,
}

impl Seen {
    /// A finished session shows everything; before that the setting decides, and the reference needs the learner to
    /// have asked for it.
    pub fn of(row: &SessionRow) -> Self {
        let finished = row.finished_at.is_some();
        let submitted = row.submitted_at.is_some();
        let discloses = |hidden: Hidden| finished || row.setting.discloses(hidden, submitted);
        Self {
            failing_case: discloses(Hidden::FailingCase),
            uncaught_misreading: discloses(Hidden::UncaughtMisreading),
            reference: finished
                || (row.solution_seen_at.is_some()
                    && row.setting.discloses(Hidden::WorkedSolution, submitted)),
        }
    }
}
