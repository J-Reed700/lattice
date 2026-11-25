//! The three asks that make up the check of a lesson against its plan: which
//! planned items the lesson teaches, what it uses before teaching, says
//! against itself or points at that the learner cannot see, and which of a
//! review's reports are of a kind the check decides. The model points at the
//! sentences it was shown; the app reads the words from its own copy.
use super::sentence::{self, Lesson};
use super::{
    Item, ANSWER_TOKENS, CHECK_TASK, FIND_DELETION_SYSTEM, FIND_EDITED_SYSTEM, FIND_SYSTEM,
    FIND_TASK, PLANNED_SYSTEM, REPORTED, REPORTS_SYSTEM, REPORTS_TASK,
};
use crate::review_findings::{FindingSite, ReviewFinding};
use lattice_core::application::services::model_contract::ask::{Alone, Answered, Refusal, Says};
use lattice_core::application::services::model_contract::shape::{
    Authored, Each, Fields, Form, New, Union, Vocabulary,
};
use lattice_core::application::services::model_contract::shown::{
    Drawn, Internal, Kind, Picked, Showing, Told,
};
use lattice_core::application::services::model_contract::units::Unit;
use lattice_core::application::services::model_contract::{Contract, ContractId};
use lattice_core::shared::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// One thing the course plan gives the lesson, as the model is asked about it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Planned {
    pub(super) id: Internal<String>,
    pub(super) kind: Told<String>,
    pub(super) text: Told<String>,
}

impl Planned {
    pub(super) fn of(item: &Item) -> Self {
        Self {
            id: Internal::of(item.id.clone()),
            kind: Told(item.kind.to_owned()),
            text: Told(item.text.clone()),
        }
    }
}

impl Kind for Planned {
    const PREFIX: &'static str = "i";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = String;

    fn reference(&self) -> String {
        self.id.value().clone()
    }
}

/// A defect a review described, as the check is shown it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Reported {
    pub(super) at: usize,
    pub(super) part: Option<Told<String>>,
    pub(super) defect: Told<String>,
    pub(super) words: Option<Told<String>>,
}

impl Reported {
    pub(super) fn of(at: usize, finding: &ReviewFinding) -> Self {
        Self {
            at,
            part: finding
                .site
                .filter(|site| *site != FindingSite::Lesson)
                .map(|site| Told(site.label())),
            defect: Told(finding.note.clone()),
            words: finding.passage.clone().map(Told),
        }
    }

    fn said(&self, id: Option<String>) -> Value {
        let mut entry = Map::new();
        if let Some(id) = id {
            entry.insert("id".into(), json!(id));
        }
        if let Some(part) = &self.part {
            entry.insert("part".into(), json!(part));
        }
        entry.insert("defect".into(), json!(self.defect));
        if let Some(words) = &self.words {
            entry.insert("words".into(), json!(words));
        }
        Value::Object(entry)
    }
}

impl Kind for Reported {
    const PREFIX: &'static str = "r";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = usize;

    fn reference(&self) -> usize {
        self.at
    }
}

/// What the course says around the lesson.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Course {
    pub(super) prior: Told<Value>,
    pub(super) title: Told<Value>,
    pub(super) earlier: Told<Value>,
    pub(super) later: Told<Value>,
    pub(super) module: Told<Value>,
    pub(super) outline: Told<Value>,
}

/// What a request of the planned check or of the findings check says.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Input {
    pub(super) course: Course,
    pub(super) lesson: Lesson,
    pub(super) planned: Vec<Planned>,
    pub(super) reported: Vec<Reported>,
}

/// What a request that rules on a review's reports says.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ReportsInput {
    pub(super) lesson: Lesson,
    pub(super) reported: Vec<Reported>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Mark {
    Yes,
    No,
}

impl Vocabulary for Mark {
    const WORDS: &'static [(&'static str, Self)] = &[("yes", Self::Yes), ("no", Self::No)];
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Verdict {
    Taught,
    NotTaught,
}

impl Vocabulary for Verdict {
    const WORDS: &'static [(&'static str, Self)] =
        &[("taught", Self::Taught), ("not_taught", Self::NotTaught)];
}

/// What the model decided about one planned item.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Taught {
    pub(super) verdict: Verdict,
    pub(super) units: Vec<Picked<Unit>>,
}

impl Authored for Taught {
    fn form() -> Form {
        Form::new().word::<Verdict>("taught").cite::<Unit>("units")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            verdict: fields.word("taught")?,
            units: fields.cite("units"),
        })
    }
}

/// One thing the lesson does wrong, as the model found it.
#[derive(Clone, Debug)]
pub(crate) enum Finding {
    Used {
        what: String,
        uses: Vec<Picked<Unit>>,
        later: Vec<Picked<Unit>>,
    },
    Clash {
        first: Picked<Unit>,
        second: Picked<Unit>,
    },
    Unseen {
        words: Picked<Unit>,
        what: String,
        never_shown: Mark,
    },
}

impl Union for Finding {
    fn forms() -> Vec<(&'static str, Form)> {
        vec![
            (
                "usedBeforeTaught",
                Form::new()
                    .text("what")
                    .cite::<Unit>("uses")
                    .cite::<Unit>("taughtLater"),
            ),
            (
                "contradiction",
                Form::new().point::<Unit>("first").point::<Unit>("second"),
            ),
            (
                "unseen",
                Form::new()
                    .point::<Unit>("words")
                    .text("what")
                    .word::<Mark>("neverShown"),
            ),
        ]
    }

    fn read(kind: &str, fields: &mut Fields<'_, '_>) -> Option<Self> {
        match kind {
            "usedBeforeTaught" => Some(Self::Used {
                what: fields.text("what")?,
                uses: fields.cite("uses"),
                later: fields.cite("taughtLater"),
            }),
            "contradiction" => Some(Self::Clash {
                first: fields.point("first")?,
                second: fields.point("second")?,
            }),
            "unseen" => Some(Self::Unseen {
                words: fields.point("words")?,
                what: fields.text("what")?,
                never_shown: fields.word("neverShown")?,
            }),
            _ => None,
        }
    }
}

impl Authored for Finding {
    fn form() -> Form {
        Form::new().one_of::<Finding>("finding")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        fields.one_of::<Finding>("finding")
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Decides {
    Decided,
    Other,
}

impl Vocabulary for Decides {
    const WORDS: &'static [(&'static str, Self)] =
        &[("decided", Self::Decided), ("other", Self::Other)];
}

/// Whether the defect a report describes is of a kind the check decides.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct About {
    pub(super) about: Decides,
}

impl Authored for About {
    fn form() -> Form {
        Form::new().word::<Decides>("about")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            about: fields.word("about")?,
        })
    }
}

/// The ask whether the lesson teaches each planned item.
pub(crate) struct LessonContractCheck;

impl Contract for LessonContractCheck {
    const ID: ContractId = ContractId::new("lesson-contract-check");
    type Input = Input;
}

/// The ask what the lesson uses before it teaches it, says against itself, or points at without showing.
pub(crate) struct LessonContractFind;

impl Contract for LessonContractFind {
    const ID: ContractId = ContractId::new("lesson-contract-find");
    type Input = Input;
}

/// How an incremental `LessonContractFindDelta` call is scoped.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) enum DeltaMode {
    DeletionOnly {
        from_part_order: usize,
        focus_part_labels: Vec<Told<String>>,
    },
    Edited {
        from_part_order: usize,
        focus_part_labels: Vec<Told<String>>,
        changed_part_labels: Vec<Told<String>>,
        delta_by_part: Vec<Vec<usize>>,
    },
}

/// What a scoped delta request of `LessonContractFindDelta` says.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct DeltaInput {
    pub(super) input: Input,
    pub(super) mode: DeltaMode,
}

/// The incremental ask for a lesson that already passed `LessonContractFind` cleanly.
pub(crate) struct LessonContractFindDelta;

impl Contract for LessonContractFindDelta {
    const ID: ContractId = ContractId::new("lesson-contract-find-delta");
    type Input = DeltaInput;
}

/// The ask which of a review's reports are of a kind the check decides.
pub(crate) struct LessonContractReports;

impl Contract for LessonContractReports {
    const ID: ContractId = ContractId::new("lesson-contract-reports");
    type Input = ReportsInput;
}

/// The task of a request about the lesson in its course: what the course says around it, then the lesson.
fn task_of(
    task: &str,
    input: &Input,
    planned: Option<Vec<Value>>,
    lesson: (Vec<Value>, Vec<Value>),
) -> Map<String, Value> {
    let course = &input.course;
    let mut this = Map::new();
    this.insert("title".into(), json!(course.title));
    if let Some(planned) = planned {
        this.insert("planned".into(), Value::Array(planned));
    }
    let mut fields = Map::new();
    fields.insert("task".into(), json!(task));
    fields.insert("priorKnowledge".into(), json!(course.prior));
    fields.insert("thisLesson".into(), Value::Object(this));
    fields.insert("earlierLessons".into(), json!(course.earlier));
    fields.insert("laterLessons".into(), json!(course.later));
    fields.insert("thisModule".into(), json!(course.module));
    fields.insert("courseOutline".into(), json!(course.outline));
    fields.insert("sections".into(), Value::Array(lesson.0));
    fields.insert("questions".into(), Value::Array(lesson.1));
    fields
}

impl Answered for LessonContractCheck {
    type Answer = Each<Planned, Taught>;
    const ALONE: Alone = Alone::Pending;

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        let planned = showing.show(&input.planned)?;
        let texts = input.lesson.texts();
        let shown = sentence::show(&texts, showing)?;
        let lesson = input.lesson.said(&sentence::labelled(&texts, &shown), &[]);
        let listed: Vec<Value> = planned
            .iter()
            .map(|(handle, item)| json!({"id": handle.written(), "kind": item.kind, "text": item.text}))
            .collect();
        let fields = task_of(CHECK_TASK, input, Some(listed), lesson);
        let mut says = Says::new(PLANNED_SYSTEM, Value::Object(fields).to_string());
        says.output_tokens = ANSWER_TOKENS;
        Ok(says)
    }

    fn accept(_input: &Input, taught: &Taught) -> std::result::Result<(), Refusal> {
        if taught.verdict == Verdict::Taught && taught.units.is_empty() {
            return Err(Refusal(
                "Cite in units at least one sentence that teaches it. Naming words in your answer does not cite them.".into(),
            ));
        }
        Ok(())
    }
}

fn accept_find(finding: &Finding) -> std::result::Result<(), Refusal> {
    match finding {
        Finding::Used { what, uses, .. } => {
            if what.trim().is_empty() {
                return Err(Refusal("Name in what the thing that is used.".into()));
            }
            if uses.is_empty() {
                return Err(Refusal(
                    "Cite in uses at least one sentence that asks the learner to use it. Naming words in your answer does not cite them.".into(),
                ));
            }
        }
        Finding::Unseen { what, .. } if what.trim().is_empty() => {
            return Err(Refusal("Say in what what the sentence points at.".into()));
        }
        Finding::Clash { .. } | Finding::Unseen { .. } => {}
    }
    Ok(())
}

impl Answered for LessonContractFind {
    type Answer = New<Finding>;
    const ALONE: Alone = Alone::Fails;

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        let texts = input.lesson.texts();
        let shown = sentence::show(&texts, showing)?;
        let lesson = input.lesson.said(&sentence::labelled(&texts, &shown), &[]);
        let mut fields = task_of(FIND_TASK, input, None, lesson);
        if !input.reported.is_empty() {
            let reported = input.reported.iter().map(|report| report.said(None));
            fields.insert("reported".into(), reported.collect());
        }
        let instructions = if input.reported.is_empty() {
            FIND_SYSTEM.to_owned()
        } else {
            format!("{FIND_SYSTEM}{REPORTED}")
        };
        let mut says = Says::new(instructions, Value::Object(fields).to_string());
        says.output_tokens = ANSWER_TOKENS;
        Ok(says)
    }

    fn accept(_input: &Input, finding: &Finding) -> std::result::Result<(), Refusal> {
        accept_find(finding)
    }
}

impl Answered for LessonContractFindDelta {
    type Answer = New<Finding>;
    const ALONE: Alone = Alone::Fails;

    fn says(input: &DeltaInput, showing: &mut Showing<'_>) -> Result<Says> {
        let texts = input.input.lesson.texts();
        let shown = sentence::show(&texts, showing)?;
        let lesson = input
            .input
            .lesson
            .said(&sentence::labelled(&texts, &shown), &[]);
        let (from_part_order, delta_by_part) = match &input.mode {
            DeltaMode::DeletionOnly {
                from_part_order, ..
            } => (*from_part_order, None),
            DeltaMode::Edited {
                from_part_order,
                delta_by_part,
                ..
            } => (*from_part_order, Some(delta_by_part.as_slice())),
        };
        let mut per_part_cursor: BTreeMap<usize, usize> = BTreeMap::new();
        let mut focus_sentences = Vec::new();
        let mut changed_sentences = Vec::new();
        for ((whose, _), shown_units) in texts.iter().zip(&shown) {
            let sentence::Whose::Lesson { part, .. } = whose else {
                continue;
            };
            let cursor = per_part_cursor.entry(*part).or_insert(0);
            let part_deltas = delta_by_part.and_then(|all| all.get(*part));
            for (handle, _) in shown_units.iter() {
                if *part >= from_part_order {
                    focus_sentences.push(handle.written());
                }
                if part_deltas.is_some_and(|deltas| deltas.contains(cursor)) {
                    changed_sentences.push(handle.written());
                }
                *cursor += 1;
            }
        }
        let mut fields = task_of(FIND_TASK, &input.input, None, lesson);
        let instructions = match &input.mode {
            DeltaMode::DeletionOnly {
                focus_part_labels, ..
            } => {
                fields.insert("focusParts".into(), json!(focus_part_labels));
                fields.insert("focusSentences".into(), json!(focus_sentences));
                FIND_DELETION_SYSTEM.to_owned()
            }
            DeltaMode::Edited {
                focus_part_labels,
                changed_part_labels,
                ..
            } => {
                fields.insert("focusParts".into(), json!(focus_part_labels));
                fields.insert("focusSentences".into(), json!(focus_sentences));
                fields.insert("changedParts".into(), json!(changed_part_labels));
                fields.insert("changedSentences".into(), json!(changed_sentences));
                FIND_EDITED_SYSTEM.to_owned()
            }
        };
        let mut says = Says::new(instructions, Value::Object(fields).to_string());
        says.output_tokens = ANSWER_TOKENS;
        Ok(says)
    }

    fn accept(_input: &DeltaInput, finding: &Finding) -> std::result::Result<(), Refusal> {
        accept_find(finding)
    }
}

impl Answered for LessonContractReports {
    type Answer = Each<Reported, About>;
    const ALONE: Alone = Alone::Pending;

    fn says(input: &ReportsInput, showing: &mut Showing<'_>) -> Result<Says> {
        let reports = showing.show(&input.reported)?;
        let lesson = input
            .lesson
            .said(&sentence::plain(&input.lesson.texts()), &[]);
        let listed: Vec<Value> = reports
            .iter()
            .map(|(handle, report)| report.said(Some(handle.written())))
            .collect();
        let mut fields = Map::new();
        fields.insert("task".into(), json!(REPORTS_TASK));
        fields.insert("sections".into(), Value::Array(lesson.0));
        fields.insert("questions".into(), Value::Array(lesson.1));
        fields.insert("reported".into(), Value::Array(listed));
        let mut says = Says::new(REPORTS_SYSTEM, Value::Object(fields).to_string());
        says.output_tokens = ANSWER_TOKENS;
        Ok(says)
    }
}
