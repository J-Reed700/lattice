//! The assessment the judge's answers make. The relation between an issue and a finding is read from the issues'
//! side only; what a finding matches is derived from it.

use lattice_core::domain::learning::drills::{Assessment, FindingOutcome, IssueOutcome};
use lattice_core::shared::error::{AppError, Result};

use super::contract::{Credit, Extra, Judgement, Verdict};

/// What the issues' answers say: an outcome for every issue, in key order, and for every finding the issues it is
/// about, each with the reason its issue was given.
pub(super) struct Credits {
    issues: Vec<(IssueOutcome, String)>,
    about: Vec<Vec<(usize, String)>>,
}

pub(super) fn unusable(what: &str) -> AppError {
    AppError::ModelAnswerUnusable(format!("The judge's answer is not usable: {what}."))
}

/// The credits the answers for the issues at key positions `judged` make, for a key of `key_len` issues and a review
/// of `findings_len` findings.
pub(super) fn credits(
    key_len: usize,
    findings_len: usize,
    judged: &[(usize, Judgement)],
) -> Result<Credits> {
    let mut issues: Vec<Option<(IssueOutcome, String)>> = vec![None; key_len];
    let mut about: Vec<Vec<(usize, String)>> = vec![Vec::new(); findings_len];
    for (position, judgement) in judged {
        let reason = judgement.reason.trim().to_owned();
        let credited = judgement.finding.as_ref().map(|picked| picked.reference);
        let outcome = match (judgement.outcome, credited) {
            (Credit::Missed, _) => IssueOutcome::Missed,
            (Credit::Found, Some(finding)) => IssueOutcome::Found { finding },
            (Credit::HalfFound, Some(finding)) => IssueOutcome::HalfFound { finding },
            (Credit::Found | Credit::HalfFound, None) => {
                return Err(unusable("an issue is credited to no finding"))
            }
        };
        if !matches!(outcome, IssueOutcome::Missed) {
            let named = judgement.about.iter().map(|picked| picked.reference);
            for finding in credited.into_iter().chain(named) {
                if let Some(list) = about.get_mut(finding) {
                    if !list.iter().any(|(issue, _)| issue == position) {
                        list.push((*position, reason.clone()));
                    }
                }
            }
        }
        if let Some(slot) = issues.get_mut(*position) {
            *slot = Some((outcome, reason));
        }
    }
    let issues = issues
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| unusable("an issue has no entry"))?;
    Ok(Credits { issues, about })
}

impl Credits {
    /// The positions of the findings no issue is about.
    pub(super) fn unclaimed(&self) -> Vec<usize> {
        self.about
            .iter()
            .enumerate()
            .filter(|(_, issues)| issues.is_empty())
            .map(|(finding, _)| finding)
            .collect()
    }

    /// The assessment, with `extras` as the verdicts on the findings `unclaimed` named.
    pub(super) fn assessment(self, extras: &[(usize, Verdict)]) -> Result<Assessment> {
        let mut findings = Vec::with_capacity(self.about.len());
        for (position, mut issues) in self.about.into_iter().enumerate() {
            if issues.is_empty() {
                let verdict = extras
                    .iter()
                    .find(|(finding, _)| *finding == position)
                    .map(|(_, verdict)| verdict)
                    .ok_or_else(|| unusable("a finding has no entry"))?;
                let outcome = match verdict.outcome {
                    Extra::ValidExtra => FindingOutcome::ValidExtra,
                    Extra::FalsePositive => FindingOutcome::FalsePositive,
                };
                findings.push((outcome, verdict.reason.trim().to_owned()));
                continue;
            }
            issues.sort_by_key(|(issue, _)| *issue);
            let mut reasons: Vec<String> = Vec::new();
            for (_, reason) in &issues {
                if !reasons.contains(reason) {
                    reasons.push(reason.clone());
                }
            }
            findings.push((
                FindingOutcome::Matches {
                    issues: issues.into_iter().map(|(issue, _)| issue).collect(),
                },
                reasons.join(" "),
            ));
        }
        Ok(Assessment {
            issues: self.issues,
            findings,
        })
    }
}
