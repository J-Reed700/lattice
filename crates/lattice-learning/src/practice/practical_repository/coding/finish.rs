//! Finishing a coding session: the scorecard, the debrief, the ratings and the course's evidence, in one transaction.
use super::super::{decode, json, uuid, InternalActivitySnapshot, LearningPracticalRepository};
use super::{add_event, name_of, open_row_on};
use crate::persistence::db;
use crate::practice::coding::patterns;
use crate::practice::coding::records::{Applied, CodingOp, FinishWrite};
use lattice_core::domain::learning::practice::ScoreStatus;
use lattice_core::shared::error::Result;
use sqlx::Row;

/// The score a session's scorecard earned: points over the maximum of the rows that were observed.
fn score_of(write: &FinishWrite<'_>) -> Option<f64> {
    let (points, max) = write
        .scorecard
        .rows
        .iter()
        .filter(|row| row.status == ScoreStatus::Observed)
        .fold((0_u32, 0_u32), |(points, max), row| {
            (
                points + u32::from(row.points.unwrap_or(0)),
                max + u32::from(row.max_points),
            )
        });
    (max > 0).then(|| f64::from(points) / f64::from(max))
}

impl LearningPracticalRepository {
    /// Writes everything a finished session leaves behind. A session already finished, or an operation already
    /// seen, writes nothing.
    pub async fn finish_coding_session(
        &self,
        program_id: &str,
        session_id: &str,
        op: &CodingOp<'_>,
        write: &FinishWrite<'_>,
        at: i64,
    ) -> Result<Applied<()>> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    /// How many evidence events a session left, for tests.
    #[cfg(test)]
    pub(crate) async fn coding_evidence_count(&self, session_id: &str) -> Result<i64> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }
}
