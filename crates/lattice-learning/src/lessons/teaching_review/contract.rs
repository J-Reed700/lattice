use super::concerns::{CheckConcern, IssueConcern};
use super::{LessonConcern, Remit};
use lattice_core::application::services::model_contract::{
    ask::{Alone, Answered, Says},
    packed::Packed,
    shape::{Authored, Fields, Form, New, Union},
    shown::{Drawn, Kind, Picked, Showing, Told},
    Contract, ContractId,
};
use lattice_core::shared::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::marker::PhantomData;

/// The room the review's answer is given.
const REVIEW_ANSWER_TOKENS: usize = 6000;

pub(super) const ANSWER_FORM_HEAD: &str =
    "Answer with items. Each item holds one entry, and the entry's kind says what it is.";
#[cfg(test)]
pub(super) const CONCERN_FORM_HEAD: &str = "An issue's concern is the one concern it fails.";

/// How the entries of an answer are written, given whether any passages or open findings are shown.
pub(super) fn answer_form(has_passages: bool, has_open: bool) -> String {
    let mut form = String::from(ANSWER_FORM_HEAD);
    if has_open {
        form.push_str(" An entry of kind issue is a defect of the whole material, as the issues described above: its note says what is wrong, and continues is the id of the open finding it continues, or none.");
    } else {
        form.push_str(" An entry of kind issue is a defect of the whole material, as the issues described above: its note says what is wrong, and continues is none.");
    }
    if has_passages {
        if has_open {
            form.push_str(" An entry of kind sectionDefect is a defect in a section in review, as the sectionDefects described above: passageId is the passage where it shows, finding says what is wrong, and continues is as for an issue.");
        } else {
            form.push_str(" An entry of kind sectionDefect is a defect in a section in review, as the sectionDefects described above: passageId is the passage where it shows, finding says what is wrong, and continues is none.");
        }
    }
    if has_open {
        form.push_str(" An entry of kind resolved gives in openFinding the id of an open finding whose defect is no longer in the candidate; an open finding that no entry names stays open.");
    }
    form
}

/// What a lesson's review adds to how its entries are written.
fn concern_form(has_passages: bool) -> &'static str {
    if has_passages {
        " An issue's concern is the one concern it fails. A sectionDefect's concerns is a list of objects, each with a concern: one for every concern the section fails."
    } else {
        " An issue's concern is the one concern it fails."
    }
}

/// A part of a section the reviewer may point at: a passage of a section in review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Passage {
    pub(super) id: String,
    pub(super) section: usize,
    pub(super) number: usize,
}

impl Kind for Passage {
    const PREFIX: &'static str = "p";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = String;

    fn reference(&self) -> String {
        self.id.clone()
    }
}

/// A finding an earlier review recorded, still open.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OpenFinding {
    pub(super) id: crate::review_findings::FindingId,
    pub(super) part: Told<String>,
    /// The passage it is about now, when the reviewer may point at it.
    pub(super) passage: Option<String>,
    pub(super) note: Told<String>,
}

impl Kind for OpenFinding {
    const PREFIX: &'static str = "f";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = crate::review_findings::FindingId;

    fn reference(&self) -> crate::review_findings::FindingId {
        self.id.clone()
    }
}

/// What one review request says.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Input {
    pub(super) system: Told<String>,
    /// The caller's task, without its candidate and with its excerpts taken out.
    pub(super) prompt: Told<Value>,
    /// The reviewed candidate: bodies as passages without names, `inReview` and `label` on each section.
    pub(super) candidate: Told<Value>,
    pub(super) passages: Vec<Passage>,
    /// The writer's excerpts, the ones the lesson cites first.
    pub(super) sources: Packed<Told<Value>>,
    pub(super) open: Vec<OpenFinding>,
    /// What an earlier part of a cut-off answer already holds.
    pub(super) reported: Told<Vec<String>>,
}

/// One entry of a review's answer.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Entry {
    /// A defect that belongs to no section.
    Issue {
        note: String,
        continues: Option<Picked<OpenFinding>>,
    },
    /// A defect in a passage of a section in review.
    Defect {
        at: Picked<Passage>,
        finding: String,
        concerns: Vec<LessonConcern>,
        continues: Option<Picked<OpenFinding>>,
    },
    /// An open finding whose defect is gone.
    Resolved(Picked<OpenFinding>),
}

/// The kind of review: which defects it reports.
pub(crate) trait Lens: Send + Sync + 'static {
    const ID: ContractId;
    const REMIT: Remit;
}

/// A review of any material: any material defect.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Anything;

/// A lesson's review: only the concerns it owns.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Concerns;

impl Lens for Anything {
    const ID: ContractId = ContractId::new("teaching-review");
    const REMIT: Remit = Remit::Open;
}

impl Lens for Concerns {
    const ID: ContractId = ContractId::new("lesson-teaching-review");
    const REMIT: Remit = Remit::Lesson;
}

/// The call that reviews a lesson's sections or an outline.
pub(crate) struct TeachingReview<L>(PhantomData<L>);

impl<L: Lens> Contract for TeachingReview<L> {
    const ID: ContractId = L::ID;
    type Input = Input;
}

/// One concern of a section's defect, as a list entry.
struct ConcernEntry(LessonConcern);

impl Authored for ConcernEntry {
    fn form() -> Form {
        Form::new().word::<CheckConcern>("concern")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self(fields.word::<CheckConcern>("concern")?.0))
    }
}

/// The kinds of entry, told apart by `kind`.
pub(crate) struct Kinds<L>(Entry, PhantomData<L>);

impl<L: Lens> Union for Kinds<L> {
    fn forms() -> Vec<(&'static str, Form)> {
        let lesson = L::REMIT == Remit::Lesson;
        let mut issue = Form::new().text("note");
        if lesson {
            issue = issue.word::<IssueConcern>("concern");
        }
        let issue = issue.point_or_none::<OpenFinding>("continues");
        let mut defect = Form::new().point::<Passage>("passageId").text("finding");
        if lesson {
            defect = defect.list::<ConcernEntry>("concerns");
        }
        let defect = defect.point_or_none::<OpenFinding>("continues");
        vec![
            ("issue", issue),
            ("sectionDefect", defect),
            ("resolved", Form::new().point::<OpenFinding>("openFinding")),
        ]
    }

    fn read(kind: &str, fields: &mut Fields<'_, '_>) -> Option<Self> {
        let lesson = L::REMIT == Remit::Lesson;
        let entry = match kind {
            "issue" => {
                let note = fields.text("note")?;
                if lesson {
                    fields.word::<IssueConcern>("concern")?;
                }
                Entry::Issue {
                    note,
                    continues: fields.point_or_none::<OpenFinding>("continues")?,
                }
            }
            "sectionDefect" => {
                let at = fields.point::<Passage>("passageId")?;
                let finding = fields.text("finding")?;
                let concerns = if lesson {
                    fields
                        .list::<ConcernEntry>("concerns")?
                        .into_iter()
                        .map(|entry| entry.0)
                        .collect()
                } else {
                    Vec::new()
                };
                Entry::Defect {
                    at,
                    finding,
                    concerns,
                    continues: fields.point_or_none::<OpenFinding>("continues")?,
                }
            }
            "resolved" => Entry::Resolved(fields.point::<OpenFinding>("openFinding")?),
            _ => return None,
        };
        Some(Self(entry, PhantomData))
    }
}

/// One item of a review's answer.
pub(crate) struct Item<L>(pub(super) Entry, PhantomData<L>);

impl<L: Lens> Authored for Item<L> {
    fn form() -> Form {
        Form::new().one_of::<Kinds<L>>("entry")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        let Kinds(entry, _) = fields.one_of::<Kinds<L>>("entry")?;
        Some(Self(entry, PhantomData))
    }
}

impl<L: Lens> Answered for TeachingReview<L> {
    type Answer = New<Item<L>>;
    const ALONE: Alone = Alone::Fails;

    fn within(input: &Input, room: usize) -> Option<Input> {
        Some(Input {
            sources: input.sources.within(room),
            ..input.clone()
        })
    }

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        let passages = showing.show(&input.passages)?;
        let open = showing.show(&input.open)?;
        let mut candidate = input.candidate.0.clone();
        for (handle, passage) in passages.iter() {
            let place = format!(
                "/blocks/{}/bodyPassages/{}",
                passage.section, passage.number
            );
            if let Some(entry) = candidate.pointer_mut(&place).and_then(Value::as_object_mut) {
                entry.insert("id".into(), json!(handle.written()));
            }
        }
        let mut task = input.prompt.0.as_object().cloned().unwrap_or_default();
        task.insert("candidate".into(), candidate);
        if L::REMIT == Remit::Lesson {
            if let Some(Value::Object(context)) = task.get_mut("authoringContext") {
                context.remove("sources");
                context.remove("referenceCatalog");
            }
        } else if !input.sources.is_empty() {
            if let Some(Value::Object(context)) = task.get_mut("authoringContext") {
                let sources: Vec<&Value> = input.sources.iter().map(|source| &source.0).collect();
                context.insert("sources".into(), json!(sources));
            }
        }
        if !input.open.is_empty() {
            let entries: Vec<Value> = open
                .iter()
                .map(|(handle, finding)| {
                    let mut entry = Map::new();
                    entry.insert("id".into(), json!(handle.written()));
                    entry.insert("part".into(), json!(finding.part));
                    let passage = finding.passage.as_ref().and_then(|id| {
                        passages
                            .iter()
                            .find(|(_, passage)| &passage.id == id)
                            .map(|(handle, _)| handle.written())
                    });
                    if let Some(passage) = passage {
                        entry.insert("passageId".into(), json!(passage));
                    }
                    entry.insert("note".into(), json!(finding.note));
                    Value::Object(entry)
                })
                .collect();
            task.insert("openFindings".into(), json!(entries));
        }
        if !input.reported.0.is_empty() {
            task.insert("alreadyReported".into(), json!(input.reported));
        }
        let has_passages = !input.passages.is_empty();
        let has_open = !input.open.is_empty();
        let mut instructions = format!(
            "{}\n{}",
            input.system.0,
            answer_form(has_passages, has_open)
        );
        if L::REMIT == Remit::Lesson {
            instructions.push_str(concern_form(has_passages));
        }
        let mut says = Says::new(instructions, Value::Object(task).to_string());
        says.output_tokens = REVIEW_ANSWER_TOKENS;
        Ok(says)
    }
}
