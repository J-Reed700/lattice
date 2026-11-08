//! An explanation of a passage of a course's source as the material of a drill
//! (docs/design/2026-11-05-spot-the-errors-drills.md sections 2, 4 and 5). The passage is taken from the course's
//! sources, for the lesson the drill is built from; the model explains it, one sentence to a line, and every sentence
//! the source does not support is rewritten from it or dropped. A planted error is proven against the source: the
//! sentence it replaces is supported with an exact quote, and the planted one is contradicted or unsupported. A
//! learner's fix of such an error is checked against the source the same way.

mod blind;
mod check;
mod clean;
mod fix;
mod judge;
mod pick;
mod plant;
mod prove;
mod review;
mod teaches;

#[cfg(test)]
pub(crate) use check::finding_of;
pub use check::ClaimSourceCheck;
pub(crate) use check::{SourceCheck, SourceFinding, SourceSays};
pub(super) use judge::judge_wording;

use super::{DrillMaterial, PlantWording, ReviewerWording, StageCtx};
use crate::drills::build::{Baseline, CleanCode, CleanReview, Mutation, SourcePassage};
use crate::drills::fix_grading::{self, FixStrategy};
use crate::drills::proof::{self, ProofStrategy, StrategyKind, Tools};
use crate::drills::runner::CodeRunner;
use crate::drills::spec::DrillSpec;
use async_trait::async_trait;
use lattice_core::domain::learning::drills::{Family, KeyIssue, Pattern, Proof};
use lattice_core::shared::error::Result;

/// The path of the one file of a drill on a passage.
pub(crate) const EXPLANATION_PATH: &str = "explanation.md";

/// An explanation of a passage, checked against the passage by `check`.
pub(crate) struct PassageMaterial<'a> {
    check: &'a dyn SourceCheck,
}

impl<'a> PassageMaterial<'a> {
    pub(crate) fn new(check: &'a dyn SourceCheck) -> Self {
        Self { check }
    }
}

#[async_trait]
impl DrillMaterial for PassageMaterial<'_> {
    fn runner(&self) -> Option<&dyn CodeRunner> {
        None
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
        _spec: &DrillSpec,
        baseline: &Baseline,
        targets: &[&'static Pattern],
    ) -> PlantWording {
        plant::wording(baseline, targets)
    }

    fn proof_strategy<'a>(
        &'a self,
        pattern: &Pattern,
        _mutation: &Mutation,
        tools: Tools<'a>,
    ) -> Box<dyn ProofStrategy + 'a> {
        if pattern.family == Family::Consistency {
            return proof::strategy(StrategyKind::Judged, tools);
        }
        Box::new(prove::SourceChecked::new(self.check))
    }

    fn fix_strategy<'a>(&'a self, issue: &KeyIssue, tools: Tools<'a>) -> Box<dyn FixStrategy + 'a> {
        match issue.proof {
            Proof::Sourced { .. } => Box::new(fix::SourceCheckedFix::new(self.check, tools)),
            Proof::Killed { .. } | Proof::Judged { .. } => {
                fix_grading::strategy(fix_grading::StrategyKind::Judged, tools)
            }
        }
    }

    fn reviewer_wording(&self, _spec: &DrillSpec) -> ReviewerWording {
        ReviewerWording {
            reviewer: blind::REVIEWER,
            describer: blind::DESCRIBER,
            subject: "An explanation of a passage of a course's source, one sentence to a line"
                .into(),
            files: "The explanation:",
        }
    }

    fn reviewer_context(&self, baseline: &Baseline) -> Option<String> {
        baseline.source.as_ref().map(source_block)
    }
}

/// What a source check is called in a trail: `purpose`, and whether the check gave no usable verdict.
pub(super) fn check_purpose(purpose: &str, found: &SourceFinding) -> String {
    if found.unusable {
        format!("{purpose} (no usable verdict)")
    } else {
        purpose.to_owned()
    }
}

/// The source passage as a prompt shows it, under its title.
pub(super) fn source_block(source: &SourcePassage) -> String {
    format!(
        "The source passage, from {}:\n<passage>\n{}\n</passage>",
        source.title, source.text
    )
}

/// The fewest sentences an explanation can have and still take `targets`: a sentence for each pattern, with an
/// unchanged sentence between any two changes, and for a contradiction a second sentence for it to contradict.
pub(super) fn sentences_needed(targets: &[&Pattern]) -> usize {
    let apart = targets.len().saturating_mul(2).saturating_sub(1);
    let contradicts = targets
        .iter()
        .any(|pattern| pattern.family == Family::Consistency);
    apart.max(if contradicts { 2 } else { 1 })
}

/// An explanation's text: one sentence to a line, no empty line, the last line ended, so that the sentences are
/// numbered 1 to n by the line each is on.
pub(super) fn explanation_text(sentences: &[String]) -> String {
    let mut text = sentences.join("\n");
    text.push('\n');
    text
}

/// The sentences of an explanation's text, one for each line that says something.
pub(super) fn sentence_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}
