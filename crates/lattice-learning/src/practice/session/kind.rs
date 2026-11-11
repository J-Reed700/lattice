//! A kind of work, as a record of small roles. The engine calls these and nothing else.

use crate::dto::LearningPracticeProfileDto;
use crate::practice::session::author::Author;
use crate::practice::session::observation::Observation;
use crate::practice::session::record::{Session, SessionCheck, SessionDebrief, SessionLearnerCase};
use crate::practice::session::seen::Seen;
use async_trait::async_trait;
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::practice::work::{
    Brief, Case, CaseResult, Change, CheckReport, CheckScope, Draft, Exercise, Form, KindId,
    PartRef, Piece, Rung, Spacing, Stage,
};
use lattice_core::shared::error::{AppError, Result};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Decides which cases a piece passes.
#[async_trait]
pub trait Checker: Send + Sync {
    async fn check(
        &self,
        brief: &Brief,
        piece: &Piece,
        cases: &[Case],
        cancel: CancellationToken,
    ) -> Result<CheckReport>;
}

/// The addressable units of a piece.
pub trait Parts: Send + Sync {
    fn all(&self, brief: &Brief, piece: &Piece) -> Result<Vec<PartRef>>;
    fn holds(&self, brief: &Brief, piece: &Piece, part: &PartRef) -> Result<bool>;
    /// The part's display name.
    fn name(&self, brief: &Brief, piece: &Piece, part: &PartRef) -> Result<String>;
}

/// The changes between two drafts, in the kind's own units.
pub trait Diff: Send + Sync {
    fn changes(&self, brief: &Brief, old: &Piece, new: &Piece) -> Result<Vec<Change>>;
}

/// Text for a model.
pub trait Showing: Send + Sync {
    /// The task as a model reads it, with the cases the learner is shown: whole lines, each ending in a newline.
    /// Never the reference.
    fn brief(&self, brief: &Brief, shown: &[Case]) -> Result<String>;
    /// A piece as a model reads it: whole lines, each ending in a newline.
    fn piece(&self, brief: &Brief, piece: &Piece) -> Result<String>;
    /// One case and, once it is checked, its result, on one line. A case that is not `open` shows only what a
    /// learner who may not look inside it sees.
    fn case(
        &self,
        brief: &Brief,
        case: &Case,
        result: Option<&CaseResult>,
        open: bool,
    ) -> Result<String>;
    /// What a check says beyond its cases, one line each.
    fn check_notes(&self, brief: &Brief, detail: &serde_json::Value) -> Result<Vec<String>>;
}

/// The words for this work. A kind overrides only what it words differently from the value's own name.
pub trait Wording: Send + Sync {
    fn card(&self) -> String;

    fn form_label(&self, form: Form) -> String {
        form.label().to_owned()
    }

    fn stage_label(&self, _brief: &Brief, _form: Form, stage: Stage) -> String {
        stage.label().to_owned()
    }

    fn check_noun(&self) -> String {
        "check".to_owned()
    }

    fn scope_label(&self, scope: CheckScope) -> String {
        scope.label().to_owned()
    }

    fn rung_label(&self, rung: Rung) -> String {
        rung.label().to_owned()
    }

    fn practice_scene(&self) -> String {
        "a practice session".to_owned()
    }

    fn interview_scene(&self) -> String {
        "a mock interview".to_owned()
    }

    fn task_noun(&self) -> String {
        "task".to_owned()
    }

    /// What a session is called in the debrief's opening sentence, without an article.
    fn session_noun(&self) -> String {
        "practice session".to_owned()
    }

    /// The checks, in the plural, as the debrief lists what it shows beside the scores.
    fn check_nouns(&self) -> String {
        "checks".to_owned()
    }

    fn piece_noun(&self) -> String {
        "work".to_owned()
    }

    fn evidence(&self) -> String {
        "the learner's work and checks".to_owned()
    }

    fn refer_rule(&self) -> String {
        "Refer to a part by its name.".to_owned()
    }

    fn unsolved_rule(&self) -> String {
        "anything of the work you write must leave the task unsolved".to_owned()
    }

    /// What the kind calls the part of its pieces a reply may quote, when it has one.
    fn quoted_work(&self) -> Option<String> {
        None
    }

    fn checked_verb(&self) -> String {
        "checked".to_owned()
    }

    fn submit_probes(&self) -> String {
        "why they chose something".to_owned()
    }

    fn checkpoint_probes(&self) -> String {
        "why they chose something".to_owned()
    }

    /// The instruction a stage opens with.
    fn stage_instruction(&self, brief: &Brief, form: Form, stage: Stage) -> String;
}

/// What a kind adds to a counterpart's reply: the fields that point at the work, and their rules. A refusal is an
/// `AppError::InvalidInput` worded for the model; any other error is a fault.
#[async_trait]
pub trait ReplyGuard: Send + Sync {
    /// The rules of the kind's reply fields, as sentences of the system prompt. Empty when it has none.
    fn rules(&self, brief: &Brief) -> String;
    /// What each of the kind's reply fields holds, as clauses of the format line. Empty when it has none.
    fn format(&self) -> String;
    /// The kind's reply fields in order: name and JSON schema. Every one is required. Empty when it has none.
    fn fields(&self, brief: &Brief) -> Result<Vec<(String, Value)>>;
    /// The parts of `piece` the reply's fields point at. Refuses a part the piece does not hold.
    fn parts(
        &self,
        brief: &Brief,
        piece: &Piece,
        fields: &Map<String, Value>,
    ) -> Result<Vec<PartRef>>;
    /// Refuses words that break one of the kind's rules.
    fn check_words(
        &self,
        exercise: &Exercise,
        fields: &Map<String, Value>,
        content: &str,
    ) -> Result<()>;
    /// Refuses a reply that solves the exercise. May run the kind's checker.
    async fn check_unsolved(
        &self,
        exercise: &Exercise,
        content: &str,
        cancel: CancellationToken,
    ) -> Result<()>;
}

/// What the learner's screen draws of the work, as values the kind's own screen code reads.
pub trait Screen: Send + Sync {
    /// The task, the draft, what each check found and the learner's own cases, without what `seen` withholds.
    fn work(&self, session: &Session, seen: &Seen) -> Result<Value>;
    /// The kind's part of a finished session's debrief. Nothing is withheld.
    fn debrief(&self, session: &Session, debrief: &SessionDebrief) -> Result<Value>;
}

/// What a hint is asked from.
pub struct HintAsk<'a> {
    pub llm: &'a dyn LLMPort,
    pub exercise: &'a Exercise,
    pub observation: &'a Observation,
    /// The observation as the model reads it.
    pub observed: &'a str,
    pub rung: Rung,
    pub in_role: bool,
    pub cancel: CancellationToken,
}

/// A hint that passed the kind's checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hint {
    /// The rung the hint is, which is not the asked one when the kind could not make that.
    pub rung: Rung,
    pub text: String,
    pub reason: String,
    pub parts: Vec<PartRef>,
}

/// Writes the hints of a kind's ladder.
#[async_trait]
pub trait Hinting: Send + Sync {
    /// The hint of `ask.rung`, or None when the kind has none of that rung.
    async fn hint(&self, ask: HintAsk<'_>) -> Result<Option<Hint>>;
}

/// A kind whose learner writes cases of their own.
pub trait LearnerCases: Send + Sync {
    /// A case the learner wrote, on one line.
    fn show(&self, brief: &Brief, body: &serde_json::Value) -> Result<String>;
    /// The key the debrief's list of the learner's cases sits under.
    fn key(&self) -> String;
    /// One case the learner wrote, with the verdict it got, as the debrief's model reads it.
    fn account(&self, brief: &Brief, case: &SessionLearnerCase) -> Result<Value>;
    /// How the debrief's model cites one of them.
    fn cited(&self) -> Citation;
    /// One case the learner wrote, as a case a check runs.
    fn case(&self, exercise: &Exercise, case: &SessionLearnerCase) -> Result<Case>;
}

/// How the debrief's model cites one kind of evidence the kind names: the tag of its items and the sentence of the
/// system text that says what the item's fields hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Citation {
    pub tag: String,
    pub sentence: String,
}

/// One thing a session is scored on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Criterion {
    pub id: String,
    pub title: String,
    pub earns: String,
}

/// What the debrief states about a strong answer beside the scores.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    /// The clause that names them in the opening sentence.
    pub names: String,
    /// The key of each in the answer.
    pub keys: Vec<String>,
    /// The sentence that says what each holds.
    pub says: String,
}

/// What a session of a form is scored on.
pub trait Rubric: Send + Sync {
    /// The criteria in the order the scorecard lists them.
    fn criteria(&self, form: Form) -> Vec<Criterion>;
    /// What the debrief states beside the scores. None when the kind states nothing.
    fn statement(&self, form: Form) -> Option<Statement>;
}

/// The session's work as the debrief's model reads and cites it. A refusal is an `AppError::InvalidInput` worded for
/// the model; any other error is a fault.
pub trait Account: Send + Sync {
    /// The key the exercise sits under.
    fn exercise_key(&self) -> String;
    /// The exercise with its reference: the debrief is the one reader that sees it.
    fn exercise(&self, exercise: &Exercise) -> Result<Value>;
    /// The key the form of the session sits under.
    fn form_key(&self) -> String;
    fn form(&self, form: Form) -> Result<Value>;
    /// The key the list of checks sits under.
    fn checks_key(&self) -> String;
    /// One check, given the check made before it.
    fn check(
        &self,
        exercise: &Exercise,
        check: &SessionCheck,
        before: Option<&SessionCheck>,
    ) -> Result<Value>;
    /// A piece no check holds: the learner's final draft.
    fn piece(&self, exercise: &Exercise, piece: &Piece) -> Result<Value>;
    /// How the model cites parts of the piece of a check.
    fn cites_parts(&self) -> Citation;
    /// How the model cites a check.
    fn cites_check(&self) -> Citation;
    /// The fields an evidence item has beyond `kind`, `ref` and `text`: name and JSON schema. Every one is required.
    fn fields(&self) -> Vec<(String, Value)>;
    /// The sentence that says to leave the fields an item does not use empty.
    fn leave_empty(&self) -> String;
    /// The parts of the piece of `check` that an item's fields name. Refuses a part the piece does not hold.
    fn parts(
        &self,
        exercise: &Exercise,
        check: &SessionCheck,
        fields: &Map<String, Value>,
    ) -> Result<Vec<PartRef>>;
}

/// The time a timed session gets.
pub trait Budget: Send + Sync {
    /// The seconds a session gets when the learner chose none. None leaves it untimed unless the learner sets a time.
    fn seconds(&self, brief: &Brief) -> Option<u32>;
}

/// Enables Complete: a piece with parts emptied, and with one part put back.
pub trait Gaps: Send + Sync {
    fn emptied(&self, brief: &Brief, piece: &Piece, parts: &[PartRef]) -> Result<Piece>;
    fn restored(&self, brief: &Brief, piece: &Piece, from: &Piece, part: &PartRef)
        -> Result<Piece>;
}

/// Enables Arrange: a piece with its parts in a seeded order.
pub trait Order: Send + Sync {
    fn shuffled(&self, brief: &Brief, piece: &Piece, seed: u64) -> Result<Piece>;
}

/// What a kind found when it established a draft's reference.
pub enum Established {
    Right {
        cases: Vec<Case>,
        proof: serde_json::Value,
    },
    /// Why the reference is not right, written for the model that drafted it.
    Wrong(String),
}

/// Where the kind's truth comes from.
#[async_trait]
pub trait Truth: Send + Sync {
    /// Whether the draft's reference is right. `cases` may come back enriched (a digest a run produced).
    async fn establish(&self, draft: &Draft, cancel: CancellationToken) -> Result<Established>;

    /// Why the exercise has more than one right answer, if it has.
    async fn unambiguous(
        &self,
        exercise: &Exercise,
        cancel: CancellationToken,
    ) -> Result<Option<String>>;
}

/// How many misreadings a proven exercise of a kind must kill, and why that many.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MisreadingNeed {
    pub count: usize,
    /// What the number is for.
    pub reason: &'static str,
}

impl MisreadingNeed {
    /// A kind that needs none of its own.
    pub const NONE: Self = Self {
        count: 0,
        reason: "",
    };
}

/// When a course has this kind of work.
pub trait Fit: Send + Sync {
    /// The kind as the fit check is told it: what the work is and when a course asks for it.
    fn asked_when(&self) -> String;

    /// Whether the course's practice profile asks for this kind outright.
    fn wanted_by(&self, _profile: &LearningPracticeProfileDto) -> bool {
        false
    }
}

/// One kind of work.
#[derive(Clone)]
pub struct Kind {
    pub id: KindId,
    pub spacing: Spacing,
    pub misreadings_needed: MisreadingNeed,
    pub wording: Arc<dyn Wording>,
    pub reply_guard: Arc<dyn ReplyGuard>,
    pub hinting: Arc<dyn Hinting>,
    pub rubric: Arc<dyn Rubric>,
    pub account: Arc<dyn Account>,
    pub budget: Arc<dyn Budget>,
    pub author: Arc<dyn Author>,
    pub checker: Arc<dyn Checker>,
    pub parts: Arc<dyn Parts>,
    pub diff: Arc<dyn Diff>,
    pub showing: Arc<dyn Showing>,
    pub screen: Arc<dyn Screen>,
    pub truth: Arc<dyn Truth>,
    pub fit: Arc<dyn Fit>,
    pub gaps: Option<Arc<dyn Gaps>>,
    pub order: Option<Arc<dyn Order>>,
    pub learner_cases: Option<Arc<dyn LearnerCases>>,
}

impl Kind {
    /// Whether the kind has the roles `form` needs.
    pub fn supports(&self, form: Form) -> bool {
        match form {
            Form::Build | Form::Repair => true,
            Form::Complete => self.gaps.is_some(),
            Form::Arrange => self.order.is_some(),
        }
    }
}

/// Every kind the app offers, built once in the composition root.
#[derive(Clone)]
pub struct Kinds(BTreeMap<KindId, Kind>);

impl Kinds {
    /// A repeated id is an error.
    pub fn new(kinds: impl IntoIterator<Item = Kind>) -> Result<Self> {
        let mut by_id = BTreeMap::new();
        for kind in kinds {
            let id = kind.id.clone();
            if by_id.insert(id.clone(), kind).is_some() {
                return Err(AppError::InvalidState(format!(
                    "The kind {} is registered twice.",
                    id.as_str()
                )));
            }
        }
        Ok(Self(by_id))
    }

    pub fn get(&self, id: &KindId) -> Result<&Kind> {
        self.0.get(id).ok_or_else(|| {
            AppError::InvalidState(format!("No kind of work is registered as {}.", id.as_str()))
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = &Kind> {
        self.0.values()
    }
}
