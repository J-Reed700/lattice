//! Rewrites only affected sections, each in as many requests as its failed
//! claims and evidence need to fit the model, every patch durable. A claim a
//! page of the course supported elsewhere needs only its citation rewritten
//! (`citation::cite_where_supported`), with no model call.
use super::claim_ledger::ClaimLedger;
use super::repair_work::{
    claim_ids, context_claims, drop_group, ledger_targets, section_examples, unmade,
    withdraw_resolved,
};
use super::*;
use crate::dto::LearningSourceDto;
use crate::lessons::copied_wording::Wording;
use crate::lessons::shape::LessonShape;
use crate::lessons::text_edits;
use crate::lessons::trace::{self, Cause, Stage};
use crate::review_findings::FindingSite;
use lattice_core::application::services::grounded_generation::GroundedRequest;
use lattice_core::application::services::model_contract::{
    Contract, ContractCall, ContractId, PromptInput,
};
use lattice_core::domain::learning::acceptance::{AcceptancePolicy, Goal};
use std::collections::{BTreeSet, VecDeque};

mod citation;
mod confine;
mod cut;
pub(super) use citation::{cite_where_supported, number_evidence, uncite, Cited};
#[cfg(test)]
pub(super) use citation::{quotation, Unwritten};
use confine::PutBack;
pub(crate) use confine::{confine, Protected};
#[cfg(test)]
pub(super) use cut::last_attempts;

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

/// The repair request for `failures`, some of one section's failed claims:
/// the section as it now stands, those claims, and their recorded evidence,
/// each passage once, in claim order.
fn section_context(context: &Value, unit: usize, failures: &[Value]) -> Result<Value> {
    let candidate = context
        .get("candidate")
        .ok_or_else(|| invalid("Missing repair candidate."))?;
    let (field, index) = section_of(candidate, unit)?;
    let section = candidate
        .get(field)
        .and_then(Value::as_array)
        .and_then(|items| items.get(index))
        .ok_or_else(|| invalid("Missing repair section."))?;
    let mut ids: Vec<&str> = Vec::new();
    for id in failures.iter().flat_map(evidence_ids) {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    let table = context.get("evidence").and_then(Value::as_array);
    let evidence = ids
        .iter()
        .map(|id| {
            table
                .into_iter()
                .flatten()
                .find(|passage| passage.get("id").and_then(Value::as_str) == Some(id))
                .cloned()
                .ok_or_else(|| invalid("A section repair is missing its recorded evidence."))
        })
        .collect::<Result<Vec<_>>>()?;
    let sections: Vec<_> = candidate
        .get("blocks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|block| json!({"kind":block.get("kind"),"title":block.get("title")}))
        .collect();
    let repair_history: Vec<_> = context
        .get("repairHistory")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|finding| finding.get("unit").and_then(Value::as_u64) == Some(unit as u64))
        .cloned()
        .collect();
    Ok(json!({
        "requirements":context.get("requirements"), "originalUnitIndex":unit,
        "lessonSections":sections,
        "candidate":{"blocks":if field == "blocks" {vec![section]} else {vec![]}, "questions":if field == "questions" {vec![section]} else {vec![]}},
        "failedClaims":failures,"evidence":evidence,
        "executions":observations(context)?.into_iter().filter(|o|o.unit==Some(unit)).collect::<Vec<_>>(),
        "repairHistory":repair_history,
    }))
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

fn repair_system(schema: &Value) -> String {
    format!("Repair verified lesson defects. Correct the listed factual defects using the recorded evidence and execution results. Preserve valid teaching, practice demands and verbatim citations. Each failed claim references IDs in the evidence table. repairHistory lists sentences earlier corrections replaced or removed because a check failed them; do not reintroduce what they asserted, or paraphrases of it. A failed claim's alsoStates lists other statements its sentence makes that the lesson keeps: write the corrected sentence so that each of them is still said, and true. Each failed claim's goal says what its corrected sentence must do; the check that follows tests exactly that. A failed claim with notApplied was sent before and its correction could not be applied, for the reason given: correct it so that it can be. A failed claim with groundReason was refuted by a strict reading whose reason is the one given, and its evidence passages did not address it (groundReason): correct what that reason found wrong and keep the rest of the sentence as it is, without searching the passages for support. When evidence cannot support a detail, remove that detail cleanly instead of replacing it with another unsupported explanation. Label Markdown code/output fences. {} {} {}", confine::CONFINEMENT, lattice_core::shared::markdown_math::MATH_GUIDANCE, text_edits::rewrite_instructions(schema))
}

/// A repair request as the window sees it. Its answer is reserved twice: an
/// answer the check refuses is carried, with this context, by its one
/// correction.
fn repair_request(system: &str, local: &Value, output_tokens: usize) -> GroundedRequest {
    let mut request = GroundedRequest::new(system, local.to_string());
    request.output_tokens = 2 * output_tokens;
    request
}

/// The ids of the passages a one-claim repair request `local` can carry, in
/// recorded order: as many as the window holds, measured where the backend
/// can count. Empty when none fits beside the section.
async fn fitted_passages(
    llm: &dyn LLMPort,
    system: &str,
    local: &Value,
    output_tokens: usize,
) -> Vec<String> {
    let passages: Vec<Value> = local
        .get("evidence")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let spent: Vec<usize> = passages
        .iter()
        .scan(0usize, |spent, passage| {
            *spent = spent.saturating_add(llm.count_tokens(&passage.to_string()) + 1);
            Some(*spent)
        })
        .collect();
    let held = |room: usize| {
        passages
            .get(..spent.partition_point(|total| *total <= room))
            .unwrap_or_default()
    };
    let build = |room: usize| {
        let kept = held(room);
        let mut local = local.clone();
        if let Some(claim) = local
            .pointer_mut("/failedClaims/0")
            .and_then(Value::as_object_mut)
        {
            let ids: Vec<Value> = kept
                .iter()
                .filter_map(|passage| passage.get("id").cloned())
                .collect();
            claim.insert("evidence".into(), Value::Array(ids));
        }
        if let Some(fields) = local.as_object_mut() {
            fields.insert("evidence".into(), json!(kept));
        }
        repair_request(system, &local, output_tokens)
    };
    let room = crate::model_call::source_room(llm, &build(0), usize::MAX);
    let fit = crate::model_call::fit_room(llm, room, build).await;
    if !fit.fits() {
        return Vec::new();
    }
    held(fit.room)
        .iter()
        .filter_map(|passage| Some(passage.get("id")?.as_str()?.to_owned()))
        .collect()
}

/// Split one section's failed claims into consecutive groups whose repair
/// request fits the model by estimate, each passage counted once per group.
/// A claim whose own evidence cannot fit even alone is a group of its own;
/// its passages are packed when it is sent. Only a section that leaves no
/// room for any claim is an error.
fn claim_groups(
    llm: &dyn LLMPort,
    system: &str,
    context: &Value,
    unit: usize,
    output_tokens: usize,
) -> Result<Vec<Vec<Value>>> {
    let failures = section_failures(context, unit);
    let base = section_context(context, unit, &[])?;
    let room = crate::model_call::source_room(
        llm,
        &repair_request(system, &base, output_tokens),
        usize::MAX,
    );
    // The section and its instructions alone: no claim can be repaired here.
    if room == 0 {
        return Err(crate::model_call::window_too_small(
            llm,
            "A section repair does not fit",
        ));
    }
    let table: HashMap<&str, usize> = context
        .get("evidence")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|passage| {
            Some((
                passage.get("id")?.as_str()?,
                llm.count_tokens(&passage.to_string()) + 1,
            ))
        })
        .collect();
    // A claim's price is its own text and those of its passages the group
    // does not list yet.
    let price = |failure: &Value, listed: &HashSet<String>| {
        llm.count_tokens(&failure.to_string())
            + 1
            + evidence_ids(failure)
                .into_iter()
                .filter(|id| !listed.contains(*id))
                .map(|id| table.get(id).copied().unwrap_or_default())
                .sum::<usize>()
    };
    let mut groups = Vec::new();
    let mut group: Vec<Value> = Vec::new();
    let mut listed: HashSet<String> = HashSet::new();
    let mut used = 0;
    for failure in failures {
        if used + price(&failure, &listed) > room && !group.is_empty() {
            groups.push(std::mem::take(&mut group));
            listed.clear();
            used = 0;
        }
        if price(&failure, &listed) > room {
            // Only this claim's own evidence is over the room: it goes
            // alone, with the passages that fit.
            groups.push(vec![failure]);
            continue;
        }
        used += price(&failure, &listed);
        listed.extend(evidence_ids(&failure).into_iter().map(str::to_owned));
        group.push(failure);
    }
    // A section with only failed examples still gets one request.
    if !group.is_empty() || groups.is_empty() {
        groups.push(group);
    }
    Ok(groups)
}

fn section_schema(schema: &Value, field: &str) -> Value {
    let array = |name: &str| json!({"type":"array","minItems":usize::from(name==field),"maxItems":usize::from(name==field),"items":schema.pointer(&format!("/properties/{name}/items")).cloned().unwrap_or_else(||json!({"type":"object"}))});
    json!({"type":"object","additionalProperties":false,"required":["blocks","questions"],"properties":{"blocks":array("blocks"),"questions":array("questions")}})
}

/// Puts the repaired section `patch` in place of the lesson's section `unit`.
fn merge(candidate: &mut Value, patch: Value, unit: usize) -> Result<()> {
    let (field, index) = section_of(candidate, unit)?;
    let repaired = patch
        .get(field)
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .ok_or_else(|| invalid("The section repair omitted its corrected content."))?;
    let original = candidate
        .get_mut(field)
        .and_then(Value::as_array_mut)
        .and_then(|items| items.get_mut(index))
        .ok_or_else(|| invalid("Missing original repair section."))?;
    *original = repaired.clone();
    Ok(())
}

/// A model's answer to a section repair or a flow repair as the one edited
/// section: its edits applied to `original`, a citation the rewrite appended
/// among the sentences kept removed, the changes outside the `targets`
/// reverted, and the section checked against the lesson schema and for its
/// kind. Gives the section and the targets whose correction was put back,
/// because it shared a change with a `protected` sentence.
pub(super) fn accept_edits(
    raw: &str,
    (original, lesson, wording): (&Value, &Value, &Wording),
    (schema, field): (&Value, &str),
    (targets, protected): (&[Protected], &[Protected]),
) -> Result<(Value, Vec<PutBack>)> {
    let mut repaired = text_edits::apply_in(original, raw, lesson, wording)?;
    let pointer = format!("/{field}/0");
    let mut put_back = Vec::new();
    if let (Some(before), Some(after)) =
        (original.pointer(&pointer), repaired.pointer_mut(&pointer))
    {
        let appended = citation::appended(before, after);
        let confined = confine(before, after, targets, protected)?;
        *after = confined.section;
        put_back = confined.put_back;
        citation::restore_appended(after, appended, field);
    }
    let validator = jsonschema::JSONSchema::compile(&section_schema(schema, field))
        .map_err(|error| invalid(format!("Invalid repair schema: {error}")))?;
    if !validator.is_valid(&repaired) {
        return Err(invalid(
            "The edited section does not match the lesson schema. Its prior draft is saved.",
        ));
    }
    let kind = |section: &Value| {
        section
            .pointer(&pointer)
            .and_then(|item| item.get("kind"))
            .cloned()
    };
    if kind(&repaired) != kind(original) {
        return Err(invalid(
            "A section repair changed its teaching or assessment role.",
        ));
    }
    Ok((repaired, put_back))
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

/// Repairs every affected section in turn, in as many requests as fit, each
/// on the section as the previous left it, saving every patch. A claim the
/// user resolved meanwhile is not sent (§9); an unusable answer costs its
/// claims an attempt and leaves the section as it was. A correction that
/// shared a change with a sentence it was not asked to change is put back and costs its own
/// claim alone, which is asked again, told why.
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
    let mut candidate = context
        .get("candidate")
        .cloned()
        .ok_or_else(|| invalid("Missing repair candidate."))?;
    let system = repair_system(schema);
    for (position, unit) in pending.iter().copied().enumerate() {
        let (field, _) = section_of(&candidate, unit)?;
        ledger.sync().await?;
        let dropped = withdraw_resolved(&mut context, ledger, policy);
        if !dropped.is_empty() {
            ledger.supersede(&dropped).await?;
        }
        let examples = claim_ids(&section_examples(&context, unit));
        if section_failures(&context, unit).is_empty() && examples.is_empty() {
            continue;
        }
        let mut groups: VecDeque<Vec<Value>> =
            match claim_groups(llm, &system, &context, unit, output_tokens) {
                Ok(groups) => groups.into(),
                Err(error) if unmade(&error) => {
                    let mut claims = claim_ids(&section_failures(&context, unit));
                    claims.extend(examples);
                    ledger.reject(policy, &claims, &error.to_string()).await?;
                    continue;
                }
                Err(error) => return Err(error),
            };
        let mut part = 0;
        while let Some(mut group) = groups.pop_front() {
            // Groups are sized part by part; a request that still does not
            // fit is split again, down to one claim with the passages that fit.
            let mut local = section_context(&context, unit, &group)?;
            if !crate::model_call::fits(llm, &repair_request(&system, &local, output_tokens)).await
            {
                if group.len() > 1 {
                    let rest = group.split_off(group.len() / 2);
                    groups.push_front(rest);
                    groups.push_front(group);
                    continue;
                }
                let kept = fitted_passages(llm, &system, &local, output_tokens).await;
                if kept.is_empty() {
                    // Nothing to repair it from: the attempt is used, and the claim goes on to its last, which cuts it.
                    let reason = "A failed statement's evidence passage does not fit in this model's context window.";
                    ledger.reject(policy, &claim_ids(&group), reason).await?;
                    drop_group(&mut context, unit, group.len(), false);
                    continue;
                }
                tracing::warn!(
                    unit,
                    kept = kept.len(),
                    recorded = group.first().map_or(0, |failure| evidence_ids(failure).len()),
                    "A failed claim's evidence exceeds the repair budget; sending the passages that fit"
                );
                if let Some(fields) = group.first_mut().and_then(Value::as_object_mut) {
                    fields.insert("evidence".into(), json!(kept));
                }
                local = section_context(&context, unit, &group)?;
            }
            part += 1;
            let parts = part + groups.len();
            crate::lesson_progress::phase(
                crate::lesson_progress::Phase::Repair,
                if parts > 1 {
                    format!(
                        "Correcting affected section {} of {} (part {part} of {parts})",
                        position + 1,
                        pending.len()
                    )
                } else {
                    format!(
                        "Correcting affected section {} of {}",
                        position + 1,
                        pending.len()
                    )
                },
            );
            let original = local
                .get("candidate")
                .ok_or_else(|| invalid("Missing repair candidate."))?;
            // The group's failed sentences may change; every other sentence
            // of the section a claim stands on comes back as it was (§10).
            let mut claims = claim_ids(&group);
            if part == 1 {
                claims.extend(claim_ids(&section_examples(&context, unit)));
            }
            let sent = ledger.with_cotenants(&claims);
            let of_unit = |set: Vec<Protected>| -> Vec<Protected> {
                set.into_iter().filter(|item| item.unit == unit).collect()
            };
            let confined = (
                of_unit(Protected::of_claims(ledger, &sent)),
                of_unit(Protected::of_ledger(ledger, &sent)),
            );
            let before_group = candidate.to_string();
            let (repaired, put_back) = match repair_group(
                llm,
                &system,
                (&local, original, &candidate, &wording),
                schema,
                field,
                output_tokens,
                (&confined.0, &confined.1),
            )
            .await
            {
                Ok((repaired, put_back)) => (merge(&mut candidate, repaired, unit), put_back),
                Err(error) => (Err(error), Vec::new()),
            };
            if let Err(error) = repaired {
                if !unmade(&error) {
                    return Err(error);
                }
                ledger.reject(policy, &claims, &error.to_string()).await?;
            } else {
                trace::changed(
                    Stage::ClaimRepair,
                    &Cause::none().claims(&claims),
                    &before_group,
                    &candidate.to_string(),
                );
            }
            // A target whose correction shared a change with a sentence it was
            // not asked to change was put back: it alone is charged and told why, and the
            // group's other targets keep their rewrites.
            for back in &put_back {
                for id in &claims {
                    let own = Protected::of_claims(ledger, std::slice::from_ref(id));
                    if own.first() == Some(&back.target) {
                        ledger
                            .reject(policy, std::slice::from_ref(id), &back.reason)
                            .await?;
                    }
                }
            }
            let saved = candidate.to_string();
            drop_group(&mut context, unit, group.len(), part == 1);
            if let Some(object) = context.as_object_mut() {
                object.insert("candidate".into(), candidate.clone());
            }
            let checkpoint = PendingRepair {
                fingerprint: repair_fingerprint(llm, prompt, schema, &candidate, references),
                defects: ledger_targets(&context),
                context: context.clone(),
            };
            let remaining = if !groups.is_empty() || position + 1 < pending.len() {
                Some(serde_json::to_string(&checkpoint)?)
            } else {
                None
            };
            // Candidate and remaining work share one repository transaction.
            crate::lesson_drafts::save_repair(&saved, remaining).await?;
        }
    }
    Ok(candidate)
}

/// What a section repair is asked: the edits of its failed sentences.
struct SectionRepair;

impl Contract for SectionRepair {
    const ID: ContractId = ContractId::new("section-repair");
    type Input = PromptInput;
}

#[cfg(any(test, feature = "test-support"))]
impl lattice_core::application::services::model_contract::test_support::RenderAnswer
    for SectionRepair
{
    type Answer = text_edits::Patch;

    fn render(answer: &text_edits::Patch) -> String {
        serde_json::to_string(answer).unwrap_or_default()
    }

    fn read(text: &str) -> Result<text_edits::Patch> {
        text_edits::read_patch(text)
    }
}

/// One request's edit of one section, confined to the failed sentences
/// (`targets`; `protected` must come back byte-identical) and validated
/// against the lesson schema. Gives the section and the targets whose
/// correction was put back, because it shared a change with a protected
/// sentence.
async fn repair_group(
    llm: &dyn LLMPort,
    system: &str,
    (local, original, lesson, wording): (&Value, &Value, &Value, &Wording),
    schema: &Value,
    field: &str,
    output_tokens: usize,
    confined: (&[Protected], &[Protected]),
) -> Result<(Value, Vec<PutBack>)> {
    let prompt = local.to_string();
    let call = ContractCall::of::<SectionRepair>(&PromptInput {
        system: system.to_owned(),
        prompt: prompt.clone(),
    })?;
    let request = crate::model_call::structured(
        system,
        prompt,
        text_edits::schema_for(original, schema),
        output_tokens,
    );
    let _model_call = crate::lesson_progress::model_call(crate::lesson_progress::Term::Rewrite);
    crate::lessons::structured_call::complete_checked_request(
        llm,
        &call,
        request,
        None,
        None,
        |raw| accept_edits(raw, (original, lesson, wording), (schema, field), confined),
    )
    .await
}

#[cfg(test)]
#[path = "repair_tests.rs"]
mod tests;
