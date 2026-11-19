//! The two questions the judge answers: what became of every issue of the key, and what became of every finding that
//! no issue claims. The relation between an issue and a finding is written once, from the issue's side.

use lattice_core::application::services::model_contract::ask::{Alone, Answered, Refusal, Says};
use lattice_core::application::services::model_contract::shape::{
    Authored, Each, Fields, Form, Vocabulary,
};
use lattice_core::application::services::model_contract::shown::{
    Drawn, Internal, Kind, Picked, Showing, Told,
};
use lattice_core::application::services::model_contract::{Contract, ContractId};
use lattice_core::shared::error::Result;
use serde::{Deserialize, Serialize};

/// The question of what became of each issue of the key.
pub(super) struct GradeReview;

impl Contract for GradeReview {
    const ID: ContractId = ContractId::new("grade-review");
    type Input = Input;
}

/// The question of what became of each finding that no issue claims.
pub(super) struct GradeExtras;

impl Contract for GradeExtras {
    const ID: ContractId = ContractId::new("grade-extras");
    type Input = ExtrasInput;
}

/// An issue of the key, as the model names it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct IssueView {
    pub(super) at: usize,
    pub(super) head: Told<String>,
    pub(super) rest: Told<String>,
}

impl Kind for IssueView {
    const PREFIX: &'static str = "i";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = usize;

    fn reference(&self) -> usize {
        self.at
    }
}

/// A finding of the review, as the model names it, with the issues whose lines its lines overlap.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct FindingView {
    pub(super) at: usize,
    pub(super) head: Told<String>,
    pub(super) rest: Told<String>,
    pub(super) overlaps: Internal<Vec<usize>>,
}

impl Kind for FindingView {
    const PREFIX: &'static str = "f";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = usize;

    fn reference(&self) -> usize {
        self.at
    }
}

/// What every request of a review shows besides the things it asks about.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Context {
    pub(super) instructions: Told<String>,
    pub(super) drill: Told<String>,
    pub(super) reviewed: Told<String>,
    pub(super) original: Told<String>,
    pub(super) source: Option<Told<String>>,
}

impl Context {
    fn opening(&self) -> Vec<String> {
        let mut parts = vec![
            self.drill.0.clone(),
            self.reviewed.0.clone(),
            self.original.0.clone(),
        ];
        parts.extend(self.source.iter().map(|source| source.0.clone()));
        parts
    }
}

/// The issues still open, and the findings of the review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Input {
    pub(super) context: Context,
    /// What one numbered line of the material is called, and more than one.
    pub(super) noun: Told<String>,
    pub(super) issues: Vec<IssueView>,
    pub(super) findings: Vec<FindingView>,
}

/// The key, shown for reference, and the findings no issue claims.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ExtrasInput {
    pub(super) context: Context,
    pub(super) key: Vec<IssueView>,
    pub(super) findings: Vec<FindingView>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Credit {
    Found,
    HalfFound,
    Missed,
}

impl Vocabulary for Credit {
    const WORDS: &'static [(&'static str, Self)] = &[
        ("found", Self::Found),
        ("half_found", Self::HalfFound),
        ("missed", Self::Missed),
    ];
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Extra {
    ValidExtra,
    FalsePositive,
}

impl Vocabulary for Extra {
    const WORDS: &'static [(&'static str, Self)] = &[
        ("valid_extra", Self::ValidExtra),
        ("false_positive", Self::FalsePositive),
    ];
}

/// What the model decided for one issue.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Judgement {
    pub(super) reason: String,
    pub(super) outcome: Credit,
    pub(super) finding: Option<Picked<FindingView>>,
    pub(super) about: Vec<Picked<FindingView>>,
}

impl Authored for Judgement {
    fn form() -> Form {
        Form::new()
            .text("reason")
            .word::<Credit>("outcome")
            .point_or_none::<FindingView>("finding")
            .cite::<FindingView>("findings")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            reason: fields.text("reason")?,
            outcome: fields.word("outcome")?,
            finding: fields.point_or_none("finding")?,
            about: fields.cite("findings"),
        })
    }
}

/// What the model decided for one finding no issue claims.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Verdict {
    pub(super) reason: String,
    pub(super) outcome: Extra,
}

impl Authored for Verdict {
    fn form() -> Form {
        Form::new().text("reason").word::<Extra>("outcome")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            reason: fields.text("reason")?,
            outcome: fields.word("outcome")?,
        })
    }
}

/// "3 issues", "1 issue", "no issues".
fn counted(count: usize, noun: &str) -> String {
    match count {
        0 => format!("no {noun}s"),
        1 => format!("1 {noun}"),
        count => format!("{count} {noun}s"),
    }
}

fn block(label: &str, name: &str, head: &Told<String>, rest: &Told<String>) -> String {
    format!("{label}{name}: {}\n{}", head.0, rest.0)
}

/// The pairs of a finding and an issue, both shown, whose lines overlap: the hint the judge is given.
fn overlap_block(
    noun: &str,
    issues: &[(String, &IssueView)],
    findings: &[(String, &FindingView)],
) -> String {
    let pairs: Vec<String> = findings
        .iter()
        .flat_map(|(finding, view)| {
            issues
                .iter()
                .filter(|(_, issue)| view.overlaps.value().contains(&issue.at))
                .map(move |(issue, _)| format!("finding {finding} and issue {issue}"))
        })
        .collect();
    if pairs.is_empty() {
        return format!("No finding's {noun} overlap an issue's {noun}.");
    }
    format!(
        "Findings and issues whose {noun} overlap, a hint and not a verdict:\n{}",
        pairs.join("\n")
    )
}

impl Answered for GradeReview {
    type Answer = Each<IssueView, Judgement>;
    const ALONE: Alone = Alone::Fails;

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        let issues = showing.show(&input.issues)?;
        let findings = showing.show(&input.findings)?;
        let mut parts = input.context.opening();
        parts.push(format!(
            "The key: {}.",
            counted(input.issues.len(), "issue")
        ));
        parts.extend(
            issues.iter().map(|(handle, issue)| {
                block("Issue ", &handle.bracketed(), &issue.head, &issue.rest)
            }),
        );
        parts.push(format!(
            "The review: {}, in the order they were made.",
            counted(input.findings.len(), "finding")
        ));
        parts.extend(findings.iter().map(|(handle, finding)| {
            block(
                "Finding ",
                &handle.bracketed(),
                &finding.head,
                &finding.rest,
            )
        }));
        let named_issues: Vec<(String, &IssueView)> = issues
            .iter()
            .map(|(handle, issue)| (handle.bracketed(), issue))
            .collect();
        let named_findings: Vec<(String, &FindingView)> = findings
            .iter()
            .map(|(handle, finding)| (handle.bracketed(), finding))
            .collect();
        parts.push(overlap_block(&input.noun.0, &named_issues, &named_findings));
        parts.push("Give an entry for every issue, the reason first.".to_owned());
        Ok(Says::new(
            input.context.instructions.0.clone(),
            parts.join("\n\n"),
        ))
    }

    fn accept(_input: &Input, judgement: &Judgement) -> std::result::Result<(), Refusal> {
        if judgement.reason.trim().is_empty() {
            return Err(Refusal(
                "This issue has no reason: say why before giving its outcome.".into(),
            ));
        }
        match judgement.outcome {
            Credit::Missed if !judgement.about.is_empty() => Err(Refusal(
                "This issue is missed but findings are listed as about it: credit it to the finding that earns it, or leave findings empty.".into(),
            )),
            Credit::Found if judgement.finding.is_none() => Err(Refusal(
                "This issue is found but names no finding: give the finding that names its cause.".into(),
            )),
            Credit::HalfFound if judgement.finding.is_none() => Err(Refusal(
                "This issue is half found but names no finding: give the finding that points at it.".into(),
            )),
            _ => Ok(()),
        }
    }
}

impl Answered for GradeExtras {
    type Answer = Each<FindingView, Verdict>;
    const ALONE: Alone = Alone::Fails;

    fn says(input: &ExtrasInput, showing: &mut Showing<'_>) -> Result<Says> {
        let findings = showing.show(&input.findings)?;
        let mut parts = input.context.opening();
        parts.push(format!("The key: {}.", counted(input.key.len(), "issue")));
        parts.extend(
            input
                .key
                .iter()
                .map(|issue| block("Issue", "", &issue.head, &issue.rest)),
        );
        parts.push(format!(
            "The findings about no issue of the key: {}, in the order they were made.",
            counted(input.findings.len(), "finding")
        ));
        parts.extend(findings.iter().map(|(handle, finding)| {
            block(
                "Finding ",
                &handle.bracketed(),
                &finding.head,
                &finding.rest,
            )
        }));
        parts.push("Give an entry for every finding, the reason first.".to_owned());
        Ok(Says::new(
            input.context.instructions.0.clone(),
            parts.join("\n\n"),
        ))
    }

    fn accept(_input: &ExtrasInput, verdict: &Verdict) -> std::result::Result<(), Refusal> {
        if verdict.reason.trim().is_empty() {
            return Err(Refusal(
                "This finding has no reason: say why before giving its outcome.".into(),
            ));
        }
        Ok(())
    }
}
