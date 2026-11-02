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
pub(in crate::features::learning) struct Rounds {
    /// The lessons prepared once more in this job.
    prepared_again: HashSet<String>,
    /// Whether a publication that found a bound source changed has been tried
    /// again.
    published_again: bool,
}

impl Rounds {
    /// Whether the job goes round again once `staged` are prepared and the
    /// program holds `sources`: a lesson whose report is bound to a source that
    /// is gone or changed is prepared again, once. A second time is an error.
    pub(in crate::features::learning) fn again(
        &mut self,
        staged: &[(String, PreparedLearningLesson)],
        sources: &[LearningSourceDto],
        llm: &dyn LLMPort,
    ) -> Result<bool> {
        let mut again = false;
        for (id, prepared) in staged {
            let stands = prepared
                .verification
                .as_ref()
                .is_some_and(|report| report.reusable(id, prepared, sources, llm));
            if stands {
                continue;
            }
            if !self.prepared_again.insert(id.clone()) {
                return Err(AppError::InvalidState(
                    "A source a lesson was checked against changed again while the lesson was being prepared. Prepare it again once the sources stop changing."
                        .into(),
                ));
            }
            again = true;
        }
        Ok(again)
    }

    /// A publication found a bound source changed: the job tries once more,
    /// and the second time fails as it found.
    pub(in crate::features::learning) fn publishing_failed(
        &mut self,
        error: AppError,
    ) -> Result<()> {
        if std::mem::replace(&mut self.published_again, true) {
            return Err(error);
        }
        Ok(())
    }
}
