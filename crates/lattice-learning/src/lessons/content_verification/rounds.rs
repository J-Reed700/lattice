//! What ends the loop that prepares a job's lessons (claim classes design
//! §3.2). A lesson stands while every source its report is bound to is as it
//! was: a resolved claim depends only on the passages its own check read, so
//! a source added since never changes it and never sends the lesson round
//! again. A lesson whose saved preparation no longer stands is prepared once
//! more in a job, and so is a publication that finds a bound source changed;
//! the second time, the job ends saying so instead of going round.
use super::*;

/// The rounds a job has gone.
#[derive(Default)]
pub(crate) struct Rounds {
    /// The lessons prepared once more in this job.
    prepared_again: HashSet<String>,
    /// Whether a publication that found a bound source changed has been tried
    /// again.
    published_again: bool,
}

impl Rounds {
    /// Whether the job goes round again once `staged` are prepared and the
    /// program holds `sources`: a lesson whose report does not stand (another
    /// checker, a report that may not publish, a bound source gone or
    /// changed) is prepared again, once. A second time is an error that says
    /// what stops it.
    pub(crate) fn again(
        &mut self,
        staged: &[(String, PreparedLearningLesson)],
        sources: &[LearningSourceDto],
        llm: &dyn LLMPort,
    ) -> Result<bool> {
        let mut again = false;
        for (id, prepared) in staged {
            let refusal = match &prepared.verification {
                Some(report) => report.refusal(id, prepared, sources, llm),
                None => Some("it has no verification report".to_owned()),
            };
            let Some(refusal) = refusal else {
                continue;
            };
            if !self.prepared_again.insert(id.clone()) {
                return Err(AppError::InvalidState(format!(
                    "The lesson was prepared again and still cannot be published: {}.",
                    refusal.trim_end_matches('.')
                )));
            }
            tracing::info!(lesson = %id, %refusal, "The lesson's preparation does not stand; it is prepared again");
            again = true;
        }
        Ok(again)
    }

    /// A publication found a bound source changed: the job tries once more,
    /// and the second time fails as it found.
    pub(crate) fn publishing_failed(
        &mut self,
        error: AppError,
    ) -> Result<()> {
        if std::mem::replace(&mut self.published_again, true) {
            return Err(error);
        }
        Ok(())
    }
}
