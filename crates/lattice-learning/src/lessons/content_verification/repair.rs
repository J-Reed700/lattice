//! Rewrites only affected sections, one request for each section's failed
//! sentences, every section durable. A claim a page of the course supported
//! elsewhere needs only its citation rewritten (`citation::cite_where_supported`),
//! with no model call.
use super::claim_ledger::ClaimLedger;
use super::repair_work::{
    claim_ids, context_claims, drop_group, ledger_targets, section_examples, unmade,
    withdraw_resolved,
};
use super::*;
use crate::dto::LearningSourceDto;
use crate::lessons::copied_wording::Wording;
use crate::lessons::rows::{self, words, Held, Left, Quotes, Rows};
use crate::lessons::shape::LessonShape;
use crate::lessons::structured_call::{refusal, Asking, CallKind, ReasonFirst};
use crate::lessons::text_edits::{
    self, NEVER_SAY_WHERE, QUOTED_WORDING, STEPS_IN_ORDER, UNQUOTED_REMARKS,
};
use crate::lessons::trace::{self, Cause, Stage};
use crate::review_findings::FindingSite;
use lattice_core::application::services::model_contract::ask::{self, ask_each, Settled};
use lattice_core::application::services::model_contract::packed::Packed;
use lattice_core::application::services::model_contract::shape::Why;
use lattice_core::application::services::model_contract::shown::{Internal, Scope, Told};
use lattice_core::domain::learning::acceptance::{AcceptancePolicy, Goal};
use std::collections::BTreeSet;

mod citation;
mod confine;
mod contract;
mod cut;
mod taking;
pub(super) use citation::{cite_where_supported, number_evidence, uncite, Cited};
#[cfg(test)]
pub(super) use citation::{quotation, Unwritten};
pub(crate) use confine::{confine, Protected};
use contract::{narrowed, quotes_of, units_of, CONFINEMENT, ENTRIES, NO_CHANGE, NO_ROOM};
pub(super) use contract::{Claim, Correction, Failed, Input, Passage, SectionRepair};
#[cfg(test)]
pub(super) use cut::last_attempts;
use taking::{section_input, Taking};

fn section_of(candidate: &Value, unit: usize) -> Result<(&'static str, usize)> {
    FindingSite::in_candidate(candidate, unit)
        .array()
        .ok_or_else(|| invalid("Repair referred to an unknown lesson section."))
}

fn observations(context: &Value) -> Result<Vec<execution::Observation>> {
    Ok(serde_json::from_value(
        context
            .get("executions")
            .cloned()
            .unwrap_or_else(|| json!([])),
    )?)
}

fn targets(context: &Value) -> Result<BTreeSet<usize>> {
    let mut targets = BTreeSet::new();
    for finding in context
        .get("failedClaims")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        targets.insert(
            finding
                .get("unit")
                .and_then(Value::as_u64)
                .ok_or_else(|| invalid("A repair finding has no section."))? as usize,
        );
    }
    for example in context
        .get("exampleClaims")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        targets.insert(
            example
                .get("unit")
                .and_then(Value::as_u64)
                .ok_or_else(|| invalid("A failed example has no repairable section."))?
                as usize,
        );
    }
    Ok(targets)
}

/// The failed claims recorded for `unit`, in recorded order.
fn section_failures(context: &Value, unit: usize) -> Vec<Value> {
    context
        .get("failedClaims")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|finding| finding.get("unit").and_then(Value::as_u64) == Some(unit as u64))
        .cloned()
        .collect()
}

/// A failed claim's evidence IDs in recorded order, the cited one first.
fn evidence_ids(failure: &Value) -> Vec<&str> {
    failure
        .get("evidence")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}

/// The section `rows` are of, as a request that shows it alone shows it: without its pointer.
pub(super) fn shown_section(rows: &Rows, shown: &rows::ShownRows<'_>, part: &str) -> Value {
    let mut section = rows
        .rendered(shown)
        .pointer(part)
        .cloned()
        .unwrap_or(Value::Null);
    if let Some(fields) = section.as_object_mut() {
        fields.shift_remove("part");
    }
    section
}

/// The material a request shows: the lesson with every part but `part` emptied.
fn only_section(candidate: &Value, part: &str) -> Value {
    let mut out = serde_json::Map::new();
    for list in ["blocks", "questions"] {
        let items: Vec<Value> = candidate
            .get(list)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(index, item)| {
                if format!("/{list}/{index}") == part {
                    item.clone()
                } else {
                    Value::Null
                }
            })
            .collect();
        out.insert(list.to_owned(), Value::Array(items));
    }
    Value::Object(out)
}

/// What a correction toward `goal` is told its sentence must do: the reading
/// that follows tests exactly that (acceptance design §2.4).
pub(super) fn goal_words(goal: Goal) -> &'static str {
    match goal {
        Goal::WhatThePassageStates => "Say only what the claim's evidence passages state. Where no passage states a detail, keep the sentence's point and leave that detail out.",
        Goal::AgreeWithThePassages => "Make the sentence agree with its evidence passages, correcting what the reason names.",
        Goal::AsTheFieldHasIt => "State it as the field generally has it, correcting what the reason names.",
        Goal::CorrectWhatWasFound => "Correct what the reason found wrong in the sentence, and keep the rest of the sentence as it is.",
        Goal::KeepTheOthers => "Take this claim out of the sentence and keep each statement in alsoStates, said as it is now.",
    }
}

const OPENING: &str = "Repair verified lesson defects. Correct the listed factual defects using the recorded evidence and execution results. Preserve valid teaching, practice demands and verbatim citations. Each failed claim lists the handles of its evidence passages. repairHistory lists sentences earlier corrections replaced or removed because a check failed them; do not reintroduce what they asserted, or paraphrases of it. A failed claim's alsoStates lists other statements its sentence makes that the lesson keeps: write the corrected sentence so that each of them is still said, and true. Each failed claim's goal says what its corrected sentence must do; the check that follows tests exactly that. A failed claim with notApplied was sent before and its correction could not be applied, for the reason given: correct it so that it can be. A failed claim with groundReason was refuted by a strict reading whose reason is the one given, and its evidence passages did not address it (groundReason): correct what that reason found wrong and keep the rest of the sentence as it is, without searching the passages for support. When evidence cannot support a detail, remove that detail cleanly instead of replacing it with another unsupported explanation. Label Markdown code/output fences. Read lessonSections alongside candidate so every correction agrees with the rest of the lesson: never introduce a claim that contradicts another section, contrast, or unflagged statement in the lesson. Never extrapolate a general protocol rule, field meaning, or architecture-wide mechanism from an unlabelled sample payload or a single product's internal feature unless an evidence passage explicitly states that rule; when passages only show a partial case or omit a detail, leave out only the unsupported detail rather than substituting a false or scope-widened rule.";

/// The instructions of a section repair, with the rules of citing when the lesson takes citations.
pub(super) fn system(question: bool, cites: bool) -> String {
    let mut parts = vec![
        OPENING.to_owned(),
        CONFINEMENT.to_owned(),
        ENTRIES.to_owned(),
        lattice_core::shared::markdown_math::MATH_GUIDANCE.to_owned(),
        words::SENTENCES.to_owned(),
    ];
    if question {
        parts.push(words::KEYS.to_owned());
    } else {
        parts.push(words::NO_KEYS.to_owned());
    }
    parts.push(words::EDITING.to_owned());
    if cites {
        parts.push(words::citing_evidence());
        parts.extend(
            [
                NEVER_SAY_WHERE,
                QUOTED_WORDING,
                UNQUOTED_REMARKS,
                STEPS_IN_ORDER,
            ]
            .map(str::to_owned),
        );
    } else {
        parts.push(words::NO_CITATIONS.to_owned());
    }
    parts.join(" ")
}

/// The sentences the teaching review after a claim repair or a user edit leaves
/// as they are: every live claim's but the `targets`', except those of the
/// machine's claims with rewrites left, which it may repair and which are
/// recorded as waiting for that (claim sources design §10.3).
pub(super) async fn teaching_protected(
    ledger: &mut ClaimLedger,
    policy: &dyn AcceptancePolicy,
    candidate: &Value,
    targets: &[ClaimId],
) -> Result<Vec<Protected>> {
    let released = ledger.releasable(policy, targets);
    ledger.begin_teaching(policy, candidate, &released).await?;
    Ok(Protected::releasing(ledger, targets, &released))
}

pub(crate) async fn candidate(
    llm: &dyn LLMPort,
    policy: &dyn AcceptancePolicy,
    prompt: &str,
    shape: LessonShape<'_>,
    mut context: Value,
    output_tokens: usize,
    (references, numbered): (&ReferenceCollection<'_>, &[LearningSourceDto]),
    ledger: &mut ClaimLedger,
) -> Result<String> {
    let sent = context_claims(&context);
    // A claim on its last attempt has its sentence taken out in code; the
    // model is asked only for the others, and for a statement whose cut is a
    // narrowing (`ClaimLedger::cut_of`).
    let before_cuts = context
        .get("candidate")
        .map(|lesson| lesson.to_string())
        .unwrap_or_default();
    let handled = cut::last_attempts(policy, shape.policy, &mut context, ledger).await?;
    if handled > 0 {
        if let Some(after) = context.get("candidate") {
            trace::changed(
                Stage::ClaimRepair,
                &Cause::none().claims(&sent),
                &before_cuts,
                &after.to_string(),
            );
        }
    }
    let candidate = if handled > 0 && targets(&context)?.is_empty() {
        let candidate = context
            .get("candidate")
            .cloned()
            .ok_or_else(|| invalid("Missing repair candidate."))?;
        crate::lesson_drafts::save_repair(&candidate.to_string(), None).await?;
        candidate
    } else {
        repair_sections(
            llm,
            policy,
            prompt,
            shape.value(),
            context,
            output_tokens,
            references,
            ledger,
        )
        .await?
    };
    // What this repair left as it was: the teaching review after it leaves
    // it too (claim sources design §10.3), as the synced ledger has it now.
    let protected = teaching_protected(ledger, policy, &candidate, &sent).await?;
    crate::lesson_progress::outside_checks(crate::teaching::review_lesson(
        llm,
        crate::teaching::LESSON_REPAIR_SYSTEM,
        prompt,
        shape,
        candidate.to_string(),
        output_tokens,
        (references, numbered),
        &protected,
    ))
    .await
}

/// Repairs every affected section in turn, one request for all of its failed
/// sentences, each on the section as the previous left it, saving every
/// section. A claim the user resolved meanwhile is not sent (§9); an entry the
/// model leaves unusable, or that changes no sentence it was asked to change,
/// costs its own claims an attempt and leaves the section as it was.
async fn repair_sections(
    llm: &dyn LLMPort,
    policy: &dyn AcceptancePolicy,
    prompt: &str,
    schema: &Value,
    mut context: Value,
    output_tokens: usize,
    references: &ReferenceCollection<'_>,
    ledger: &mut ClaimLedger,
) -> Result<Value> {
    let pending = targets(&context)?;
    if pending.is_empty() {
        return Err(invalid("No affected sections were recorded for repair."));
    }
    let wording = Wording::of(references);
    let cites = text_edits::takes_citations(schema);
    let options = crate::model_call::answer_options();
    let mut candidate = context
        .get("candidate")
        .cloned()
        .ok_or_else(|| invalid("Missing repair candidate."))?;
    for (position, unit) in pending.iter().copied().enumerate() {
        let (field, index) = section_of(&candidate, unit)?;
        let part = format!("/{field}/{index}");
        ledger.sync().await?;
        let dropped = withdraw_resolved(&mut context, ledger, policy);
        if !dropped.is_empty() {
            ledger.supersede(&dropped).await?;
        }
        let failures = section_failures(&context, unit);
        let examples = section_examples(&context, unit);
        if failures.is_empty() && examples.is_empty() {
            continue;
        }
        let mut claims = claim_ids(&failures);
        claims.extend(claim_ids(&examples));
        let input = section_input(
            llm,
            ledger,
            (&context, &candidate),
            (unit, &part),
            (&wording, cites, output_tokens),
        )?;
        let bare = Input {
            items: Vec::new(),
            ..input.clone()
        };
        if ask::room::<SectionRepair>(llm, &bare, &options)? == 0 {
            let error = crate::model_call::window_too_small(llm, "A section repair does not fit");
            ledger.reject(policy, &claims, &error.to_string()).await?;
            continue;
        }
        let own = Rows::of_part(&candidate, &part);
        for item in &input.items {
            if units_of(&own, &part, item.targets.value()).is_empty() {
                tracing::warn!(
                    unit,
                    claim = item.id.as_str(),
                    "A failed claim's sentence was not found in its section; its correction cannot be applied"
                );
            }
        }
        crate::lesson_progress::phase(
            crate::lesson_progress::Phase::Repair,
            format!(
                "Correcting affected section {} of {}",
                position + 1,
                pending.len()
            ),
        );
        let answered = {
            let _model_call =
                crate::lesson_progress::model_call(crate::lesson_progress::Term::Rewrite);
            ask_each::<SectionRepair>(
                llm,
                &Asking {
                    progress: None,
                    reasoning: Some(ReasonFirst {
                        kind: CallKind::Authoring,
                        subject: "the section correction",
                        brief: "Review each failed statement and the allowed references, choose the exact unit ids to edit or drop without touching verified claims or editing any unit twice, and write out the corrected wording.",
                    }),
                },
                &mut Scope::for_question(),
                &input.items,
                |open, _| Input {
                    items: open.iter().map(|item| (*item).clone()).collect(),
                    ..input.clone()
                },
                &options,
            )
            .await
        };
        let before = candidate.to_string();
        let mut made: Vec<ClaimId> = Vec::new();
        match answered {
            Err(error) if !unmade(&error) => return Err(error),
            Err(error) => {
                let sent: Vec<ClaimId> = input.items.iter().flat_map(Failed::claim_ids).collect();
                ledger.reject(policy, &sent, &error.to_string()).await?;
            }
            Ok(settled) => {
                let mut taking = Taking {
                    input: &input,
                    schema,
                    field,
                    quotes: quotes_of(&input),
                    rows: Rows::of_part(&candidate, &part),
                };
                for (item, settled) in input.items.iter().zip(settled) {
                    match taking.settle(item, settled) {
                        Ok(()) => made.extend(item.claim_ids()),
                        Err(reason) => {
                            ledger.reject(policy, &item.claim_ids(), &reason).await?;
                        }
                    }
                }
                let repaired = taking.rows.value().pointer(&part).cloned();
                if let (Some(section), Some(slot)) = (repaired, candidate.pointer_mut(&part)) {
                    *slot = section;
                }
            }
        }
        if !made.is_empty() {
            trace::changed(
                Stage::ClaimRepair,
                &Cause::none().claims(&made),
                &before,
                &candidate.to_string(),
            );
        }
        let saved = candidate.to_string();
        drop_group(&mut context, unit, failures.len(), true);
        if let Some(object) = context.as_object_mut() {
            object.insert("candidate".into(), candidate.clone());
        }
        let checkpoint = PendingRepair {
            fingerprint: repair_fingerprint(llm, prompt, schema, &candidate, references),
            defects: ledger_targets(&context),
            context: context.clone(),
        };
        let remaining = if position + 1 < pending.len() {
            Some(serde_json::to_string(&checkpoint)?)
        } else {
            None
        };
        crate::lesson_drafts::save_repair(&saved, remaining).await?;
    }
    Ok(candidate)
}

#[cfg(test)]
#[path = "repair_tests.rs"]
mod tests;
