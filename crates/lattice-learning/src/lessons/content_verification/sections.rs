//! Sections after cuts (claim classes design §3.10). A cut can leave a word
//! pointing at nothing, and a section can be left with too little to teach.
//! Every section that took a cut gets one bounded flow repair after the last,
//! a section left with too little goes out of the lesson, and a lesson that
//! loses too much is not published.
//!
//! The sections that go stay in the draft, at their units, until the lesson
//! is final: the checks leave them out (`ClaimChecks::removed`), so everything
//! the checks keep per unit stays valid, and the last step takes them out in
//! code and moves the report and the ledger to the lesson's new numbering.
use super::claim_ledger::ClaimLedger;
use super::repair_work::rejected;
use super::*;
use crate::dto::LessonQuestionKind;
use crate::generation::{parse_json, split_check, teaching_sentences};
use crate::lessons::text_edits;
use lattice_core::domain::learning::claim_review::ClaimId;
use std::collections::{BTreeMap, BTreeSet};

/// What the flow repair is told besides the edit protocol.
const FLOW: &str = "Sentences were taken out of this lesson section because they were shown to be wrong. Restore the flow of what is left: fix a word that now points at nothing (\"this\", \"these\", \"as above\", a step number) and join the sentences on either side of a gap if they no longer follow, changing as little as you can. gaps lists, for each place a sentence was taken out, the sentence kept before it and the one kept after it: change only those sentences, or add a short connecting sentence between them. verifiedSentences are checked and must stay exactly as they are; connect around them. Add no statement of fact: no new claim, figure, name, quotation, cause or consequence. If the section reads well as it is, give no edits.";

/// What a lesson is told when the cuts leave it no statement: the part of it
/// that would have to be taken out.
pub(super) const NOTHING_LEFT: &str = "every statement in it would have to be taken out";

/// The sentences kept on either side of one place a cut took text out of.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Gap {
    pub(super) field: String,
    pub(super) before: Option<String>,
    pub(super) after: Option<String>,
}

/// A section that took cuts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Taken {
    /// Its title, or a question's prompt: the record is of this section.
    pub(super) label: String,
    pub(super) gaps: Vec<Gap>,
    /// A cut took the answer of a check.
    #[serde(default)]
    pub(super) answer_lost: bool,
}

/// A section's flow repair, kept until a pass has shown whether what it wrote
/// states anything.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Mended {
    pub(super) label: String,
    /// The section as it stood before the repair.
    pub(super) before: Value,
    /// The claims standing in it then.
    pub(super) known: Vec<ClaimId>,
    #[serde(default)]
    pub(super) checked: bool,
}

/// What the cuts of one lesson left, by unit.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(super) struct Cuts {
    pub(super) taken: BTreeMap<usize, Taken>,
    /// The sections that go, with the title or prompt of each.
    pub(super) going: BTreeMap<usize, String>,
    pub(super) mended: BTreeMap<usize, Mended>,
    #[serde(default)]
    pub(crate) reviewed_without_going: Option<String>,
}

impl Cuts {
    pub(super) fn units(&self) -> Going {
        Going(self.going.keys().copied().collect())
    }

    /// Drops what was recorded of a unit that is no longer the section it was
    /// recorded for.
    pub(super) fn keep_current(&mut self, lesson: &Value) {
        let same = |unit: &usize, label: &str| unit_label(lesson, *unit).as_deref() == Some(label);
        self.taken.retain(|unit, taken| same(unit, &taken.label));
        self.going.retain(|unit, label| same(unit, label));
        self.mended.retain(|unit, mended| same(unit, &mended.label));
    }

    pub(super) fn take(
        &mut self,
        unit: usize,
        label: String,
        (gaps, answer_lost): (Vec<Gap>, bool),
    ) {
        let taken = self.taken.entry(unit).or_insert_with(|| Taken {
            label,
            ..Taken::default()
        });
        for gap in gaps {
            if !taken.gaps.contains(&gap) {
                taken.gaps.push(gap);
            }
        }
        taken.answer_lost |= answer_lost;
    }

    pub(super) fn go(&mut self, unit: usize, label: String) {
        self.going.insert(unit, label);
    }
}

/// The units that go out of a lesson, numbered as the lesson is: sections
/// first, then questions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Going(BTreeSet<usize>);

impl Going {
    #[cfg(test)]
    pub(super) fn new(units: impl IntoIterator<Item = usize>) -> Self {
        Self(units.into_iter().collect())
    }

    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(super) fn contains(&self, unit: usize) -> bool {
        self.0.contains(&unit)
    }

    /// Where `unit` is once the sections that go are out; `None` for one that
    /// goes.
    pub(super) fn after(&self, unit: usize) -> Option<usize> {
        (!self.0.contains(&unit)).then(|| unit.saturating_sub(self.0.range(..unit).count()))
    }

    /// `lesson` without them.
    pub(super) fn out_of(&self, lesson: &Value) -> Value {
        let mut out = lesson.clone();
        let blocks = lesson
            .get("blocks")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        for (list, first) in [("blocks", 0), ("questions", blocks)] {
            if let Some(items) = out.get_mut(list).and_then(Value::as_array_mut) {
                let mut unit = first;
                items.retain(|_| {
                    let stays = !self.0.contains(&unit);
                    unit += 1;
                    stays
                });
            }
        }
        out
    }

    /// `whole` with the sections that stay replaced by those of `reviewed`,
    /// which was `whole` without the ones that go; `None` when `reviewed` has
    /// a different shape.
    pub(super) fn back_in(&self, whole: &Value, reviewed: &Value) -> Option<Value> {
        let mut merged = whole.clone();
        let blocks = whole.get("blocks")?.as_array()?.len();
        for (list, first) in [("blocks", 0), ("questions", blocks)] {
            let items = whole.get(list)?.as_array()?.len();
            let sent = reviewed.get(list)?.as_array()?;
            let staying: Vec<usize> = (0..items)
                .filter(|at| !self.0.contains(&(first + at)))
                .collect();
            if sent.len() != staying.len() {
                return None;
            }
            let slots = merged.get_mut(list)?.as_array_mut()?;
            for (at, item) in staying.into_iter().zip(sent) {
                *slots.get_mut(at)? = item.clone();
            }
        }
        Some(merged)
    }
}

impl LessonVerificationReport {
    /// The report of the lesson without the sections that go, its units
    /// numbered as the lesson now is.
    pub(super) fn without(mut self, going: &Going) -> Self {
        self.coverage = std::mem::take(&mut self.coverage)
            .into_iter()
            .filter_map(|mut unit| {
                unit.index = going.after(unit.index)?;
                Some(unit)
            })
            .collect();
        self.coverage_audit = std::mem::take(&mut self.coverage_audit)
            .into_iter()
            .filter_map(|mut unit| {
                unit.index = going.after(unit.index)?;
                Some(unit)
            })
            .collect();
        self.findings = std::mem::take(&mut self.findings)
            .into_iter()
            .filter_map(|mut finding| {
                finding.unit = going.after(finding.unit)?;
                Some(finding)
            })
            .collect();
        self.executions = std::mem::take(&mut self.executions)
            .into_iter()
            .filter_map(|mut run| {
                if let Some(unit) = run.unit {
                    run.unit = Some(going.after(unit)?);
                }
                Some(run)
            })
            .collect();
        self
    }
}

/// The pointer to a unit of a lesson: a section, or a question after them.
fn unit_pointer(lesson: &Value, unit: usize) -> Option<String> {
    let blocks = lesson.get("blocks")?.as_array()?.len();
    Some(if unit < blocks {
        format!("/blocks/{unit}")
    } else {
        format!("/questions/{}", unit - blocks)
    })
}

fn unit_at(lesson: &Value, unit: usize) -> Option<&Value> {
    lesson.pointer(&unit_pointer(lesson, unit)?)
}

fn set_unit(lesson: &mut Value, unit: usize, section: Value) -> bool {
    let Some(pointer) = unit_pointer(lesson, unit) else {
        return false;
    };
    match lesson.pointer_mut(&pointer) {
        Some(slot) => {
            *slot = section;
            true
        }
        None => false,
    }
}

/// A section's title, or a question's prompt: what a record of it is checked
/// against.
pub(super) fn unit_label(lesson: &Value, unit: usize) -> Option<String> {
    let blocks = lesson.get("blocks")?.as_array()?.len();
    let field = if unit < blocks { "title" } else { "prompt" };
    unit_at(lesson, unit)?
        .get(field)?
        .as_str()
        .map(str::to_owned)
}

/// The sections `cuts` leaves with too little to teach in `lesson`, whatever
/// else of it could be published.
pub(super) fn left_with_too_little(lesson: &Value, cuts: &Cuts) -> Vec<usize> {
    let mut cuts = cuts.clone();
    cuts.keep_current(lesson);
    too_little(lesson, &cuts)
}

/// Puts `units` of `lesson` back as they are in `from`, and what `cuts` took
/// from them back as it is in `earlier`.
pub(super) fn restore_units(
    (lesson, cuts): (&mut Value, &mut Cuts),
    (from, earlier): (&Value, &Cuts),
    units: &[usize],
) {
    for unit in units {
        if let Some(section) = unit_at(from, *unit).cloned() {
            set_unit(lesson, *unit, section);
        }
        match earlier.taken.get(unit) {
            Some(taken) => cuts.taken.insert(*unit, taken.clone()),
            None => cuts.taken.remove(unit),
        };
    }
}

/// The sections that took cuts and are left with too little to teach: fewer
/// than two teaching sentences, or a check whose answer went.
fn too_little(lesson: &Value, cuts: &Cuts) -> Vec<usize> {
    let blocks = lesson
        .get("blocks")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    cuts.taken
        .iter()
        .filter(|(unit, _)| **unit < blocks)
        .filter(|(unit, taken)| {
            taken.answer_lost
                || lesson
                    .pointer(&format!("/blocks/{unit}/body"))
                    .and_then(Value::as_str)
                    .is_none_or(|body| teaching_sentences(body).len() < 2)
        })
        .map(|(unit, _)| *unit)
        .collect()
}

/// Why `lesson`, with the sections `cuts` takes out and those left with too
/// little, could not be published, when it could not.
pub(super) fn unpublishable(lesson: &Value, cuts: &Cuts) -> Option<String> {
    let mut cuts = cuts.clone();
    cuts.keep_current(lesson);
    for unit in too_little(lesson, &cuts) {
        if let Some(label) = unit_label(lesson, unit) {
            cuts.go(unit, label);
        }
    }
    thin(lesson, &cuts.units())
}

/// Why a lesson that loses the sections that go cannot be published, when it
/// cannot: more than a third of its sections, its first explanation, all its
/// questions or every question of a kind it asks, without which it cannot be
/// saved.
fn thin(lesson: &Value, going: &Going) -> Option<String> {
    let blocks = lesson.get("blocks")?.as_array()?;
    let sections = blocks.len();
    let lost = (0..sections).filter(|unit| going.contains(*unit)).count();
    let first = blocks
        .iter()
        .position(|block| block.get("kind").and_then(Value::as_str) == Some("explanation"));
    let questions = lesson
        .get("questions")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let questions_lost = (sections..sections + questions)
        .filter(|unit| going.contains(*unit))
        .count();
    let asks = |unit: usize, kind: &LessonQuestionKind| {
        lesson
            .pointer(&format!("/questions/{}/kind", unit - sections))
            .and_then(Value::as_str)
            == Some(kind.as_str())
    };
    let kind_lost = LessonQuestionKind::ALL.into_iter().find(|kind| {
        let asked: Vec<usize> = (sections..sections + questions)
            .filter(|unit| asks(*unit, kind))
            .collect();
        !asked.is_empty() && asked.iter().all(|unit| going.contains(*unit))
    });
    if first.is_some_and(|unit| going.contains(unit)) {
        Some("its first explanation would have to be taken out".to_owned())
    } else if 3 * lost > sections {
        Some(format!(
            "{lost} of its {sections} sections would have to be taken out"
        ))
    } else if questions > 0 && questions_lost == questions {
        Some("all of its questions would have to be taken out".to_owned())
    } else {
        kind_lost.map(|kind| format!("it would be left without a {} question", kind.as_str()))
    }
}

/// Whether a cut took the answer of a check: the first teaching sentence under
/// its reveal line.
pub(super) fn answer_lost(before: &str, after: &str) -> bool {
    let answer = |text: &str| {
        split_check(text).and_then(|(_, reveal)| teaching_sentences(reveal).into_iter().next())
    };
    let had = answer(before);
    had.is_some() && had != answer(after)
}

/// The places `after` has lost teaching sentences of `before` at, each with
/// the sentences kept beside it.
pub(super) fn gaps(field: &str, before: &str, after: &str) -> Vec<Gap> {
    let (old, new) = (teaching_sentences(before), teaching_sentences(after));
    let mut kept = 0;
    let stays: Vec<bool> = old
        .iter()
        .map(|sentence| {
            let same = new.get(kept) == Some(sentence);
            if same {
                kept += 1;
            }
            same
        })
        .collect();
    let mut found = Vec::new();
    let mut at = 0;
    while at < old.len() {
        if stays.get(at) == Some(&true) {
            at += 1;
            continue;
        }
        let end = (at..old.len())
            .find(|next| stays.get(*next) == Some(&true))
            .unwrap_or(old.len());
        found.push(Gap {
            field: field.to_owned(),
            before: at.checked_sub(1).and_then(|k| old.get(k)).cloned(),
            after: old.get(end).cloned(),
        });
        at = end;
    }
    found
}

/// The place `after` lost a code block `before` held at, with the sentences
/// kept beside it.
pub(super) fn gap_around_code(field: &str, before: &str, after: &str) -> Vec<Gap> {
    let shared: usize = before
        .chars()
        .zip(after.chars())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum();
    let before_it = before
        .get(..shared)
        .and_then(|head| teaching_sentences(head).pop());
    let after_it = after
        .get(shared..)
        .and_then(|tail| teaching_sentences(tail).into_iter().next());
    if before_it.is_none() && after_it.is_none() {
        return Vec::new();
    }
    vec![Gap {
        field: field.to_owned(),
        before: before_it,
        after: after_it,
    }]
}

/// One section's flow repair: the section with the edits the model gave for
/// its gaps, or `None` when it gave none that can stand. A change to a
/// sentence a claim stands on is no repair, and neither is one outside the
/// sentences beside a gap, which is taken back.
async fn mend_section(
    llm: &dyn LLMPort,
    (schema, output_tokens): (&Value, usize),
    (unit, block, section): (usize, bool, &Value),
    (taken, protected): (&Taken, &[Protected]),
) -> Result<Option<Value>> {
    let stands = |gap: &Gap, sentence: &Option<String>| -> Option<String> {
        let text = section.pointer(&format!("/{}", gap.field))?.as_str()?;
        sentence
            .clone()
            .filter(|sentence| text.contains(sentence.as_str()))
    };
    let gaps: Vec<Gap> = taken
        .gaps
        .iter()
        .filter_map(|gap| {
            let (before, after) = (stands(gap, &gap.before), stands(gap, &gap.after));
            (before.is_some() || after.is_some()).then(|| Gap {
                field: gap.field.clone(),
                before,
                after,
            })
        })
        .collect();
    if gaps.is_empty() {
        return Ok(None);
    }
    let mut targets = Vec::new();
    for gap in &gaps {
        for quote in [&gap.before, &gap.after].into_iter().flatten() {
            targets.push(Protected {
                unit,
                field: gap.field.clone(),
                quote: quote.clone(),
                released: false,
            });
        }
    }
    let verified: Vec<&str> = protected.iter().map(|item| item.quote.as_str()).collect();
    let local = json!({"section": section, "gaps": gaps, "verifiedSentences": verified});
    let (list, original) = if block {
        ("blocks", json!({"blocks":[section],"questions":[]}))
    } else {
        ("questions", json!({"blocks":[],"questions":[section]}))
    };
    let system = format!("{FLOW} {}", text_edits::INSTRUCTIONS);
    let raw = {
        let _model_call =
            crate::lesson_progress::model_call(crate::lesson_progress::Term::FlowRepair);
        crate::generation::complete_json(
            llm,
            &system,
            local.to_string(),
            text_edits::schema_for(&original, schema),
            output_tokens,
        )
        .await?
    };
    let reply: Value = parse_json(&raw)?;
    let asked = |key: &str| {
        reply
            .get(key)
            .and_then(Value::as_array)
            .is_some_and(|items| !items.is_empty())
    };
    if !asked("edits") && !asked("citations") {
        return Ok(None);
    }
    let repaired = text_edits::apply(&original, &raw)?;
    let pointer = format!("/{list}/0");
    let (Some(before), Some(after)) = (original.pointer(&pointer), repaired.pointer(&pointer))
    else {
        return Ok(None);
    };
    let confined = confine(before, after, &targets, protected)?.section;
    Ok((confined != *before).then_some(confined))
}

/// Whether a flow repair failed in what the model answered, so that the
/// section keeps its text as cut, rather than in an outage or a cancellation.
fn unusable(error: &AppError) -> bool {
    rejected(error) || matches!(error, AppError::ModelAnswerUnusable(_))
}

/// What the lesson needs once nothing is left for the checks.
pub(super) enum Settled {
    /// The draft changed, or the sections that go changed: verify it again.
    Again(String),
    /// Nothing is left to do: the sections that go can be taken out.
    Done,
    /// The lesson is not published.
    Stopped(AppError),
}

/// The step the loop takes when every claim is resolved (claim classes design
/// §3.10): sections left with too little go, a lesson that loses too much,
/// or every statement it had, stops, every section that took a cut gets its
/// one flow repair, and the teaching review reads the lesson as it will be.
pub(super) async fn settle(
    llm: &dyn LLMPort,
    (prompt, schema, output_tokens): (&str, &Value, usize),
    references: &ReferenceCollection<'_>,
    raw: &str,
    ledger: &mut ClaimLedger,
) -> Result<Settled> {
    let lesson: Value = parse_json(raw)?;
    ledger.cuts.keep_current(&lesson);
    let mut grew = false;
    for unit in too_little(&lesson, &ledger.cuts) {
        if let (false, Some(label)) = (
            ledger.cuts.going.contains_key(&unit),
            unit_label(&lesson, unit),
        ) {
            ledger.cuts.go(unit, label);
            grew = true;
        }
    }
    if grew {
        ledger.save_cuts().await?;
        return Ok(Settled::Again(raw.to_owned()));
    }
    let going = ledger.cuts.units();
    if let Some(what) = thin(&lesson, &going) {
        return Ok(Settled::Stopped(ledger.stop_unsupported(raw, &what).await?));
    }
    if ledger.nothing_left(&[]) {
        return Ok(Settled::Stopped(
            ledger.stop_unsupported(raw, NOTHING_LEFT).await?,
        ));
    }
    let mendable: Vec<usize> = ledger
        .cuts
        .taken
        .keys()
        .copied()
        .filter(|unit| !going.contains(*unit))
        .collect();
    crate::lesson_progress::cuts_known(mendable.len());
    let todo: Vec<usize> = mendable
        .iter()
        .copied()
        .filter(|unit| !ledger.cuts.mended.contains_key(unit))
        .collect();
    let total = todo.len();
    let blocks = lesson
        .get("blocks")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let mut edited = lesson.clone();
    for (position, unit) in todo.into_iter().enumerate() {
        crate::lesson_progress::phase(
            crate::lesson_progress::Phase::Repair,
            format!(
                "Restoring the flow of section {} of {total} after the cuts",
                position + 1
            ),
        );
        let (Some(label), Some(section), Some(taken)) = (
            unit_label(&edited, unit),
            unit_at(&edited, unit).cloned(),
            ledger.cuts.taken.get(&unit).cloned(),
        ) else {
            continue;
        };
        let protected: Vec<Protected> = Protected::of_ledger(ledger, &[])
            .into_iter()
            .filter(|item| item.unit == unit)
            .collect();
        let known = ledger.live_in(unit);
        let outcome = match mend_section(
            llm,
            (schema, output_tokens),
            (unit, unit < blocks, &section),
            (&taken, &protected),
        )
        .await
        {
            Ok(outcome) => outcome,
            Err(error) if unusable(&error) => {
                tracing::info!(unit, %error, "claim.state: a flow repair could not be used; the section keeps the text as cut");
                None
            }
            Err(error) => return Err(error),
        };
        let unchanged = outcome.is_none();
        if let Some(repaired) = outcome {
            set_unit(&mut edited, unit, repaired);
            crate::lesson_drafts::save(&edited.to_string()).await?;
        }
        ledger.cuts.mended.insert(
            unit,
            Mended {
                label,
                before: section,
                known,
                checked: unchanged,
            },
        );
        crate::lesson_progress::flow_repaired();
        ledger.save_cuts().await?;
    }
    let without_going = going.out_of(&edited);
    let without_going_digest = digest(&without_going.to_string());
    let needs_going_review = !going.is_empty()
        && ledger.cuts.reviewed_without_going.as_deref() != Some(without_going_digest.as_str());
    if edited != lesson || needs_going_review {
        // The review reads the lesson as it will be, and the others' text it
        // changes comes back to the sections' places.
        let protected: Vec<Protected> = Protected::of_ledger(ledger, &[])
            .into_iter()
            .filter_map(|item| {
                let unit = going.after(item.unit)?;
                Some(Protected { unit, ..item })
            })
            .collect();
        let reviewed = crate::lesson_progress::outside_checks(crate::teaching::review_lesson(
            llm,
            crate::teaching::LESSON_REPAIR_SYSTEM,
            prompt,
            schema,
            without_going.to_string(),
            output_tokens,
            references,
            &protected,
        ))
        .await?;
        ledger.cuts.reviewed_without_going = Some(digest(&reviewed));
        ledger.save_cuts().await?;
        let reviewed: Value = parse_json(&reviewed)?;
        if let Some(merged) = going.back_in(&edited, &reviewed) {
            edited = merged;
        }
        crate::lesson_drafts::save(&edited.to_string()).await?;
    }
    if edited != lesson {
        return Ok(Settled::Again(edited.to_string()));
    }
    Ok(Settled::Done)
}

/// After a pass over a draft with flow repairs in it: a repair whose section
/// now holds a statement that was not there before is taken back, the section
/// keeping its text as cut (claim classes design §3.10). `Some` is the draft
/// with those sections as they were.
pub(super) async fn unmend(ledger: &mut ClaimLedger, candidate: &Value) -> Result<Option<String>> {
    let waiting: Vec<usize> = ledger
        .cuts
        .mended
        .iter()
        .filter(|(_, mended)| !mended.checked)
        .map(|(unit, _)| *unit)
        .collect();
    if waiting.is_empty() {
        return Ok(None);
    }
    let mut restored = candidate.clone();
    let mut taken_back = false;
    for unit in waiting {
        let Some(mended) = ledger.cuts.mended.get(&unit).cloned() else {
            continue;
        };
        let fresh = ledger
            .live_in(unit)
            .iter()
            .any(|claim| !mended.known.contains(claim));
        if fresh {
            taken_back |= set_unit(&mut restored, unit, mended.before);
        }
        if let Some(entry) = ledger.cuts.mended.get_mut(&unit) {
            entry.checked = true;
        }
    }
    ledger.save_cuts().await?;
    Ok(taken_back.then(|| restored.to_string()))
}

/// The lesson without the sections that go, with its report, its ledger and
/// its saved inventories moved to the lesson's new numbering; as it was when
/// none goes.
pub(super) async fn excise(
    llm: &dyn LLMPort,
    ledger: &mut ClaimLedger,
    raw: String,
    candidate: Value,
    report: LessonVerificationReport,
) -> Result<(String, Value, LessonVerificationReport)> {
    let going = ledger.cuts.units();
    if going.is_empty() {
        return Ok((raw, candidate, report));
    }
    let units: usize = ["blocks", "questions"]
        .iter()
        .filter_map(|list| candidate.get(*list).and_then(Value::as_array))
        .map(Vec::len)
        .sum();
    let lesson = going.out_of(&candidate);
    ledger.keep_findings(&report, &going);
    ledger.relocate(&going).await?;
    super::inventory::revisions::renumber(llm, units, |unit| going.after(unit)).await?;
    let raw = lesson.to_string();
    crate::lesson_drafts::save(&raw).await?;
    ledger.clear_cuts().await?;
    Ok((raw, lesson, report.without(&going)))
}

#[cfg(test)]
#[path = "sections_tests.rs"]
mod tests;
