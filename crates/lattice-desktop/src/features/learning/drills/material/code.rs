//! Code as the material of a drill: the clean code and its suite are written by the model and run on a runner where
//! the language has one, a defect is proven by a killing test, by behaviour kept or by a judge, and a learner's fix is
//! run or judged.

mod blind;
mod clean;
mod judge;
mod plant;
mod review;

pub(super) use judge::judge_wording;

use super::{DrillMaterial, PlantWording, ReviewerWording, StageCtx};
use crate::features::learning::drills::build::{Baseline, CleanCode, CleanReview, Mutation};
use crate::features::learning::drills::fix_grading::{self, FixStrategy};
use crate::features::learning::drills::proof::{self, ProofStrategy, Tools};
use crate::features::learning::drills::runner::CodeRunner;
use crate::features::learning::drills::spec::DrillSpec;
use async_trait::async_trait;
use lattice_core::domain::learning::drills::{KeyIssue, Pattern};
use lattice_core::shared::error::Result;

/// Code, run on `runner` where the language has one.
pub(in crate::features::learning) struct CodeMaterial<'a> {
    runner: Option<&'a dyn CodeRunner>,
}

impl<'a> CodeMaterial<'a> {
    /// Code whose suite and killing tests run on `runner`, or, without one, whose defects are all proven by
    /// judgement.
    pub(in crate::features::learning) fn new(runner: Option<&'a dyn CodeRunner>) -> Self {
        Self { runner }
    }
}

/// The language of the code the spec asks for.
fn language_of(spec: &DrillSpec) -> &str {
    spec.material.language().unwrap_or_default()
}

#[async_trait]
impl DrillMaterial for CodeMaterial<'_> {
    fn runner(&self) -> Option<&dyn CodeRunner> {
        self.runner
    }

    async fn clean(&self, spec: &DrillSpec, ctx: &StageCtx<'_>) -> Result<CleanCode> {
        self.write_clean(spec, ctx).await
    }

    async fn clean_review(
        &self,
        spec: &DrillSpec,
        code: &CleanCode,
        ctx: &StageCtx<'_>,
    ) -> Result<CleanReview> {
        self.review_clean(spec, code, ctx).await
    }

    fn plant_wording(
        &self,
        spec: &DrillSpec,
        baseline: &Baseline,
        targets: &[&'static Pattern],
    ) -> PlantWording {
        self.code_plant_wording(language_of(spec), baseline, targets, spec.lesson_issues())
    }

    fn proof_strategy<'a>(
        &'a self,
        pattern: &Pattern,
        mutation: &Mutation,
        tools: Tools<'a>,
    ) -> Box<dyn ProofStrategy + 'a> {
        let kind = proof::choose(
            pattern.family,
            self.runner.is_some(),
            &mutation.verification,
        );
        proof::strategy(kind, tools)
    }

    fn fix_strategy<'a>(&'a self, issue: &KeyIssue, tools: Tools<'a>) -> Box<dyn FixStrategy + 'a> {
        fix_grading::strategy(fix_grading::choose(issue), tools)
    }

    fn reviewer_wording(&self, spec: &DrillSpec) -> ReviewerWording {
        ReviewerWording {
            reviewer: blind::REVIEWER,
            describer: blind::DESCRIBER,
            subject: format!("Language: {}", language_of(spec)),
            files: "The code:",
        }
    }
}
