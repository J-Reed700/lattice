//! What a drill is made of. The build, the proofs, the fix grading and the judge are one pipeline for every kind of
//! drill (docs/design/2026-11-05-spot-the-errors-drills.md section 4); a `DrillMaterial` is the Strategy that holds
//! the parts of it that depend on the kind of material: how the clean material is written and reviewed, what the
//! planter and the reviewers are told, and how a planted defect is proven and a learner's fix is checked.
//!
//! `CodeMaterial` is the only one so far.

mod code;

pub(in crate::features::learning) use code::CodeMaterial;

use super::build::{Baseline, CleanCode, CleanReview, Mutation};
use super::fix_grading::FixStrategy;
use super::proof::{ProofStrategy, Tools};
use super::runner::CodeRunner;
use super::spec::DrillSpec;
use async_trait::async_trait;
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::drills::{KeyIssue, Material, Pattern};
use lattice_core::shared::error::{AppError, Result};
use sqlx::SqlitePool;
use tokio_util::sync::CancellationToken;

/// What a material's stages are given to work with: the model, the database and the build's cancellation.
pub(in crate::features::learning) struct StageCtx<'a> {
    pub llm: &'a dyn LLMPort,
    pub pool: &'a SqlitePool,
    pub cancel: &'a CancellationToken,
}

/// What the planter is told and shown.
pub(in crate::features::learning) struct PlantWording {
    /// The planter's instructions.
    pub system: String,
    /// The material, the patterns to plant and whatever else the planter is shown.
    pub prompt: String,
    /// Whether each change comes with a test that observes it, or the reason none can.
    pub asks_verification: bool,
}

/// What the blind reviewer and the describer of a problem it found are told.
pub(in crate::features::learning) struct ReviewerWording {
    /// The blind reviewer's instructions.
    pub reviewer: &'static str,
    /// The instructions for describing a valid extra as an entry of the key.
    pub describer: &'static str,
    /// The line that heads the material in both prompts.
    pub subject: String,
}

/// What the judge of a review is told, and how it names the material.
pub(in crate::features::learning) struct JudgeWording {
    /// The judge's instructions.
    pub system: &'static str,
    /// What the prose step of the two-step call works through.
    pub brief: &'static str,
    /// What the task's first line puts in brackets after the drill's title.
    pub kind: String,
    /// The heading of the material as the reviewer saw it.
    pub reviewed: &'static str,
    /// The heading of the material before any issue was planted.
    pub original: &'static str,
}

impl JudgeWording {
    /// The wording for drills of `material`.
    pub(in crate::features::learning) fn of(material: &Material) -> Result<Self> {
        Ok(code::judge_wording(code_language(material)?))
    }
}

/// The parts of a drill's build, proofs and grading that depend on what is reviewed.
#[async_trait]
pub(in crate::features::learning) trait DrillMaterial:
    Send + Sync
{
    /// What the proofs and the hidden suite run on, when the material has anything to run.
    fn runner(&self) -> Option<&dyn CodeRunner>;

    /// Stage 1. The clean files and whatever the proofs need later: for code, the suite.
    async fn clean(&self, spec: &DrillSpec, ctx: &StageCtx<'_>) -> Result<CleanCode>;

    /// Stage 2. Repairs the clean material until it has no issue of its own.
    async fn clean_review(
        &self,
        spec: &DrillSpec,
        code: &CleanCode,
        ctx: &StageCtx<'_>,
    ) -> Result<CleanReview>;

    /// Stage 3. What the planter is told and shown for `targets` in the material of `baseline`.
    fn plant_wording(
        &self,
        spec: &DrillSpec,
        baseline: &Baseline,
        targets: &[&'static Pattern],
    ) -> PlantWording;

    /// Stage 4. How `mutation`, an instance of `pattern`, is proven, on `tools`.
    fn proof_strategy<'a>(
        &self,
        pattern: &Pattern,
        mutation: &Mutation,
        tools: Tools<'a>,
    ) -> Box<dyn ProofStrategy + 'a>;

    /// How a learner's fix of `issue` is checked, on `tools`.
    fn fix_strategy<'a>(&self, issue: &KeyIssue, tools: Tools<'a>) -> Box<dyn FixStrategy + 'a>;

    /// Stage 5. What the blind reviewer and the describer are told.
    fn reviewer_wording(&self, spec: &DrillSpec) -> ReviewerWording;
}

/// The language of the code `material` is, or why it is not code.
pub(in crate::features::learning) fn code_language(material: &Material) -> Result<&str> {
    material
        .language()
        .ok_or_else(|| AppError::InvalidInput("Drills on a passage are not supported yet.".into()))
}
