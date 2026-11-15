//! Stage 6: a fresh reviewer, told nothing of the key, reviews the drill's files. The grader, the one judgement of a
//! review against a key, judges its findings. A finding the grader calls a valid extra is described as an entry of
//! the key, so that a learner who finds it is credited; how findable each planted issue was is kept as evidence.

use super::ask::normalise;
use super::code_units::{CodeFile, CodeUnits, Weight};
use super::outputs::{
    Baseline, BlindReview, Findability, LessonChecked, Proved, ReviewedFinding, Trail,
};
use super::DrillBuild;
use crate::drills::budget;
use crate::drills::grader::Review;
use crate::drills::spec::DrillSpec;
use crate::lessons::structured_call::Asking;
use crate::model_call;
use lattice_core::application::services::model_contract::ask::{
    ask, ask_new, failed, Alone, Answered, Refusal, Says, Settled, Why,
};
use lattice_core::application::services::model_contract::edit::Consistent;
use lattice_core::application::services::model_contract::shape::{
    Authored, Fields, Form, New, Union, Value,
};
use lattice_core::application::services::model_contract::shown::{
    Drawn, Internal, Kind, Picked, Scope, Showing, Told,
};
use lattice_core::application::services::model_contract::units::{Grain, Unit, Units};
use lattice_core::application::services::model_contract::{Contract, ContractId};
use lattice_core::domain::learning::drills::{
    apply, overlaps, patterns_for, validate, Drill, DrillFile, Edit, EditFault, Finding,
    FindingOutcome, KeyIssue, LineSpan, Origin, Pattern, Proof,
};
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};

/// Said after the reviewer's wording: the form of its answer.
const REVIEW_FORM: &str = " Answer with an object whose `items` list holds one object for each finding, with the fields `from`, `to`, `comment` and `severity`. `severity` is one of critical, major, minor, nit. With no findings, `items` is empty.";

/// Said after the describer's wording: the form of its answer.
const ENTRY_FORM: &str = " Answer with an object whose `entry` is an object with the field `kind`, which is `none` or `issue`. An `issue` has the fields `pattern`, `severity`, `consequence`, `fix` and `explanation`; `severity` is one of critical, major, minor, nit, and `fix` is the change to the lines of the file that repairs the problem.";

/// The call that reviews the drill's files without knowing the key.
struct BlindReviewAsk;

impl Contract for BlindReviewAsk {
    const ID: ContractId = ContractId::new("blind-review");
    type Input = ReviewInput;
}

/// What one blind review reads.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReviewInput {
    wording: Told<String>,
    subject: Told<String>,
    files: Vec<CodeFile>,
    output_tokens: Internal<usize>,
}

/// A finding of the reviewer: the lines it is about, and what is wrong with them.
#[derive(Debug, Clone)]
struct Remark {
    from: Picked<Unit>,
    to: Picked<Unit>,
    comment: String,
    severity: Weight,
}

impl Authored for Remark {
    fn form() -> Form {
        Form::new()
            .point::<Unit>("from")
            .point::<Unit>("to")
            .text("comment")
            .word::<Weight>("severity")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            from: fields.point::<Unit>("from")?,
            to: fields.point::<Unit>("to")?,
            comment: fields.text("comment")?,
            severity: fields.word::<Weight>("severity")?,
        })
    }
}

/// The lines of one file a remark is about, counting from zero.
struct Block {
    file: usize,
    first: usize,
    last: usize,
}

/// The file a shown line is in, and its place there counting from zero.
fn place(files: &[CodeFile], line: &Picked<Unit>) -> Option<(usize, usize)> {
    let file = files.iter().position(|file| {
        Units::of(&file.text.0, Grain::Line)
            .text_of(line.reference)
            .is_some()
    })?;
    Some((file, line.reference.index()))
}

impl Remark {
    /// The lines it is about, when both ends are lines of one file and the first is not after the last.
    fn block(&self, files: &[CodeFile]) -> Option<Block> {
        let (file, first) = place(files, &self.from)?;
        let (other, last) = place(files, &self.to)?;
        (file == other && first <= last).then_some(Block { file, first, last })
    }
}

impl Answered for BlindReviewAsk {
    type Answer = New<Remark>;
    const ALONE: Alone = Alone::Fails;

    fn says(input: &ReviewInput, showing: &mut Showing<'_>) -> Result<Says> {
        let files = CodeUnits::of(&input.files).rendered(showing)?;
        let mut says = Says::new(
            format!("{}{REVIEW_FORM}", input.wording.0),
            format!("{}\n\n{files}", input.subject.0),
        );
        says.output_tokens = *input.output_tokens.value();
        Ok(says)
    }

    fn accept(input: &ReviewInput, remark: &Remark) -> std::result::Result<(), Refusal> {
        match remark.block(&input.files) {
            Some(_) => Ok(()),
            None => Err(Refusal(
                "`from` and `to` must be lines of one file, with `from` not after `to`.".into(),
            )),
        }
    }
}

/// A pattern of the catalogue the describer may name.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Candidate {
    id: Internal<String>,
    name: Told<String>,
    definition: Told<String>,
}

impl Candidate {
    fn of(pattern: &Pattern) -> Self {
        Self {
            id: Internal::of(pattern.id.to_owned()),
            name: Told(pattern.name.to_owned()),
            definition: Told(pattern.definition.to_owned()),
        }
    }
}

impl Kind for Candidate {
    const PREFIX: &'static str = "p";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = String;

    fn reference(&self) -> String {
        self.id.value().clone()
    }
}

/// The call that describes a finding the grader called a valid extra as an entry of the key.
struct KeyEntryAsk;

impl Contract for KeyEntryAsk {
    const ID: ContractId = ContractId::new("key-entry");
    type Input = EntryInput;
}

/// What one description reads.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct EntryInput {
    wording: Told<String>,
    subject: Told<String>,
    heading: Told<String>,
    files: Vec<CodeFile>,
    path: Told<String>,
    quote: Told<String>,
    comment: Told<String>,
    reason: Told<String>,
    patterns: Vec<Candidate>,
    output_tokens: Internal<usize>,
}

/// What a described finding comes to.
#[derive(Debug, Clone)]
enum Described {
    /// An instance of no pattern of the catalogue: nothing joins the key.
    Nothing,
    Issue {
        pattern: Picked<Candidate>,
        severity: Weight,
        consequence: String,
        fix: Consistent<Unit, String>,
        explanation: String,
    },
}

impl Union for Described {
    fn forms() -> Vec<(&'static str, Form)> {
        vec![
            ("none", Form::new()),
            (
                "issue",
                Form::new()
                    .point::<Candidate>("pattern")
                    .word::<Weight>("severity")
                    .text("consequence")
                    .change::<Unit, String>("fix")
                    .text("explanation"),
            ),
        ]
    }

    fn read(kind: &str, fields: &mut Fields<'_, '_>) -> Option<Self> {
        match kind {
            "none" => Some(Self::Nothing),
            "issue" => Some(Self::Issue {
                pattern: fields.point::<Candidate>("pattern")?,
                severity: fields.word::<Weight>("severity")?,
                consequence: fields.text("consequence")?,
                fix: fields.change::<Unit, String>("fix")?,
                explanation: fields.text("explanation")?,
            }),
            _ => None,
        }
    }
}

/// The entry of the key a described finding is.
#[derive(Debug, Clone)]
struct Entry(Described);

impl Authored for Entry {
    fn form() -> Form {
        Form::new().one_of::<Described>("entry")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        fields.one_of::<Described>("entry").map(Self)
    }
}

/// The edit a fix makes to the files, with the lines around it that make it occur once, or what to tell the model about a fix that cannot be one.
fn fix_of(
    files: &[CodeFile],
    fix: &Consistent<Unit, String>,
) -> std::result::Result<Edit, Refusal> {
    let units = CodeUnits::of(files);
    units.file_of(fix.change())?;
    if units.changes_nothing(fix.change()) {
        return Err(Refusal("The fix changes nothing.".into()));
    }
    if let Settled::Unanswered(Why::Contradicted(told)) =
        CodeUnits::of(files).apply(fix.clone())
    {
        return Err(Refusal(told));
    }
    let Some(rewrite) = CodeUnits::of(files).rewrite(fix.clone()) else {
        return Err(Refusal(
            "The fix does not apply to the code. Change the lines shown.".into(),
        ));
    };
    let drill: Vec<DrillFile> = files
        .iter()
        .map(|file| DrillFile {
            path: file.path.0.clone(),
            text: file.text.0.clone(),
        })
        .collect();
    for extra in 0..=rewrite.lines() {
        let Some(edit) = rewrite.edit(extra) else {
            break;
        };
        match apply(&drill, std::slice::from_ref(&edit)) {
            Ok(_) => return Ok(edit),
            Err(EditFault::Ambiguous { .. }) => {}
            Err(_) => break,
        }
    }
    Err(Refusal(
        "The fix does not apply to the code. Change the lines shown.".into(),
    ))
}

impl Answered for KeyEntryAsk {
    type Answer = Value<Entry>;
    const ALONE: Alone = Alone::Pending;

    fn says(input: &EntryInput, showing: &mut Showing<'_>) -> Result<Says> {
        let files = CodeUnits::of(&input.files).rendered(showing)?;
        let patterns = showing.show(&input.patterns)?;
        let mut task = format!(
            "{}\n\n{}\n\n{files}\n\nThe comment, on {}:\n<quote>\n{}\n</quote>\n<comment>\n{}\n</comment>\nThe judge's reason: {}\n\nThe patterns:\n",
            input.subject.0,
            input.heading.0,
            input.path.0,
            input.quote.0,
            input.comment.0,
            input.reason.0,
        );
        for (handle, pattern) in patterns.iter() {
            task.push_str(&format!(
                "- {} ({}): {}\n",
                handle.bracketed(),
                pattern.name.0,
                pattern.definition.0
            ));
        }
        let mut says = Says::new(format!("{}{ENTRY_FORM}", input.wording.0), task);
        says.output_tokens = *input.output_tokens.value();
        Ok(says)
    }

    fn accept(input: &EntryInput, entry: &Entry) -> std::result::Result<(), Refusal> {
        let Described::Issue {
            consequence,
            explanation,
            fix,
            ..
        } = &entry.0
        else {
            return Ok(());
        };
        if consequence.trim().is_empty() || explanation.trim().is_empty() {
            return Err(Refusal("Give the consequence and the explanation.".into()));
        }
        fix_of(&input.files, fix).map(|_| ())
    }
}

impl DrillBuild<'_> {
    /// Stage 6. What a fresh reviewer found, how the grader judged it, how findable each planted issue was, and the
    /// issues the review added to the key. With no planted issue there is no key to judge against, and nothing is
    /// asked.
    pub(crate) async fn blind_review(
        &self,
        spec: &DrillSpec,
        baseline: &Baseline,
        proved: &Proved,
        checked: &LessonChecked,
    ) -> Result<BlindReview> {
        let planted = proved.planted_key(checked);
        let mut trail = Trail::default();
        if planted.is_empty() {
            return Ok(BlindReview {
                findings: Vec::new(),
                assessment: None,
                findability: Vec::new(),
                extras: Vec::new(),
                trail,
            });
        }
        let model = self.llm.model_name().to_owned();
        let wording = self.material.reviewer_wording(spec);
        let subject = match self.material.reviewer_context(baseline) {
            Some(context) => format!("{}\n\n{context}", wording.subject),
            None => wording.subject.clone(),
        };
        let files: Vec<CodeFile> = proved.files.iter().map(CodeFile::of).collect();
        let asking = Asking {
            progress: None,
            reasoning: None,
        };
        let found = ask_new::<BlindReviewAsk>(
            self.llm,
            &asking,
            &mut Scope::for_question(),
            ReviewInput {
                wording: Told(wording.reviewer.to_owned()),
                subject: Told(subject.clone()),
                files: files.clone(),
                output_tokens: Internal::of(budget::findings(spec.lines)),
            },
            |_| None,
            &model_call::answer_options(),
        )
        .await?;
        if let Some(why) = &found.rest {
            return Err(failed(why));
        }
        trail.call("blind review", &model);
        let reviewed: Vec<ReviewedFinding> = found
            .items
            .iter()
            .filter_map(|remark| reviewed_of(&files, remark))
            .collect();
        let findings: Vec<Finding> = reviewed
            .iter()
            .map(|finding| Finding {
                span: finding.span.clone(),
                text: finding.comment.clone(),
                severity: Some(finding.severity),
            })
            .collect();
        let drill = Drill {
            id: baseline.drill_id.clone(),
            material: spec.material.clone(),
            title: baseline.title.clone(),
            files: proved.files.clone(),
            key: planted.clone(),
        };
        let assessment = self
            .grader
            .assess(&Review {
                drill: &drill,
                clean: &baseline.files,
                source: baseline.source.as_ref(),
                findings: &findings,
            })
            .await?;
        validate(planted.len(), findings.len(), &assessment).map_err(|fault| {
            AppError::InvalidData(format!(
                "The grader's assessment is not consistent with the review: {fault:?}"
            ))
        })?;
        let findability: Vec<Findability> = planted
            .iter()
            .zip(&assessment.issues)
            .map(|(issue, (outcome, reason))| Findability {
                issue: issue.id.clone(),
                pattern: issue.pattern.clone(),
                outcome: *outcome,
                reason: reason.clone(),
            })
            .collect();
        let patterns: Vec<Candidate> = patterns_for(&spec.material).map(Candidate::of).collect();
        let mut extras: Vec<KeyIssue> = Vec::new();
        for (position, (outcome, reason)) in assessment.findings.iter().enumerate() {
            if !matches!(outcome, FindingOutcome::ValidExtra) {
                continue;
            }
            let Some(finding) = reviewed.get(position) else {
                continue;
            };
            let settled = ask::<KeyEntryAsk>(
                self.llm,
                &asking,
                &mut Scope::for_question(),
                &EntryInput {
                    wording: Told(wording.describer.to_owned()),
                    subject: Told(subject.clone()),
                    heading: Told(wording.files.to_owned()),
                    files: files.clone(),
                    path: Told(finding.path.clone()),
                    quote: Told(finding.quote.clone()),
                    comment: Told(finding.comment.clone()),
                    reason: Told(reason.clone()),
                    patterns: patterns.clone(),
                    output_tokens: Internal::of(budget::key_entry()),
                },
                &model_call::answer_options(),
            )
            .await?;
            let described = match settled {
                Settled::Answered(entry) => {
                    trail.call("key entry", &model);
                    entry.0
                }
                Settled::Unanswered(_) => {
                    // An entry that cannot be described exactly is not added; a learner who finds the problem is
                    // still credited as a valid extra by the grader.
                    trail.call("key entry (not used)", &model);
                    continue;
                }
            };
            let Described::Issue {
                pattern,
                severity,
                consequence,
                fix,
                explanation,
            } = described
            else {
                continue;
            };
            let Ok(fix) = fix_of(&files, &fix) else {
                continue;
            };
            let Some(span) = lines_of(&proved.files, &fix.path, &fix.before) else {
                continue;
            };
            let pattern = pattern.reference;
            let known = planted.iter().chain(&extras).any(|issue| {
                issue.fix == fix || (issue.pattern == pattern && overlaps(&issue.span, &span))
            });
            if known {
                continue;
            }
            extras.push(KeyIssue {
                id: format!("review-{}", extras.len() + 1),
                pattern,
                severity: severity.0,
                span,
                consequence,
                fix,
                explanation,
                proof: Proof::Judged {
                    reason: reason.clone(),
                },
                origin: Origin::FoundInReview,
                lesson: None,
            });
        }
        Ok(BlindReview {
            findings: reviewed,
            assessment: Some(assessment),
            findability,
            extras,
            trail,
        })
    }
}

/// The finding a remark is: its lines as the app shows them, with their span.
fn reviewed_of(files: &[CodeFile], remark: &Remark) -> Option<ReviewedFinding> {
    let block = remark.block(files)?;
    let file = files.get(block.file)?;
    let quote: Vec<&str> = file
        .text
        .0
        .lines()
        .skip(block.first)
        .take(block.last.saturating_sub(block.first).saturating_add(1))
        .collect();
    Some(ReviewedFinding {
        path: file.path.0.clone(),
        quote: normalise(&quote.join("\n")),
        comment: remark.comment.clone(),
        severity: remark.severity.0,
        span: Some(LineSpan {
            path: file.path.0.clone(),
            start: u32::try_from(block.first.saturating_add(1)).ok()?,
            end: u32::try_from(block.last.saturating_add(1)).ok()?,
        }),
    })
}

/// The lines `text` occupies in the file at `path`, when it is there once.
fn lines_of(files: &[DrillFile], path: &str, text: &str) -> Option<LineSpan> {
    let identity = Edit {
        path: path.to_owned(),
        before: text.to_owned(),
        after: text.to_owned(),
    };
    apply(files, std::slice::from_ref(&identity))
        .ok()
        .and_then(|(_, spans)| spans.into_iter().next())
}

#[cfg(test)]
#[path = "blind_review/tests.rs"]
mod tests;
