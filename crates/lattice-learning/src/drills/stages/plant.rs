//! Stage 3: the model plants one defect for each target pattern, as a change to the lines of the clean code. Code
//! makes the change, so the edit's text and the key's line spans are read off the result and never written by the
//! model. When the brief has evidence, up to half the changes (rounded up) may be lesson issues: each cites the
//! evidence it breaks, and is of a pattern of the list or of another pattern of the catalogue.

use super::ask::{describe_fault, normalise};
use super::code_units::{describe_edit, placed, CodeFile, Weight};
use super::outputs::{Baseline, Mutation, Planted, Trail, Verification};
use super::DrillBuild;
use crate::drills::budget;
use crate::drills::material::{PlantFrame, PlantWording};
use crate::drills::proof::is_behavioural;
use crate::drills::spec::DrillSpec;
use crate::lessons::structured_call::Asking;
use crate::model_call::answer_options;
use lattice_core::application::services::model_contract::ask::{
    ask_each, Alone, Answered, Refusal, Says, Settled,
};
use lattice_core::application::services::model_contract::edit::Consistent;
use lattice_core::application::services::model_contract::shape::{Authored, Each, Fields, Form};
use lattice_core::application::services::model_contract::shown::{
    Drawn, Internal, Kind, Picked, Scope, Showing, Told,
};
use lattice_core::application::services::model_contract::units::{Grain, Unit, Units};
use lattice_core::application::services::model_contract::{Contract, ContractId};
use lattice_core::domain::learning::drills::{
    apply, patterns_for, DrillFile, Edit, Family, LessonBrief, Pattern, Severity,
};
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};

/// Said after the wording of the stage: the form of the answer's changes.
const CHANGE_FORM: &str = " A change has four lists: `replace`, whose entries name a line in `at` and give in `with` the lines it becomes; `insert`, whose entries give new lines in `with` to put after the line named in `after`, or after `start`; `remove`, the names of lines to delete; and `move`, whose entries name a line in `at` and the line it goes after in `after`. Leave a list empty when the change does not need it.";

/// A pattern asked for: one change is planted for each.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Target {
    id: Internal<String>,
    name: Told<String>,
    family: Told<String>,
    definition: Told<String>,
    /// What was wrong with the earlier answer for this pattern, when it is asked again.
    note: Option<Told<String>>,
}

impl Target {
    fn of(pattern: &Pattern, note: Option<String>) -> Self {
        Self {
            id: Internal::of(pattern.id.to_owned()),
            name: Told(pattern.name.to_owned()),
            family: Told(pattern.family.slug().to_owned()),
            definition: Told(pattern.definition.to_owned()),
            note: note.map(Told),
        }
    }
}

impl Kind for Target {
    const PREFIX: &'static str = "t";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = String;

    fn reference(&self) -> String {
        self.id.value().clone()
    }
}

/// A pattern of the catalogue that is not asked for, which a lesson issue may be of.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Catalogued {
    id: Internal<String>,
    name: Told<String>,
    family: Told<String>,
}

impl Catalogued {
    fn of(pattern: &Pattern) -> Self {
        Self {
            id: Internal::of(pattern.id.to_owned()),
            name: Told(pattern.name.to_owned()),
            family: Told(pattern.family.slug().to_owned()),
        }
    }
}

impl Kind for Catalogued {
    const PREFIX: &'static str = "c";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = String;

    fn reference(&self) -> String {
        self.id.value().clone()
    }
}

/// A unit of the brief's evidence, counting from zero.
#[derive(Debug, Clone)]
pub(super) struct Cited {
    at: usize,
}

impl Kind for Cited {
    const PREFIX: &'static str = "e";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = usize;

    fn reference(&self) -> usize {
        self.at
    }
}

/// What one request says about the patterns still open.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Input {
    instructions: Told<String>,
    frame: Told<PlantFrame>,
    brief: Option<Told<LessonBrief>>,
    files: Vec<CodeFile>,
    targets: Vec<Target>,
    /// The patterns a lesson issue may be of besides the targets.
    catalogue: Vec<Catalogued>,
    /// The changes already made, which stay.
    others: Vec<Told<Edit>>,
    /// The patterns already planted.
    planted: Vec<Internal<String>>,
    /// How many more lesson issues may be given.
    lessons_left: Told<usize>,
    /// The output room the answer is promised.
    room: Internal<usize>,
}

/// One planted defect: the change to the code and what the model says of it. The statement has the fields of a
/// test when `TESTED`, and of a lesson issue when `LESSONS`.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Plan<const TESTED: bool, const LESSONS: bool> {
    cited: Vec<Picked<Cited>>,
    pattern: Option<Picked<Catalogued>>,
    change: Consistent<Unit, String>,
    severity: Severity,
    consequence: String,
    explanation: String,
    killing_test: String,
    no_test_because: String,
}

impl<const TESTED: bool, const LESSONS: bool> Authored for Plan<TESTED, LESSONS> {
    fn form() -> Form {
        let mut form = Form::new();
        if LESSONS {
            form = form
                .cite::<Cited>("lessonSentences")
                .point_or_none::<Catalogued>("lessonPattern");
        }
        form = form
            .change::<Unit, String>("change")
            .word::<Weight>("severity")
            .text("consequence")
            .text("explanation");
        if TESTED {
            form = form.text("killingTest").text("noTestBecause");
        }
        form
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        let (cited, pattern) = if LESSONS {
            (
                fields.cite::<Cited>("lessonSentences"),
                fields.point_or_none::<Catalogued>("lessonPattern")?,
            )
        } else {
            (Vec::new(), None)
        };
        let change = fields.change::<Unit, String>("change")?;
        let severity = fields.word::<Weight>("severity")?.0;
        let consequence = fields.text("consequence")?;
        let explanation = fields.text("explanation")?;
        let (killing_test, no_test_because) = if TESTED {
            (fields.text("killingTest")?, fields.text("noTestBecause")?)
        } else {
            (String::new(), String::new())
        };
        Some(Self {
            cited,
            pattern,
            change,
            severity,
            consequence,
            explanation,
            killing_test,
            no_test_because,
        })
    }
}

/// The call that plants the defects.
pub(super) struct Plant<const TESTED: bool, const LESSONS: bool>;

impl<const TESTED: bool, const LESSONS: bool> Contract for Plant<TESTED, LESSONS> {
    const ID: ContractId = ContractId::new("drill-plant");
    type Input = Input;
}

/// The names `fields` as a sentence's list.
fn listed(fields: &[&str]) -> String {
    match fields.split_last() {
        Some((last, [])) => (*last).to_owned(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
        None => String::new(),
    }
}

impl<const TESTED: bool, const LESSONS: bool> Answered for Plant<TESTED, LESSONS> {
    type Answer = Each<Target, Plan<TESTED, LESSONS>>;
    const ALONE: Alone = Alone::Fails;

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        let frame = &input.frame.0;
        let targets = showing.show(&input.targets)?;
        let catalogue = showing.show(&input.catalogue)?;
        let evidence: Vec<Cited> = input
            .brief
            .as_ref()
            .map(|brief| (0..brief.0.evidence.len()).map(|at| Cited { at }).collect())
            .unwrap_or_default();
        let labels: Vec<String> = showing
            .show(&evidence)?
            .iter()
            .map(|(handle, _)| handle.bracketed())
            .collect();
        let mut task = frame.before.clone();
        if let Some(brief) = &input.brief {
            task.push_str(&brief.0.render_with(|number| {
                number
                    .checked_sub(1)
                    .and_then(|at| labels.get(at))
                    .cloned()
                    .unwrap_or_default()
            }));
            task.push_str("\n\n");
        }
        task.push_str(&frame.heading);
        for (at, file) in input.files.iter().enumerate() {
            if at > 0 {
                task.push_str(&frame.gap);
            }
            let units = Units::of(&file.text.0, Grain::Line);
            let shown = units.shown(showing)?;
            task.push_str(&frame.open.replace("{path}", &file.path.0));
            task.push('\n');
            task.push_str(&units.rows(&shown));
            task.push('\n');
            task.push_str(&frame.close);
        }
        task.push_str(&frame.after);
        task.push_str(if LESSONS {
            "\nThe patterns, in order:\n"
        } else {
            "\nThe patterns, one change for each:\n"
        });
        for (handle, target) in targets.iter() {
            task.push_str(&format!(
                "- {} ({}, family {}): {}\n",
                handle.bracketed(),
                target.name.0,
                target.family.0,
                target.definition.0
            ));
        }
        if LESSONS {
            let left = input.lessons_left.0;
            if left > 0 {
                task.push_str(&format!(
                    "\nMake one change for each pattern of this list, each of a different pattern. Up to {left} of the changes may be lesson issues. A lesson issue stands in the entry of one pattern of this list: it is that pattern's change, or, when its defect is an instance of another pattern of the catalogue, it takes the place of that pattern's change.\n\nThe other patterns of the catalogue, for the pattern of a lesson issue:\n"
                ));
            } else {
                task.push_str("\nMake one change for each pattern of this list, each of a different pattern. No lesson issue is left to give: every change has an empty `lessonSentences` and `none` in `lessonPattern`.\n\nThe other patterns of the catalogue:\n");
            }
            for (handle, pattern) in catalogue.iter() {
                task.push_str(&format!(
                    "- {} ({}, family {})\n",
                    handle.bracketed(),
                    pattern.name.0,
                    pattern.family.0
                ));
            }
        }
        if !input.others.is_empty() {
            let others: Vec<String> = input
                .others
                .iter()
                .map(|edit| describe_edit(&edit.0))
                .collect();
            task.push_str(&format!(
                "\nThe other changes, which stay. Each new change touches different lines from them, with an unchanged line between:\n{}\n",
                others.join("\n\n")
            ));
        }
        let noted: Vec<String> = targets
            .iter()
            .filter_map(|(handle, target)| {
                target
                    .note
                    .as_ref()
                    .map(|note| format!("- {}: {}", handle.bracketed(), note.0))
            })
            .collect();
        if !noted.is_empty() {
            task.push_str(&format!(
                "\nAbout your earlier answer for these patterns:\n{}\n",
                noted.join("\n")
            ));
        }
        let mut fields = Vec::new();
        if LESSONS {
            fields.extend(["`lessonSentences`", "`lessonPattern`"]);
        }
        fields.extend(["`change`", "`severity`", "`consequence`", "`explanation`"]);
        if TESTED {
            fields.extend(["`killingTest`", "`noTestBecause`"]);
        }
        let instructions = format!(
            "{} Answer with an `entries` list holding one entry for each pattern of the list, its `of` the label of the pattern. An entry holds {}.{CHANGE_FORM}",
            input.instructions.0,
            listed(&fields)
        );
        let mut says = Says::new(instructions, task);
        says.output_tokens = *input.room.value();
        Ok(says)
    }

    fn accept(input: &Input, plan: &Plan<TESTED, LESSONS>) -> std::result::Result<(), Refusal> {
        if plan.consequence.trim().is_empty() || plan.explanation.trim().is_empty() {
            return Err(Refusal("Give the consequence and the explanation.".into()));
        }
        if !plan.killing_test.trim().is_empty() && !plan.no_test_because.trim().is_empty() {
            return Err(Refusal(
                "Give either `killingTest` or `noTestBecause`, not both.".into(),
            ));
        }
        if plan.cited.is_empty() && plan.pattern.is_some() {
            return Err(Refusal(
                "A pattern in `lessonPattern` goes with the evidence the lesson issue breaks in `lessonSentences`. For a change that is not a lesson issue, give `none`.".into(),
            ));
        }
        if !plan.cited.is_empty() && input.lessons_left.0 == 0 {
            return Err(Refusal(
                "No lesson issue is left to give: give an empty `lessonSentences` and `none` in `lessonPattern`.".into(),
            ));
        }
        if let Some(pattern) = &plan.pattern {
            if input
                .planted
                .iter()
                .any(|planted| planted.value() == &pattern.reference)
            {
                return Err(Refusal(
                    "That pattern of the catalogue already has a change, and every change is of a different pattern. Give another pattern, or the change for the pattern of this entry with `none` in `lessonPattern`.".into(),
                ));
            }
        }
        let others: Vec<Edit> = input.others.iter().map(|edit| edit.0.clone()).collect();
        placed(&input.files, &others, &plan.change).map(|_| ())
    }
}

/// What the stage is asked about, the same in every round.
struct Base<'a> {
    targets: Vec<&'static Pattern>,
    /// The patterns of the material that are not targets.
    others: Vec<&'static Pattern>,
    files: &'a [DrillFile],
    coded: Vec<CodeFile>,
    tests: &'a [String],
    wording: PlantWording,
    brief: Option<LessonBrief>,
}

/// What the rounds so far planted.
#[derive(Default)]
struct Standing {
    edits: Vec<Edit>,
    planted: Vec<String>,
    lessons: usize,
}

/// What the stage makes of one answered plan.
enum Judged {
    Placed(Box<Mutation>),
    /// It meets what an earlier plan of the round planted: it is asked again, beside that.
    Conflict,
    /// It cannot stand, and the model is told why.
    Faulty(String),
}

/// What the plan under the pattern `own` comes to, beside what is already planted.
fn judge<const TESTED: bool, const LESSONS: bool>(
    plan: &Plan<TESTED, LESSONS>,
    own: &'static Pattern,
    base: &Base<'_>,
    standing: &Standing,
) -> Judged {
    let pattern = plan
        .pattern
        .as_ref()
        .and_then(|picked| {
            base.others
                .iter()
                .find(|known| known.id == picked.reference)
                .copied()
        })
        .unwrap_or(own);
    let lesson = !plan.cited.is_empty();
    if lesson && standing.lessons >= base.wording.lesson_issues {
        return Judged::Conflict;
    }
    if standing.planted.iter().any(|planted| planted == pattern.id) {
        return Judged::Conflict;
    }
    let Ok((edit, _)) = placed(&base.coded, &standing.edits, &plan.change) else {
        return Judged::Conflict;
    };
    if pattern.family == Family::Testing && !base.tests.contains(&edit.path) {
        return Judged::Faulty(format!(
            "A change for a testing pattern edits a test file ({}), not {}.",
            base.tests.join(", "),
            edit.path
        ));
    }
    let verification = if TESTED && is_behavioural(pattern.family) {
        let test = Some(normalise(&plan.killing_test)).filter(|test| !test.trim().is_empty());
        let reason = Some(plan.no_test_because.trim()).filter(|reason| !reason.is_empty());
        match (test, reason) {
            (Some(test), None) => Verification::KillingTest(test),
            (None, Some(reason)) => Verification::Unobservable(reason.to_owned()),
            (Some(_), Some(_)) => {
                return Judged::Faulty(
                    "Give either `killingTest` or `noTestBecause`, not both.".into(),
                )
            }
            (None, None) => {
                return Judged::Faulty(
                    "Give a killing test, or in `noTestBecause` the reason that no deterministic test can observe this defect.".into(),
                )
            }
        }
    } else {
        Verification::NotAsked
    };
    Judged::Placed(Box::new(Mutation {
        pattern: pattern.id.to_owned(),
        edit,
        severity: plan.severity,
        consequence: plan.consequence.clone(),
        explanation: plan.explanation.clone(),
        verification,
        lesson_sentences: plan
            .cited
            .iter()
            .map(|picked| picked.reference.saturating_add(1))
            .collect(),
    }))
}

impl DrillBuild<'_> {
    /// Stage 3. One mutation for each target pattern that applies to the baseline and can be undone: the lesson
    /// issues after the others.
    pub(crate) async fn plant(&self, spec: &DrillSpec, baseline: &Baseline) -> Result<Planted> {
        let targets = spec.targets()?;
        let wording = self.material.plant_wording(spec, baseline, &targets);
        let others: Vec<&'static Pattern> = patterns_for(&spec.material)
            .filter(|pattern| !targets.iter().any(|target| target.id == pattern.id))
            .collect();
        let lessons = wording.lesson_issues > 0;
        let brief = baseline
            .brief
            .clone()
            .filter(|_| lessons)
            .map(|brief| LessonBrief {
                lesson_id: String::new(),
                ..brief
            });
        let tested = wording.asks_verification;
        let base = Base {
            targets,
            others,
            files: &baseline.files,
            coded: baseline.files.iter().map(CodeFile::of).collect(),
            tests: &baseline.tests,
            wording,
            brief,
        };
        match (tested, lessons) {
            (false, false) => self.planting::<false, false>(&base).await,
            (false, true) => self.planting::<false, true>(&base).await,
            (true, false) => self.planting::<true, false>(&base).await,
            (true, true) => self.planting::<true, true>(&base).await,
        }
    }

    /// Asks for the patterns still open until every one has its change.
    async fn planting<const TESTED: bool, const LESSONS: bool>(
        &self,
        base: &Base<'_>,
    ) -> Result<Planted> {
        let model = self.llm.model_name().to_owned();
        let mut scope = Scope::for_question();
        let mut trail = Trail::default();
        let mut slots: Vec<Option<Mutation>> = base.targets.iter().map(|_| None).collect();
        let mut notes: Vec<Option<String>> = base.targets.iter().map(|_| None).collect();
        let mut strikes: Vec<usize> = base.targets.iter().map(|_| 0).collect();
        let mut standing = Standing::default();
        let mut round = 0usize;
        loop {
            let open: Vec<usize> = slots
                .iter()
                .enumerate()
                .filter(|(_, slot)| slot.is_none())
                .map(|(at, _)| at)
                .collect();
            if open.is_empty() {
                break;
            }
            let items: Vec<Target> = open
                .iter()
                .filter_map(|&at| {
                    let pattern = base.targets.get(at)?;
                    Some(Target::of(pattern, notes.get(at).cloned().flatten()))
                })
                .collect();
            let shared = Input {
                instructions: Told(base.wording.system.clone()),
                frame: Told(base.wording.frame.clone()),
                brief: base.brief.clone().map(Told),
                files: base.coded.clone(),
                targets: Vec::new(),
                catalogue: base
                    .others
                    .iter()
                    .map(|pattern| Catalogued::of(pattern))
                    .collect(),
                others: standing.edits.iter().cloned().map(Told).collect(),
                planted: standing.planted.iter().cloned().map(Internal::of).collect(),
                lessons_left: Told(base.wording.lesson_issues.saturating_sub(standing.lessons)),
                room: Internal::of(budget::mutations(open.len())),
            };
            let settled = ask_each::<Plant<TESTED, LESSONS>>(
                self.llm,
                &Asking {
                    progress: None,
                    reasoning: None,
                },
                &mut scope,
                &items,
                |open, _| Input {
                    targets: open.iter().map(|target| (*target).clone()).collect(),
                    ..shared.clone()
                },
                &answer_options(),
            )
            .await?;
            trail.call(
                if round == 0 {
                    "plant"
                } else {
                    "plant (asked again)"
                },
                &model,
            );
            round = round.saturating_add(1);
            let mut moved = false;
            for (&at, settled) in open.iter().zip(settled) {
                let Settled::Answered(plan) = settled else {
                    return Err(AppError::ModelAnswerUnusable(
                        "The answer said nothing about a pattern asked for.".into(),
                    ));
                };
                let Some(&own) = base.targets.get(at) else {
                    continue;
                };
                match judge(&plan, own, base, &standing) {
                    Judged::Placed(mutation) => {
                        standing.edits.push(mutation.edit.clone());
                        standing.planted.push(mutation.pattern.clone());
                        if !mutation.lesson_sentences.is_empty() {
                            standing.lessons = standing.lessons.saturating_add(1);
                        }
                        if let Some(slot) = slots.get_mut(at) {
                            *slot = Some(*mutation);
                        }
                        moved = true;
                    }
                    Judged::Conflict => {}
                    Judged::Faulty(told) => {
                        moved = true;
                        let struck = strikes.get_mut(at).map(|count| {
                            *count = count.saturating_add(1);
                            *count
                        });
                        if struck.unwrap_or(0) >= 2 {
                            return Err(AppError::ModelAnswerUnusable(told));
                        }
                        if let Some(note) = notes.get_mut(at) {
                            *note = Some(told);
                        }
                    }
                }
            }
            if !moved {
                return Err(AppError::ModelAnswerUnusable(
                    "The changes of the answer could not be made together.".into(),
                ));
            }
        }
        let (lessons, catalogued): (Vec<Mutation>, Vec<Mutation>) = slots
            .into_iter()
            .flatten()
            .partition(|mutation| !mutation.lesson_sentences.is_empty());
        let mutations: Vec<Mutation> = catalogued.into_iter().chain(lessons).collect();
        undoable(base.files, &mutations)?;
        Ok(Planted { mutations, trail })
    }
}

/// Checks that the mutations apply together to the files and that each can be undone alone.
fn undoable(files: &[DrillFile], mutations: &[Mutation]) -> Result<()> {
    let edits: Vec<Edit> = mutations
        .iter()
        .map(|mutation| mutation.edit.clone())
        .collect();
    let labels: Vec<String> = mutations
        .iter()
        .map(|mutation| format!("Edit for {}", mutation.pattern))
        .collect();
    let paths: Vec<String> = files.iter().map(|file| file.path.clone()).collect();
    let (changed, _) = apply(files, &edits).map_err(|fault| {
        AppError::ModelAnswerUnusable(describe_fault(fault, &edits, &labels, &paths))
    })?;
    for mutation in mutations {
        let fix = mutation.fix();
        if apply(&changed, std::slice::from_ref(&fix)).is_err() {
            return Err(AppError::ModelAnswerUnusable(format!(
                "The change for {} cannot be undone.",
                mutation.pattern
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "plant/tests.rs"]
mod tests;
