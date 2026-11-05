//! What came of a learner's fix to a drill: for each key issue whether the change fixes it and what decided that,
//! and whether the change broke what worked. The form stored with an attempt is the form the review view reads.

use serde::{Deserialize, Serialize};

/// What a result says when the hidden suite fails on an issue's fixed files.
const BROKE: &str = "Your changes break what worked: the hidden suite fails on your files.";

/// Whether a learner's change fixes one issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum FixOutcome {
    Fixed,
    /// The change fixes part of the issue, or fixes it and breaks something that worked.
    Partly,
    NotFixed,
}

/// What decided an issue's outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum DecidedBy {
    /// The issue's killing test, run on the learner's files.
    Test,
    /// The hidden suite, run on the learner's files.
    Suite,
    /// A judge, shown the code, the learner's code and the issue.
    Judge,
}

/// What the hidden suite made of the learner's files as a whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SuiteVerdict {
    /// The suite passes: nothing that worked is broken.
    Passed,
    /// The suite fails: the fix broke behaviour that worked. `output` is what it wrote.
    Failed { output: String },
    /// The suite proved nothing: the drill has none, no runtime was there to run it, or the run did not finish.
    NotRun { why: String },
}

/// What one issue's own check found: its killing test run on the learner's files, or a judge's reading of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueCheck {
    pub outcome: FixOutcome,
    /// `Test` or `Judge`.
    pub decided_by: DecidedBy,
    pub reason: String,
    /// What the test wrote, when it failed.
    pub output: Option<String>,
    /// Why the issue's test could not decide it and a judge did instead.
    pub fallback: Option<String>,
    /// The other issues, by position in the key, whose key fixes were applied to the learner's files for the issue's
    /// test: none when a judge decided the issue.
    pub graded_beside: Vec<usize>,
}

/// One key issue's result, by its position in the key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct IssueFix {
    pub issue: usize,
    pub outcome: FixOutcome,
    pub decided_by: DecidedBy,
    pub reason: String,
    /// What the killing test wrote when it failed, or what the suite wrote when it decided the issue.
    pub output: Option<String>,
    /// Why the issue's test could not decide it and a judge did instead.
    pub fallback: Option<String>,
    /// The other issues, by position in the key, whose key fixes were applied to the learner's files for this issue's
    /// test, so that the issue was graded beside them: none when a judge decided the issue.
    pub graded_beside: Vec<usize>,
    /// The hidden suite's verdict on the learner's files: the same for every issue of the fix.
    pub suite: SuiteVerdict,
}

impl IssueFix {
    /// The result for key issue `issue`, from its own check and the suite's verdict on the learner's files. The
    /// suite leaves what the check found as it is, except that an issue the check calls fixed is only partly fixed
    /// when the suite fails: the change fixes it and breaks what worked. Every result carries the verdict.
    pub fn settle(issue: usize, check: IssueCheck, suite: &SuiteVerdict) -> Self {
        let broken = match suite {
            SuiteVerdict::Failed { output } if check.outcome == FixOutcome::Fixed => Some(output),
            _ => None,
        };
        match broken {
            Some(output) => Self {
                issue,
                outcome: FixOutcome::Partly,
                decided_by: DecidedBy::Suite,
                reason: format!("{} {BROKE}", check.reason),
                output: Some(output.clone()),
                fallback: check.fallback,
                graded_beside: check.graded_beside,
                suite: suite.clone(),
            },
            None => Self {
                issue,
                outcome: check.outcome,
                decided_by: check.decided_by,
                reason: check.reason,
                output: check.output,
                fallback: check.fallback,
                graded_beside: check.graded_beside,
                suite: suite.clone(),
            },
        }
    }
}
