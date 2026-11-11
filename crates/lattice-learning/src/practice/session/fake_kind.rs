//! A kind whose piece is a JSON array of strings, so the engine's tests need no code runner and no subject.
//!
//! A case's body is `{"index": i, "expected": "x"}`; it passes when the piece holds that string at that index.

use crate::dto::LearningPracticeProfileDto;
use crate::practice::practical_dto::{LearningSimulationSpeaker, LearningSimulationTurnDto};
use crate::practice::session::author::{Author, Authored, Commission, Failed};
use crate::practice::session::kind::{
    Account, Budget, Checker, Citation, Criterion, Diff, Established, Fit, Gaps, Hint, HintAsk,
    Hinting, Kind, LearnerCases, MisreadingNeed, Order, Parts, ReplyGuard, Rubric, Screen, Showing,
    Statement, Truth, Wording,
};
use crate::practice::session::observation::{ClockView, Coverage, Observation};
use crate::practice::session::record::{Session, SessionCheck, SessionDebrief, SessionLearnerCase};
use crate::practice::session::seen::Seen;
use async_trait::async_trait;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::practice::work::{
    Brief, Case, CaseResult, CaseStatus, CaseVisibility, Change, ChangeKind, CheckReport,
    Difficulty, Draft, Exercise, Form, KindId, MarkState, Misreading, PartMark, PartRef, Piece,
    Rung, Setting, Spacing, Stage,
};
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Map, Value};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

/// The slots of a piece, or None when the piece is not an array of strings.
fn slots(piece: &Piece) -> Option<Vec<String>> {
    piece
        .value()
        .as_array()?
        .iter()
        .map(|slot| slot.as_str().map(str::to_owned))
        .collect()
}

fn slots_of(piece: &Piece) -> Result<Vec<String>> {
    slots(piece)
        .ok_or_else(|| AppError::InvalidState("The piece is not an array of strings.".into()))
}

fn index_of(part: &PartRef) -> Result<usize> {
    part.as_str()
        .parse()
        .map_err(|_| AppError::InvalidState(format!("{} is not a slot.", part.as_str())))
}

fn piece_of(slots: Vec<String>) -> Piece {
    Piece::new(json!(slots))
}

/// What the fake's reply guard does, as plain configured values.
#[derive(Default)]
struct ReplyConfig {
    /// The reply has a `slot` field: the indexes of the letters it points at.
    slot: bool,
    /// A word the reply may not say.
    refused_word: Option<String>,
    /// A text that, in a reply, solves the exercise.
    solved_by: Option<String>,
}

/// What the fake's rubric says, as plain configured values.
struct RubricConfig {
    criteria: Vec<Criterion>,
    statement: Option<Statement>,
}

impl Default for RubricConfig {
    fn default() -> Self {
        let criterion = |id: &str, title: &str, earns: &str| Criterion {
            id: id.to_owned(),
            title: title.to_owned(),
            earns: earns.to_owned(),
        };
        Self {
            criteria: vec![
                criterion("accuracy", "Accuracy", "Got the letters right."),
                criterion("process", "Process", "Worked in small steps."),
                criterion("clarity", "Clarity", "Said what they were doing."),
            ],
            statement: None,
        }
    }
}

/// The roles of the fake kind, one struct for all of them.
pub(crate) struct Letters {
    reference: Vec<String>,
    rubric: RubricConfig,
    ambiguous: Option<String>,
    reply: ReplyConfig,
    /// The rungs the fake has no hint for.
    off: Vec<Rung>,
    /// The `observed` text of every hint asked for.
    observed: Arc<Mutex<Vec<String>>>,
    asked_when: String,
    wanted: bool,
    answers: Mutex<VecDeque<Authored>>,
    commissioned: Arc<Mutex<Vec<Commissioned>>>,
}

/// What the fake author was asked for.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Commissioned {
    pub form: Form,
    pub lesson_id: Option<String>,
    pub failed: Vec<Failed>,
}

#[async_trait]
impl Author for Letters {
    async fn draft(&self, commission: &Commission<'_>) -> Result<Authored> {
        if let Ok(mut asked) = self.commissioned.lock() {
            asked.push(Commissioned {
                form: commission.form,
                lesson_id: commission.lesson.map(|lesson| lesson.id.clone()),
                failed: commission.failed.to_vec(),
            });
        }
        self.answers
            .lock()
            .ok()
            .and_then(|mut answers| answers.pop_front())
            .ok_or_else(|| AppError::InternalError("Unexpected extra draft".into()))
    }
}

impl Fit for Letters {
    fn asked_when(&self) -> String {
        self.asked_when.clone()
    }

    fn wanted_by(&self, _profile: &LearningPracticeProfileDto) -> bool {
        self.wanted
    }
}

#[async_trait]
impl Checker for Letters {
    async fn check(
        &self,
        _brief: &Brief,
        piece: &Piece,
        cases: &[Case],
        _cancel: CancellationToken,
    ) -> Result<CheckReport> {
        let held = slots(piece);
        let results = cases
            .iter()
            .map(|case| {
                let index = case
                    .body
                    .get("index")
                    .and_then(Value::as_u64)
                    .and_then(|index| usize::try_from(index).ok());
                let expected = case.body.get("expected").and_then(Value::as_str);
                let status = match (&held, index, expected) {
                    (Some(held), Some(index), Some(expected)) => {
                        if held.get(index).map(String::as_str) == Some(expected) {
                            CaseStatus::Passed
                        } else {
                            CaseStatus::Failed
                        }
                    }
                    _ => CaseStatus::Error,
                };
                CaseResult {
                    case_id: case.id.clone(),
                    status,
                    detail: Value::Null,
                }
            })
            .collect();
        let marks = match &held {
            None => Vec::new(),
            Some(held) => self
                .reference
                .iter()
                .enumerate()
                .filter_map(|(index, wanted)| {
                    let got = held.get(index);
                    if got == Some(wanted) {
                        return None;
                    }
                    let state = if got.is_some_and(|got| got.is_empty()) {
                        MarkState::Empty
                    } else {
                        MarkState::Wrong
                    };
                    Some(PartMark {
                        part: PartRef::new(&index.to_string()),
                        state,
                    })
                })
                .collect(),
        };
        Ok(CheckReport {
            results,
            marks,
            detail: Value::Null,
        })
    }
}

impl Parts for Letters {
    fn all(&self, _brief: &Brief, piece: &Piece) -> Result<Vec<PartRef>> {
        Ok((0..slots_of(piece)?.len())
            .map(|index| PartRef::new(&index.to_string()))
            .collect())
    }

    fn holds(&self, _brief: &Brief, piece: &Piece, part: &PartRef) -> Result<bool> {
        Ok(index_of(part)? < slots_of(piece)?.len())
    }

    fn name(&self, _brief: &Brief, _piece: &Piece, part: &PartRef) -> Result<String> {
        Ok(format!("slot {}", index_of(part)?))
    }
}

impl Diff for Letters {
    fn changes(&self, _brief: &Brief, old: &Piece, new: &Piece) -> Result<Vec<Change>> {
        let (old, new) = (slots_of(old)?, slots_of(new)?);
        let mut changes = Vec::new();
        for index in 0..old.len().max(new.len()) {
            let (before, after) = (old.get(index), new.get(index));
            if before == after {
                continue;
            }
            let part = PartRef::new(&index.to_string());
            if let Some(text) = before {
                changes.push(Change {
                    part: part.clone(),
                    kind: ChangeKind::Removed,
                    text: text.clone(),
                });
            }
            if let Some(text) = after {
                changes.push(Change {
                    part,
                    kind: ChangeKind::Added,
                    text: text.clone(),
                });
            }
        }
        Ok(changes)
    }
}

impl Showing for Letters {
    fn brief(&self, brief: &Brief, shown: &[Case]) -> Result<String> {
        let mut text = format!("Brief: {}\n", brief.title);
        for case in shown {
            text.push_str(&format!("Shown: {}\n", case.title));
        }
        Ok(text)
    }

    fn piece(&self, _brief: &Brief, piece: &Piece) -> Result<String> {
        Ok(format!("{}\n", slots_of(piece)?.join(", ")))
    }

    fn case(
        &self,
        _brief: &Brief,
        case: &Case,
        result: Option<&CaseResult>,
        open: bool,
    ) -> Result<String> {
        let mut text = case.title.clone();
        if let Some(index) = open.then(|| case.body.get("index")).flatten() {
            text.push_str(&format!(" (slot {index})"));
        }
        if let Some(result) = result {
            text.push_str(&format!(" -> {}", result.status.label()));
        }
        Ok(text)
    }

    fn check_notes(&self, _brief: &Brief, detail: &Value) -> Result<Vec<String>> {
        Ok(detail
            .get("notes")
            .and_then(Value::as_array)
            .map(|notes| {
                notes
                    .iter()
                    .filter_map(|note| note.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default())
    }
}

impl Screen for Letters {
    fn work(&self, session: &Session, seen: &Seen) -> Result<Value> {
        Ok(json!({
            "spec": session.exercise.brief.spec,
            "draft": session.row.draft.value(),
            "reference": seen.reference.then(|| session.exercise.reference.value().clone()),
            "checks": session
                .checks
                .iter()
                .map(|check| json!({"id": check.id, "found": check.report.detail}))
                .collect::<Vec<_>>(),
        }))
    }

    fn debrief(&self, _session: &Session, debrief: &SessionDebrief) -> Result<Value> {
        Ok(json!({"stated": debrief.stated}))
    }
}

impl Wording for Letters {
    fn card(&self) -> String {
        "Letters".to_owned()
    }

    fn stage_instruction(&self, brief: &Brief, form: Form, stage: Stage) -> String {
        format!(
            "Do the {} step of {}.",
            self.stage_label(brief, form, stage),
            brief.title
        )
    }
}

#[async_trait]
impl ReplyGuard for Letters {
    fn rules(&self, _brief: &Brief) -> String {
        String::new()
    }

    fn format(&self) -> String {
        String::new()
    }

    fn fields(&self, _brief: &Brief) -> Result<Vec<(String, Value)>> {
        Ok(if self.reply.slot {
            vec![(
                "slot".to_owned(),
                json!({"type": "array", "items": {"type": "integer"}}),
            )]
        } else {
            Vec::new()
        })
    }

    fn parts(
        &self,
        _brief: &Brief,
        piece: &Piece,
        fields: &Map<String, Value>,
    ) -> Result<Vec<PartRef>> {
        let Some(slots) = fields.get("slot").and_then(Value::as_array) else {
            return Ok(Vec::new());
        };
        let held = slots_of(piece)?.len();
        slots
            .iter()
            .map(|slot| {
                let index = slot
                    .as_u64()
                    .and_then(|index| usize::try_from(index).ok())
                    .ok_or_else(|| AppError::InvalidInput("A slot is a whole number.".into()))?;
                if index >= held {
                    return Err(AppError::InvalidInput(format!(
                        "Slot {index} is not in the piece. Name only a slot the piece has."
                    )));
                }
                Ok(PartRef::new(&index.to_string()))
            })
            .collect()
    }

    fn check_words(
        &self,
        _exercise: &Exercise,
        _fields: &Map<String, Value>,
        content: &str,
    ) -> Result<()> {
        match &self.reply.refused_word {
            Some(word) if content.contains(word.as_str()) => Err(AppError::InvalidInput(format!(
                "The reply says \"{word}\". Never say it."
            ))),
            _ => Ok(()),
        }
    }

    async fn check_unsolved(
        &self,
        _exercise: &Exercise,
        content: &str,
        _cancel: CancellationToken,
    ) -> Result<()> {
        match &self.reply.solved_by {
            Some(marker) if content.contains(marker.as_str()) => Err(AppError::InvalidInput(
                "The reply solves the exercise. Describe the step instead.".into(),
            )),
            _ => Ok(()),
        }
    }
}

/// The name the fake's hints give a rung.
pub(crate) fn rung_name(rung: Rung) -> &'static str {
    match rung {
        Rung::Nudge => "nudge",
        Rung::Idea => "idea",
        Rung::Plan => "plan",
        Rung::Point => "point",
    }
}

#[async_trait]
impl Hinting for Letters {
    async fn hint(&self, ask: HintAsk<'_>) -> Result<Option<Hint>> {
        if let Ok(mut observed) = self.observed.lock() {
            observed.push(ask.observed.to_owned());
        }
        if self.off.contains(&ask.rung) {
            return Ok(None);
        }
        let held = slots_of(&ask.observation.piece)?;
        let parts = self
            .reference
            .iter()
            .enumerate()
            .filter(|(index, wanted)| held.get(*index) != Some(*wanted))
            .map(|(index, _)| PartRef::new(&index.to_string()))
            .collect();
        Ok(Some(Hint {
            rung: ask.rung,
            text: format!("A {} hint.", rung_name(ask.rung)),
            reason: "It fits the rung.".to_owned(),
            parts,
        }))
    }
}

impl LearnerCases for Letters {
    fn show(&self, _brief: &Brief, body: &Value) -> Result<String> {
        let letter = body.get("letter").and_then(Value::as_str).unwrap_or("?");
        Ok(format!("letter {letter}"))
    }

    fn key(&self) -> String {
        "cases".to_owned()
    }

    fn account(&self, _brief: &Brief, case: &SessionLearnerCase) -> Result<Value> {
        Ok(json!({
            "ordinal": case.ordinal,
            "letter": case.body.get("letter"),
            "verdict": case.verdict.label().to_lowercase(),
        }))
    }

    fn cited(&self) -> Citation {
        Citation {
            tag: "case".to_owned(),
            sentence: "A case: ref is the case's ordinal.".to_owned(),
        }
    }

    fn case(&self, _exercise: &Exercise, case: &SessionLearnerCase) -> Result<Case> {
        let letter = case
            .body
            .get("letter")
            .and_then(Value::as_str)
            .unwrap_or("?");
        Ok(Case {
            id: format!("mine-{}", case.ordinal),
            title: format!("Your case {}", case.ordinal),
            visibility: CaseVisibility::Shown,
            body: json!({"index": case.ordinal, "expected": letter}),
        })
    }
}

/// An easy exercise gets ten minutes; any other is untimed.
impl Budget for Letters {
    fn seconds(&self, brief: &Brief) -> Option<u32> {
        (brief.difficulty == Difficulty::Easy).then_some(600)
    }
}

impl Rubric for Letters {
    fn criteria(&self, _form: Form) -> Vec<Criterion> {
        self.rubric.criteria.clone()
    }

    fn statement(&self, _form: Form) -> Option<Statement> {
        self.rubric.statement.clone()
    }
}

impl Account for Letters {
    fn exercise_key(&self) -> String {
        "exercise".to_owned()
    }

    fn exercise(&self, exercise: &Exercise) -> Result<Value> {
        Ok(json!({
            "title": exercise.brief.title,
            "reference": slots_of(&exercise.reference)?,
        }))
    }

    fn form_key(&self) -> String {
        "form".to_owned()
    }

    fn form(&self, form: Form) -> Result<Value> {
        Ok(json!(form.label().to_lowercase()))
    }

    fn checks_key(&self) -> String {
        "checks".to_owned()
    }

    fn check(
        &self,
        _exercise: &Exercise,
        check: &SessionCheck,
        before: Option<&SessionCheck>,
    ) -> Result<Value> {
        let slots = match before {
            Some(before) if before.piece == check.piece => {
                json!(format!("the same as check {}", before.id))
            }
            _ => json!(slots_of(&check.piece)?),
        };
        Ok(json!({
            "id": check.id,
            "scope": check.scope.label(),
            "passed": check.report.results.iter().filter(|result| result.status == CaseStatus::Passed).count(),
            "total": check.report.results.len(),
            "slots": slots,
        }))
    }

    fn piece(&self, _exercise: &Exercise, piece: &Piece) -> Result<Value> {
        Ok(json!(slots_of(piece)?))
    }

    fn cites_parts(&self) -> Citation {
        Citation {
            tag: "slot".to_owned(),
            sentence: "A slot: ref is a check id, index is a slot of that check, counted from 0."
                .to_owned(),
        }
    }

    fn cites_check(&self) -> Citation {
        Citation {
            tag: "check".to_owned(),
            sentence: "A check: ref is a check id.".to_owned(),
        }
    }

    fn fields(&self) -> Vec<(String, Value)> {
        vec![("index".to_owned(), json!({"type": "integer"}))]
    }

    fn leave_empty(&self) -> String {
        "Leave index 0 when a slot is not cited.".to_owned()
    }

    /// The slot is not checked against the piece here: that is the engine's to require.
    fn parts(
        &self,
        _exercise: &Exercise,
        _check: &SessionCheck,
        fields: &Map<String, Value>,
    ) -> Result<Vec<PartRef>> {
        let index = fields
            .get("index")
            .and_then(Value::as_u64)
            .ok_or_else(|| AppError::InvalidInput("index is a whole number from 0.".into()))?;
        Ok(vec![PartRef::new(&index.to_string())])
    }
}

impl Gaps for Letters {
    fn emptied(&self, _brief: &Brief, piece: &Piece, parts: &[PartRef]) -> Result<Piece> {
        let mut held = slots_of(piece)?;
        for part in parts {
            let slot = held
                .get_mut(index_of(part)?)
                .ok_or_else(|| AppError::InvalidState(format!("No slot {}.", part.as_str())))?;
            slot.clear();
        }
        Ok(piece_of(held))
    }

    fn restored(
        &self,
        _brief: &Brief,
        piece: &Piece,
        from: &Piece,
        part: &PartRef,
    ) -> Result<Piece> {
        let index = index_of(part)?;
        let mut held = slots_of(piece)?;
        let source = slots_of(from)?;
        let (slot, text) = held
            .get_mut(index)
            .zip(source.get(index))
            .ok_or_else(|| AppError::InvalidState(format!("No slot {}.", part.as_str())))?;
        *slot = text.clone();
        Ok(piece_of(held))
    }
}

impl Order for Letters {
    fn shuffled(&self, _brief: &Brief, piece: &Piece, seed: u64) -> Result<Piece> {
        let mut held = slots_of(piece)?;
        let len = u64::try_from(held.len()).unwrap_or(u64::MAX);
        if len > 1 {
            let by = usize::try_from(seed % len).unwrap_or(0).max(1);
            held.rotate_left(by);
        }
        Ok(piece_of(held))
    }
}

#[async_trait]
impl Truth for Letters {
    async fn establish(&self, draft: &Draft, cancel: CancellationToken) -> Result<Established> {
        let report =
            Checker::check(self, &draft.brief, &draft.reference, &draft.cases, cancel).await?;
        if report.all_passed() {
            return Ok(Established::Right {
                cases: draft.cases.clone(),
                proof: Value::Null,
            });
        }
        let failing: Vec<&str> = report
            .results
            .iter()
            .filter(|result| result.status != CaseStatus::Passed)
            .map(|result| result.case_id.as_str())
            .collect();
        Ok(Established::Wrong(format!(
            "The reference fails the cases {}.",
            failing.join(", ")
        )))
    }

    async fn unambiguous(
        &self,
        _exercise: &Exercise,
        _cancel: CancellationToken,
    ) -> Result<Option<String>> {
        Ok(self.ambiguous.clone())
    }
}

/// Builds the fake kind with the roles and needs a test chooses.
pub(crate) struct FakeKindBuilder {
    id: String,
    asked_when: String,
    wanted: bool,
    reference: Vec<String>,
    rubric: RubricConfig,
    gaps: bool,
    order: bool,
    learner_cases: bool,
    needed: usize,
    ambiguous: Option<String>,
    spacing: Spacing,
    reply: ReplyConfig,
    off: Vec<Rung>,
    script: Vec<Authored>,
}

/// A fake kind whose reference piece is `reference`. It supports Build and Repair only, needs no misreadings and
/// finds every exercise unambiguous until a test says otherwise.
pub(crate) fn fake_kind(reference: &[&str]) -> FakeKindBuilder {
    FakeKindBuilder {
        id: "letters".to_owned(),
        asked_when: "Plain work with letters. A course asks for it when learners arrange letters."
            .to_owned(),
        wanted: false,
        reference: reference.iter().map(|slot| (*slot).to_owned()).collect(),
        rubric: RubricConfig::default(),
        gaps: false,
        order: false,
        learner_cases: false,
        needed: 0,
        ambiguous: None,
        spacing: Spacing::Item,
        reply: ReplyConfig::default(),
        off: Vec::new(),
        script: Vec::new(),
    }
}

impl FakeKindBuilder {
    /// The kind is registered under `id`.
    pub(crate) fn with_id(mut self, id: &str) -> Self {
        self.id = id.to_owned();
        self
    }

    /// What the fit check is told about the kind.
    pub(crate) fn asked_when(mut self, words: &str) -> Self {
        self.asked_when = words.to_owned();
        self
    }

    /// Every profile asks for the kind outright.
    pub(crate) fn wanted_outright(mut self) -> Self {
        self.wanted = true;
        self
    }

    pub(crate) fn with_spacing(mut self, spacing: Spacing) -> Self {
        self.spacing = spacing;
        self
    }

    pub(crate) fn with_gaps(mut self) -> Self {
        self.gaps = true;
        self
    }

    pub(crate) fn with_order(mut self) -> Self {
        self.order = true;
        self
    }

    /// The debrief states a model answer and a pitfall beside the scores.
    pub(crate) fn stating_a_model_and_a_pitfall(mut self) -> Self {
        self.rubric.statement = Some(Statement {
            names: "the model answer and the pitfall to avoid".to_owned(),
            keys: vec!["model".to_owned(), "pitfall".to_owned()],
            says: "The model is how a strong learner fills the slots, and the pitfall is the mistake to avoid."
                .to_owned(),
        });
        self
    }

    pub(crate) fn with_learner_cases(mut self) -> Self {
        self.learner_cases = true;
        self
    }

    pub(crate) fn needing(mut self, misreadings: usize) -> Self {
        self.needed = misreadings;
        self
    }

    pub(crate) fn ambiguous(mut self, reason: &str) -> Self {
        self.ambiguous = Some(reason.to_owned());
        self
    }

    /// Replies carry a `slot` field: the indexes of the letters they point at.
    pub(crate) fn with_reply_slot(mut self) -> Self {
        self.reply.slot = true;
        self
    }

    /// Replies that say `word` are refused.
    pub(crate) fn refusing_word(mut self, word: &str) -> Self {
        self.reply.refused_word = Some(word.to_owned());
        self
    }

    /// Replies that hold `marker` solve the exercise.
    pub(crate) fn solving_with(mut self, marker: &str) -> Self {
        self.reply.solved_by = Some(marker.to_owned());
        self
    }

    /// The author answers each commission with the next of `script`.
    pub(crate) fn authoring(mut self, script: Vec<Authored>) -> Self {
        self.script = script;
        self
    }

    /// The kind has no hint of `rung`.
    pub(crate) fn without_rung(mut self, rung: Rung) -> Self {
        self.off.push(rung);
        self
    }

    pub(crate) fn build(self) -> Kind {
        self.assemble().kind
    }

    /// The kind, and the `observed` text of every hint it is asked for.
    pub(crate) fn build_recording(self) -> (Kind, Arc<Mutex<Vec<String>>>) {
        let assembled = self.assemble();
        (assembled.kind, assembled.observed)
    }

    /// The kind, and what its author is commissioned with.
    pub(crate) fn build_authoring(self) -> (Kind, Arc<Mutex<Vec<Commissioned>>>) {
        let assembled = self.assemble();
        (assembled.kind, assembled.commissioned)
    }

    fn assemble(self) -> Assembled {
        let observed = Arc::new(Mutex::new(Vec::new()));
        let commissioned = Arc::new(Mutex::new(Vec::new()));
        let roles = Arc::new(Letters {
            reference: self.reference,
            rubric: self.rubric,
            ambiguous: self.ambiguous,
            reply: self.reply,
            off: self.off,
            observed: observed.clone(),
            asked_when: self.asked_when,
            wanted: self.wanted,
            answers: Mutex::new(self.script.into()),
            commissioned: commissioned.clone(),
        });
        let kind = Kind {
            id: KindId::new(&self.id),
            spacing: self.spacing,
            misreadings_needed: MisreadingNeed {
                count: self.needed,
                reason: "the fake kind counts them",
            },
            wording: roles.clone(),
            reply_guard: roles.clone(),
            hinting: roles.clone(),
            rubric: roles.clone(),
            account: roles.clone(),
            budget: roles.clone(),
            author: roles.clone(),
            checker: roles.clone(),
            parts: roles.clone(),
            diff: roles.clone(),
            showing: roles.clone(),
            screen: roles.clone(),
            truth: roles.clone(),
            fit: roles.clone(),
            gaps: self.gaps.then(|| roles.clone() as Arc<dyn Gaps>),
            order: self.order.then(|| roles.clone() as Arc<dyn Order>),
            learner_cases: self.learner_cases.then_some(roles as Arc<dyn LearnerCases>),
        };
        Assembled {
            kind,
            observed,
            commissioned,
        }
    }
}

/// The kind with the records its fake roles keep.
struct Assembled {
    kind: Kind,
    observed: Arc<Mutex<Vec<String>>>,
    commissioned: Arc<Mutex<Vec<Commissioned>>>,
}

/// An exercise of the fake kind: the reference is a, b, c; the id of its case is `case-1` and of its misreading
/// `miss-1`.
pub(crate) fn letters_exercise() -> Exercise {
    Exercise {
        kind: KindId::new("letters"),
        form: Form::Build,
        brief: Brief {
            title: "Spell it".to_owned(),
            difficulty: Difficulty::Easy,
            spec: Value::Null,
        },
        reference: piece_of(vec!["a".into(), "b".into(), "c".into()]),
        misreadings: vec![Misreading {
            id: "miss-1".to_owned(),
            description: "Reads the letters backwards".to_owned(),
            piece: piece_of(vec!["c".into(), "b".into(), "a".into()]),
            killed_by: vec!["case-1".to_owned()],
        }],
        cases: vec![Case {
            id: "case-1".to_owned(),
            title: "First slot".to_owned(),
            visibility: CaseVisibility::Hidden,
            body: json!({"index": 0, "expected": "a"}),
        }],
        start: piece_of(vec![String::new(), String::new(), String::new()]),
        planted: None,
        proof: Value::Null,
    }
}

/// What the counterpart sees of a session of the fake kind in `setting`: the session `session-1`, one turn
/// `turn-7`, and the draft a, x, c.
pub(crate) fn letters_observation(setting: Setting) -> Observation {
    Observation {
        session_id: "session-1".to_owned(),
        form: Form::Build,
        setting,
        stage: Stage::Make,
        stages_reached: vec![Stage::Understand, Stage::Plan, Stage::Make],
        brief: letters_exercise().brief,
        shown: Vec::new(),
        clock: ClockView {
            active_ms: 61_000,
            budget_seconds: None,
        },
        piece: piece_of(vec!["a".into(), "x".into(), "c".into()]),
        has_baseline: false,
        changes: Vec::new(),
        last_check: None,
        learner_cases: Vec::new(),
        coverage: Coverage {
            caught: 0,
            total: 0,
            uncaught: None,
        },
        hints: Vec::new(),
        turns: vec![LearningSimulationTurnDto {
            id: "turn-7".to_owned(),
            ordinal: 0,
            speaker: LearningSimulationSpeaker::Learner,
            content: "Where do I start?".to_owned(),
            citations: Vec::new(),
            model_name: None,
            created_at: 0,
        }],
    }
}

/// A model that answers with the next canned text and keeps every request it was sent.
pub(crate) struct ScriptedModel {
    answers: Mutex<VecDeque<String>>,
    requests: Mutex<Vec<CompletionRequest>>,
}

impl ScriptedModel {
    pub(crate) fn new<'a>(answers: impl IntoIterator<Item = &'a str>) -> Self {
        Self {
            answers: Mutex::new(answers.into_iter().map(str::to_owned).collect()),
            requests: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn requests(&self) -> Vec<CompletionRequest> {
        self.requests
            .lock()
            .map(|sent| sent.clone())
            .unwrap_or_default()
    }

    /// The messages of every request, joined, in the order sent.
    pub(crate) fn texts(&self) -> Vec<String> {
        self.requests()
            .iter()
            .map(|request| {
                request
                    .input
                    .iter()
                    .filter_map(|input| match input {
                        CompletionInput::Message { content, .. } => Some(content.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .collect()
    }
}

#[async_trait]
impl LLMPort for ScriptedModel {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        if let Ok(mut sent) = self.requests.lock() {
            sent.push(request.clone());
        }
        let next = self
            .answers
            .lock()
            .ok()
            .and_then(|mut answers| answers.pop_front());
        next.map(CompletionResponse::from_text)
            .ok_or_else(|| AppError::InternalError("Unexpected extra model call".into()))
    }

    fn model_name(&self) -> &str {
        "scripted-model"
    }

    fn max_context_tokens(&self) -> usize {
        128_000
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}
