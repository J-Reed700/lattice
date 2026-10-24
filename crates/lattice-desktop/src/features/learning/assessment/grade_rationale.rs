//! Why a model-graded answer earned what it did (docs/design/2026-10-23-
//! quizzes-and-tests.md, section 4.3a), checked in code before anyone sees it.
//!
//! The grader returns, per rubric criterion, a verdict, the points, what was
//! missing, the learner's own words it rests on, what full credit includes and
//! a source quote. Quotes must be verbatim; points must agree with the
//! verdict; and a grade that loses points without saying why is not a grade:
//! it is a grader failure, which reads "Couldn't grade yet" and never "Not
//! correct".
use crate::features::learning::assessment_engine::LearningRubricCriterion;
use crate::features::learning::dto::{
    LearningAssessmentGradeStatus, LearningCriterionRationaleDto, LearningCriterionVerdict,
    LearningGradeRationaleDto,
};
use serde::Deserialize;

/// A grader's verdict on one criterion, before any check.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GraderVerdict {
    Met,
    PartlyMet,
    Missed,
}

/// One criterion as the grader returned it. Field order in the schema puts
/// the reasoning first and the points last.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraderCriterion {
    pub criterion_id: String,
    #[serde(default)]
    pub your_words: Vec<String>,
    #[serde(default)]
    pub what_was_missing: String,
    #[serde(default)]
    pub full_credit_includes: String,
    #[serde(default)]
    pub source_quote: Option<String>,
    pub verdict: GraderVerdict,
    pub points: f64,
}

/// One item as the grader returned it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraderItem {
    pub criteria: Vec<GraderCriterion>,
    pub summary: String,
}

/// The passage that establishes the expected points: the item's stored
/// reference quote and the source version it came from.
#[derive(Debug, Clone, Copy)]
pub struct Reference<'a> {
    pub text: &'a str,
    pub version_id: Option<&'a str>,
}

/// Who graded: shown in words beside every model grade.
#[derive(Debug, Clone)]
pub struct Grader<'a> {
    pub model: &'a str,
    pub graded_at: i64,
}

/// A checked grade, or the reason it cannot be shown.
#[derive(Debug, Clone)]
pub enum Checked {
    Graded {
        rationale: LearningGradeRationaleDto,
        /// Points earned over the rubric's points.
        fraction: f64,
        status: LearningAssessmentGradeStatus,
    },
    /// "Couldn't grade yet": the item stays pending with Retry grading.
    Unexplained(String),
}

pub const UNEXPLAINED: &str = "Couldn't grade yet: the model didn't explain this grade.";

fn spaced(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether `quote` appears verbatim in `text` once whitespace is normalised.
pub fn verbatim(text: &str, quote: &str) -> bool {
    let quote = spaced(quote);
    !quote.is_empty() && spaced(text).contains(&quote)
}

/// A short, stable name for the rubric a grade was given against.
pub fn rubric_version(rubric: &[LearningRubricCriterion]) -> String {
    let joined = rubric
        .iter()
        .map(|c| format!("{}|{}|{}|{}", c.id, c.title, c.description, c.max_points))
        .collect::<Vec<_>>()
        .join("\n");
    let digest = crate::features::learning::content_verification::digest(&joined);
    format!("rubric:{}", digest.chars().take(12).collect::<String>())
}

/// Check a grader's answer for one item against its rubric, the learner's
/// answer and the reference passage.
pub fn check(
    rubric: &[LearningRubricCriterion],
    answer: &str,
    reference: Option<Reference<'_>>,
    raw: GraderItem,
    grader: &Grader<'_>,
) -> Checked {
    if raw.criteria.len() != rubric.len() {
        return Checked::Unexplained(UNEXPLAINED.into());
    }
    let mut criteria = Vec::with_capacity(rubric.len());
    let mut earned = 0.0;
    let mut possible = 0.0;
    let mut lost_points = false;
    let mut uncertain = false;
    for criterion in rubric {
        let Some(graded) = raw.criteria.iter().find(|g| g.criterion_id == criterion.id) else {
            return Checked::Unexplained(UNEXPLAINED.into());
        };
        let max = f64::from(criterion.max_points);
        // The verdict is trusted over the points; points that disagree with
        // it are recomputed, never shown as returned.
        let points = match graded.verdict {
            GraderVerdict::Met => max,
            GraderVerdict::Missed => 0.0,
            GraderVerdict::PartlyMet if graded.points > 0.0 && graded.points < max => graded.points,
            GraderVerdict::PartlyMet => max / 2.0,
        };
        let missing = graded.what_was_missing.trim().to_string();
        if points < max {
            lost_points = true;
            if missing.is_empty() {
                return Checked::Unexplained(UNEXPLAINED.into());
            }
        }
        let words: Vec<String> = graded
            .your_words
            .iter()
            .map(|w| w.trim().to_string())
            .filter(|w| !w.is_empty())
            .collect();
        let kept: Vec<String> = words
            .iter()
            .filter(|w| verbatim(answer, w))
            .cloned()
            .collect();
        let source_quote = graded
            .source_quote
            .as_deref()
            .map(str::trim)
            .filter(|q| !q.is_empty());
        let source_ok = match (source_quote, reference) {
            (None, _) => true,
            (Some(quote), Some(reference)) => verbatim(reference.text, quote),
            (Some(_), None) => false,
        };
        let criterion_uncertain = kept.len() != words.len() || !source_ok;
        uncertain |= criterion_uncertain;
        earned += points;
        possible += max;
        criteria.push(LearningCriterionRationaleDto {
            criterion_id: criterion.id.clone(),
            title: criterion.title.clone(),
            verdict: if criterion_uncertain {
                LearningCriterionVerdict::Uncertain
            } else {
                match graded.verdict {
                    GraderVerdict::Met => LearningCriterionVerdict::Met,
                    GraderVerdict::PartlyMet => LearningCriterionVerdict::PartlyMet,
                    GraderVerdict::Missed => LearningCriterionVerdict::Missed,
                }
            },
            points: Some(points),
            max_points: max,
            what_was_missing: missing.chars().take(1_200).collect(),
            your_words: kept,
            full_credit_includes: graded
                .full_credit_includes
                .trim()
                .chars()
                .take(800)
                .collect(),
            source_quote: if source_ok {
                source_quote.map(str::to_owned)
            } else {
                None
            },
            source_version_id: if source_ok && source_quote.is_some() {
                reference.and_then(|r| r.version_id.map(str::to_owned))
            } else {
                None
            },
        });
    }
    let summary = raw.summary.trim().to_string();
    if lost_points && summary.is_empty() {
        return Checked::Unexplained(UNEXPLAINED.into());
    }
    Checked::Graded {
        rationale: LearningGradeRationaleDto {
            summary: summary.chars().take(1_200).collect(),
            criteria,
            grader_model: grader.model.to_string(),
            rubric_version: rubric_version(rubric),
            graded_at: grader.graded_at,
        },
        fraction: if possible > 0.0 {
            earned / possible
        } else {
            0.0
        },
        status: if uncertain {
            LearningAssessmentGradeStatus::Uncertain
        } else {
            LearningAssessmentGradeStatus::Provisional
        },
    }
}

/// The one-criterion rubric a short answer is judged against when it matched
/// no accepted variant.
pub fn equivalence_rubric() -> Vec<LearningRubricCriterion> {
    vec![LearningRubricCriterion {
        id: EQUIVALENCE_CRITERION.into(),
        title: "Says the same as the accepted answer".into(),
        description: "The answer means the same thing as an accepted answer, in other words."
            .into(),
        max_points: 1,
    }]
}

pub const EQUIVALENCE_CRITERION: &str = "00000000-0000-4000-8000-00000000e001";
