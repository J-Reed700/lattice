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
        uuid(session_id, "coding session")?;
        uuid(op.operation_id, "operation")?;
        write.scorecard.validate()?;
        let mut tx = self.begin_coding_write(program_id).await?;
        let operation = op.operation(program_id, session_id);
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return Ok(Applied::Replayed);
        }
        let row = super::row_on(&mut tx, program_id, session_id).await?;
        if row.finished_at.is_some() {
            tx.commit().await.map_err(db)?;
            return Ok(Applied::Replayed);
        }
        let row = open_row_on(&mut tx, program_id, session_id).await?;
        let snapshot: String = sqlx::query_scalar(
            "SELECT activity_snapshot_json FROM learning_simulation_sessions WHERE id=?",
        )
        .bind(session_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        let activity: InternalActivitySnapshot = decode(&snapshot)?;
        let mut active_ms = row.active_ms;
        if let Some(opened) = row.opened_at {
            active_ms += u64::try_from(at - opened).unwrap_or(0);
            add_event(&mut tx, session_id, "closed", &serde_json::json!({}), at).await?;
        }
        for (ordinal, each) in write.scorecard.rows.iter().enumerate() {
            sqlx::query("INSERT INTO learning_scorecards(session_id,criterion_id,ordinal,title,status,points,max_points,evidence_json,feedback,next_step) VALUES(?,?,?,?,?,?,?,?,?,?)")
                .bind(session_id).bind(&each.criterion_id).bind(i64::try_from(ordinal).unwrap_or(i64::MAX))
                .bind(&each.title).bind(name_of(&each.status)?)
                .bind(each.points.map(i64::from)).bind(i64::from(each.max_points))
                .bind(json(&each.evidence)?).bind(&each.feedback).bind(&each.next_step)
                .execute(&mut *tx).await.map_err(db)?;
        }
        let target: Option<i64> =
            sqlx::query_scalar("SELECT interview_at FROM learning_practice_profiles WHERE program_id=?")
                .bind(program_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
                .flatten();
        let ratings = patterns::record_ratings_in(
            &mut tx,
            program_id,
            write.language,
            write.pattern_ids,
            write.rating,
            at,
            target,
        )
        .await?;
        sqlx::query("INSERT INTO learning_coding_debriefs(session_id,approach,complexity,ratings_json,model,written_at) VALUES(?,?,?,?,?,?)")
            .bind(session_id).bind(write.approach).bind(write.complexity)
            .bind(json(&ratings)?).bind(write.model).bind(at)
            .execute(&mut *tx).await.map_err(db)?;
        let observed = write
            .scorecard
            .rows
            .iter()
            .any(|each| each.status == ScoreStatus::Observed);
        let observation = format!(
            "Passed {} of {} cases on the last submit.",
            write.passed, write.total
        );
        let assistance = serde_json::json!({"help": write.help, "hints": write.hints});
        for outcome_id in &activity.outcome_ids {
            sqlx::query("INSERT INTO learning_evidence_events(id,program_id,outcome_id,source_kind,source_id,dimension,result,score,observation,evidence_quote,assistance_json,observed_at) VALUES(?,?,?,'simulation',?,'application',?,?,?,NULL,?,?)")
                .bind(uuid::Uuid::new_v4().to_string()).bind(program_id).bind(outcome_id)
                .bind(session_id).bind(if observed { "observed" } else { "uncertain" })
                .bind(score_of(write)).bind(&observation).bind(json(&assistance)?).bind(at)
                .execute(&mut *tx).await.map_err(db)?;
        }
        sqlx::query("UPDATE learning_coding_sessions SET draft_json=?,active_ms=?,opened_at=NULL,pending_cue_json=NULL,finished_at=?,revision=revision+1 WHERE id=?")
            .bind(json(write.files)?)
            .bind(i64::try_from(active_ms).unwrap_or(i64::MAX))
            .bind(at)
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query("UPDATE learning_simulation_sessions SET status='submitted',submitted_at=?,updated_at=?,revision=revision+1 WHERE id=?")
            .bind(at)
            .bind(at)
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        operation.record(&mut *tx, &()).await?;
        tx.commit().await.map_err(db)?;
        Ok(Applied::Done(()))
    }

    /// How many evidence events a session left, for tests.
    #[cfg(test)]
    pub(crate) async fn coding_evidence_count(&self, session_id: &str) -> Result<i64> {
        let row = sqlx::query("SELECT count(*) AS n FROM learning_evidence_events WHERE source_id=?")
            .bind(session_id)
            .fetch_one(&self.pool)
            .await
            .map_err(db)?;
        Ok(row.get("n"))
    }
}
