//! The drill build: a Template Method of checkpointed stages. A drill is built from clean code the way mutation
//! testing builds mutants: defects are planted as exact edits and each is proven, so the answer key is exact
//! (docs/design/2026-11-04-code-review-drills.md section 2).
//!
//! The stages run in a fixed order. Each is a method with a typed input and output, and its output is saved as a
//! job checkpoint, so a resumed job starts at the first stage without one and repeats no model call and no run:
//!
//! 1. clean code (`drill-build-v1:clean_code`)
//! 2. clean review (`drill-build-v1:clean_review`)
//! 3. plant (`drill-build-v1:plant`)
//! 4. prove (`drill-build-v1:prove`)
//! 5. blind review (`drill-build-v1:blind_review`)
//! 6. store: one transaction writes the drill and completes the job, so it has no checkpoint to resume from.

use crate::features::learning::drills::grader::ReviewGrader;
use crate::features::learning::drills::runner::CodeRunner;
use crate::features::learning::drills::spec::DrillSpec;
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::Result;
use lattice_core::shared::runtime::jobs::{JobContext, JobOutcome};
use serde::{de::DeserializeOwned, Serialize};
use sqlx::SqlitePool;
use std::future::Future;
use tokio_util::sync::CancellationToken;

// The stages live in `stages/`: `build/` is ignored by the repository's .gitignore.
#[path = "stages/ask.rs"]
pub(in crate::features::learning) mod ask;
#[path = "stages/blind_review.rs"]
mod blind_review;
#[path = "stages/clean_code.rs"]
mod clean_code;
#[path = "stages/clean_review.rs"]
mod clean_review;
#[path = "stages/outputs.rs"]
pub(in crate::features::learning) mod outputs;
#[path = "stages/plant.rs"]
mod plant;
#[path = "stages/prove.rs"]
mod prove;
#[path = "stages/store.rs"]
pub(in crate::features::learning) mod store;

pub(in crate::features::learning) use outputs::*;

/// Names every checkpoint of a build; a change to what the stages save must not resume a build saved under the old
/// shape.
pub(in crate::features::learning) const CHECKPOINT_PREFIX: &str = "drill-build-v1";

/// The steps of a build, for the job's progress: the five checkpointed stages and the store.
pub(in crate::features::learning) const STEPS: u32 = 6;

/// A checkpointed stage of a build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::features::learning) enum Stage {
    CleanCode,
    CleanReview,
    Plant,
    Prove,
    BlindReview,
}

impl Stage {
    /// Every stage, in the order they run.
    #[cfg(test)]
    pub(in crate::features::learning) const ALL: [Stage; 5] = [
        Stage::CleanCode,
        Stage::CleanReview,
        Stage::Plant,
        Stage::Prove,
        Stage::BlindReview,
    ];

    pub(in crate::features::learning) fn name(self) -> &'static str {
        match self {
            Self::CleanCode => "clean_code",
            Self::CleanReview => "clean_review",
            Self::Plant => "plant",
            Self::Prove => "prove",
            Self::BlindReview => "blind_review",
        }
    }

    /// The key its output is saved under.
    pub(in crate::features::learning) fn key(self) -> String {
        format!("{CHECKPOINT_PREFIX}:{}", self.name())
    }

    fn index(self) -> u32 {
        match self {
            Self::CleanCode => 0,
            Self::CleanReview => 1,
            Self::Plant => 2,
            Self::Prove => 3,
            Self::BlindReview => 4,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::CleanCode => "Writing the code",
            Self::CleanReview => "Reviewing the code",
            Self::Plant => "Planting the defects",
            Self::Prove => "Proving the defects",
            Self::BlindReview => "Reviewing the drill blind",
        }
    }
}

/// The step after the last stage.
const STORE_INDEX: u32 = 5;

/// Builds one drill. The model, the runner, the grader and the database are the build's own; the job runtime's
/// context carries the checkpoints and the cancellation.
pub(in crate::features::learning) struct DrillBuild<'a> {
    llm: &'a dyn LLMPort,
    runner: Option<&'a dyn CodeRunner>,
    grader: &'a dyn ReviewGrader,
    pool: &'a SqlitePool,
    cancel: CancellationToken,
}

impl<'a> DrillBuild<'a> {
    /// A build that runs code on `runner`, or, without one, proves every defect by judgement alone.
    pub(in crate::features::learning) fn new(
        llm: &'a dyn LLMPort,
        runner: Option<&'a dyn CodeRunner>,
        grader: &'a dyn ReviewGrader,
        pool: &'a SqlitePool,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            llm,
            runner,
            grader,
            pool,
            cancel,
        }
    }

    /// Runs the stages in order, resuming at the first without a checkpoint. `Stopped` when the job is cancelled;
    /// `Committed` when the drill is stored and the job completed with it.
    pub(in crate::features::learning) async fn run(
        &self,
        context: &JobContext,
        spec: &DrillSpec,
    ) -> Result<JobOutcome> {
        let Some(code) = self
            .stage(context, Stage::CleanCode, || self.clean_code(spec))
            .await?
        else {
            return Ok(JobOutcome::Stopped);
        };
        let Some(review) = self
            .stage(context, Stage::CleanReview, || {
                self.clean_review(spec, &code)
            })
            .await?
        else {
            return Ok(JobOutcome::Stopped);
        };
        let baseline = Baseline::new(&code, &review);
        let Some(planted) = self
            .stage(context, Stage::Plant, || self.plant(spec, &baseline))
            .await?
        else {
            return Ok(JobOutcome::Stopped);
        };
        let Some(proved) = self
            .stage(context, Stage::Prove, || {
                self.prove(spec, &baseline, &planted)
            })
            .await?
        else {
            return Ok(JobOutcome::Stopped);
        };
        let Some(blind) = self
            .stage(context, Stage::BlindReview, || {
                self.blind_review(spec, &baseline, &proved)
            })
            .await?
        else {
            return Ok(JobOutcome::Stopped);
        };
        let at = STORE_INDEX.max(context.job().progress_current);
        let label = format!("Storing the drill ({} of {STEPS})", STORE_INDEX + 1);
        if !context.progress(at, &label, None).await? {
            return Ok(JobOutcome::Stopped);
        }
        let built = store::Built {
            baseline: &baseline,
            code: &code,
            review: &review,
            planted: &planted,
            proved: &proved,
            blind: &blind,
        };
        self.store(context, spec, built).await
    }

    /// The output of `stage`: the checkpoint when one was saved, otherwise what `work` makes, saved before it is
    /// returned. `None` when the job was cancelled or is no longer running. Model calls and runs happen inside
    /// `work`, which the job's cancellation drops.
    async fn stage<T, F, Fut>(
        &self,
        context: &JobContext,
        stage: Stage,
        work: F,
    ) -> Result<Option<T>>
    where
        T: Serialize + DeserializeOwned,
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        let key = stage.key();
        if let Some(saved) = context.checkpoint(&key).await? {
            if let Ok(output) = serde_json::from_value::<T>(saved) {
                return Ok(Some(output));
            }
        }
        let at = stage.index().max(context.job().progress_current);
        let label = format!("{} ({} of {STEPS})", stage.label(), stage.index() + 1);
        if !context.progress(at, &label, None).await? {
            return Ok(None);
        }
        let Some(output) = context.until_cancelled(work()).await else {
            return Ok(None);
        };
        let output = output?;
        let saved = serde_json::to_value(&output)?;
        if !context.put_checkpoint(&key, &saved).await? {
            return Ok(None);
        }
        tracing::info!(stage = stage.name(), "A drill build stage is done");
        Ok(Some(output))
    }
}
