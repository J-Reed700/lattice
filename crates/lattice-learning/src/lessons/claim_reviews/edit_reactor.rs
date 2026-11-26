//! Derived-data reactor keeping the claim ledger, verification report, and
//! teaching-findings change markers in sync with `learning.section` and
//! `learning.question` edits, claim actions, and reverts.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use lattice_core::domain::learning::claim_review::{
    Actor, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimState, EventProvenance, LessonReview,
    UnitKind, UserAction,
};
use lattice_core::domain::provenance::ClaimKind;
use lattice_core::shared::edits::{
    Change, Consequence, EditOrigin, EditReactor, EditRegistry, Reaction,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::{Database, Reading, WriteTx};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::published_edit::{self, update_report, Recorded};
use super::published_remove;
use super::published_restore;
use super::published_rewrite::{self, FieldPlan, Written};
use super::repository::{
    append_in, blocks, bump_revision, load_in, teaching_findings as findings_repo,
};
use super::service::{endings, record_edit, settle_planned_in, EditRecord, Ending};
use super::trust_changed::redecide_kinds;
use crate::assessment_engine::LearningItemFormat;
use crate::edits::keys::{self, affected};
use crate::edits::question::ledger_unit;

/// Explicit context supplied by claim-review actions when editing a section.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum ClaimsContext {
    Claim { claim_id: String, event: ClaimEvent },
    Sentence { sentence: String },
}

/// Receipt recorded on `content_edits` for reversible ledger mutations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ClaimsReceipt {
    pub acted: Vec<(String, u32)>,
    pub created: Vec<String>,
    pub step: String,
    pub revert: bool,
}

pub(super) const ACTED_SINCE: &str =
    "A statement this change ended was acted on since. Undo that action in the review, then undo this change.";
pub(super) const MOVED_UNDER_ACTION: &str =
    "The section's text changed while this was being saved. Try again.";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FollowUp {
    kinds: Vec<ClaimKind>,
}

struct FieldDiff {
    field: String,
    kind: UnitKind,
    title_rule: bool,
    before: String,
    after: String,
}

struct ResolvedUnit {
    lesson_id: String,
    unit_index: usize,
    section: bool,
    site: &'static str,
    site_index: usize,
    before_body: String,
    after_title: String,
    after_body: String,
    fields: Vec<FieldDiff>,
}

struct ClaimsReactor {
    database: Database,
}

pub(crate) fn register(registry: &mut EditRegistry, database: &Database) -> Result<()> {
    registry.add_reactor(Arc::new(ClaimsReactor {
        database: database.clone(),
    }))
}

fn provenance(review: &LessonReview, at: i64) -> EventProvenance {
    EventProvenance {
        actor: Actor::User,
        at,
        job_id: None,
        app_version: env!("CARGO_PKG_VERSION").into(),
        checker_model: Some(review.checker_model.clone()),
    }
}

fn question_fields(
    format: LearningItemFormat,
    doc: &Value,
) -> (String, String, Vec<(String, String)>) {
    let prompt = doc
        .get("prompt")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let explanation = doc
        .get("explanation")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let mut options = Vec::new();
    let mut rationales = Vec::new();
    match format {
        LearningItemFormat::SingleChoice | LearningItemFormat::MultiSelect => {
            if let Some(choices) = doc.get("choices").and_then(Value::as_array) {
                for choice in choices {
                    options.push(
                        choice
                            .get("text")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                    );
                    rationales.push(
                        choice
                            .get("rationale")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                    );
                }
            }
        }
        LearningItemFormat::TrueFalse => {
            options.push("True".to_owned());
            options.push("False".to_owned());
        }
        _ => {}
    }
    if rationales.iter().all(|reason| reason.trim().is_empty()) {
        rationales.clear();
    }
    let text = options
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(explanation.as_str()))
        .chain(rationales.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join("\n\n");
    let mut fields = Vec::new();
    fields.push(("prompt".into(), prompt.clone()));
    for (slot, option) in options.into_iter().enumerate() {
        fields.push((format!("options/{slot}"), option));
    }
    fields.push(("explanation".into(), explanation));
    for (slot, rationale) in rationales.into_iter().enumerate() {
        fields.push((format!("rationales/{slot}"), rationale));
    }
    (prompt, text, fields)
}

fn pair_question_fields(
    before: Vec<(String, String)>,
    after: Vec<(String, String)>,
) -> Vec<FieldDiff> {
    let mut names: Vec<String> = before.iter().map(|(name, _)| name.clone()).collect();
    for (name, _) in &after {
        if !names.contains(name) {
            names.push(name.clone());
        }
    }
    let lookup = |list: &[(String, String)], key: &str| -> String {
        list.iter()
            .find(|(name, _)| name == key)
            .map(|(_, val)| val.clone())
            .unwrap_or_default()
    };
    names
        .into_iter()
        .map(|field| {
            let kind = UnitKind::of(false, &field);
            let before_val = lookup(&before, &field);
            let after_val = lookup(&after, &field);
            FieldDiff {
                field,
                kind,
                title_rule: kind == UnitKind::Question,
                before: before_val,
                after: after_val,
            }
        })
        .collect()
}

async fn resolve_unit(unit: &mut Reading<'_>, change: &Change<'_>) -> Result<Option<ResolvedUnit>> {
    if change.content.kind == keys::SECTION {
        let Some((lesson_id, ordinal)) = keys::section_key(&change.content.key) else {
            return Ok(None);
        };
        let get_str = |doc: &Value, key: &str| {
            doc.get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let before_title = get_str(change.before, "title");
        let before_body = get_str(change.before, "body");
        let after_title = get_str(change.after, "title");
        let after_body = get_str(change.after, "body");
        return Ok(Some(ResolvedUnit {
            lesson_id: lesson_id.to_owned(),
            unit_index: ordinal,
            section: true,
            site: "section",
            site_index: ordinal,
            before_body: before_body.clone(),
            after_title: after_title.clone(),
            after_body: after_body.clone(),
            fields: vec![
                FieldDiff {
                    field: "title".into(),
                    kind: UnitKind::Block,
                    title_rule: true,
                    before: before_title,
                    after: after_title,
                },
                FieldDiff {
                    field: "body".into(),
                    kind: UnitKind::Block,
                    title_rule: false,
                    before: before_body,
                    after: after_body,
                },
            ],
        }));
    }
    if change.content.kind == keys::QUESTION {
        let Some(lu) = ledger_unit(&mut *unit, &change.content.key).await? else {
            return Ok(None);
        };
        let sections = usize::try_from(blocks::section_count(&mut *unit, &lu.lesson_id).await?)
            .unwrap_or(usize::MAX);
        let site_index = lu.unit_index.saturating_sub(sections);
        let (_, before_body, before_fields) = question_fields(lu.format, change.before);
        let (after_title, after_body, after_fields) = question_fields(lu.format, change.after);
        return Ok(Some(ResolvedUnit {
            lesson_id: lu.lesson_id,
            unit_index: lu.unit_index,
            section: false,
            site: "question",
            site_index,
            before_body,
            after_title,
            after_body,
            fields: pair_question_fields(before_fields, after_fields),
        }));
    }
    Ok(None)
}

struct RewritePlan<'a> {
    ending: Vec<Ending<'a>>,
    written: Vec<(&'a str, UnitKind, Written)>,
}

fn plan_rewrite<'a>(claims: &'a [ClaimLedgerEntry], unit: &'a ResolvedUnit) -> RewritePlan<'a> {
    let mut plans: Vec<(&FieldDiff, FieldPlan)> = Vec::new();
    for diff in &unit.fields {
        if diff.before == diff.after {
            continue;
        }
        let mut planned = published_rewrite::plan(
            claims,
            unit.unit_index,
            unit.section,
            &diff.field,
            &diff.before,
            &diff.after,
        );
        if diff.title_rule && planned.edited.is_empty() {
            planned.written.clear();
        }
        plans.push((diff, planned));
    }
    let mut ending = Vec::new();
    let mut written = Vec::new();
    for (diff, planned) in plans {
        ending.extend(endings(claims, &planned, &diff.before));
        for item in planned.written {
            written.push((diff.field.as_str(), diff.kind, item));
        }
    }
    RewritePlan { ending, written }
}

fn checked_consequences(ending: &[Ending<'_>]) -> Vec<Consequence> {
    let count = ending
        .iter()
        .filter(|item| matches!(item.entry.state(), ClaimState::ResolvedByEvidence { .. }))
        .count();
    if count == 0 {
        return Vec::new();
    }
    let message = if count == 1 {
        "Saving changes 1 statement that was checked against the sources. What you write is marked as written by you, not checked.".into()
    } else {
        format!(
            "Saving changes {count} statements that were checked against the sources. What you write is marked as written by you, not checked."
        )
    };
    vec![Consequence {
        code: "claims.checked_statements".into(),
        message,
        related: Vec::new(),
    }]
}

fn reverted_receipt_of(change: &Change<'_>) -> Option<ClaimsReceipt> {
    if !matches!(change.origin, EditOrigin::Revert { .. }) {
        return None;
    }
    let receipt: ClaimsReceipt = serde_json::from_value(change.reverted_receipt?.clone()).ok()?;
    (!receipt.revert).then_some(receipt)
}

fn to_recorded(rec: Recorded, step: &str) -> ClaimsReceipt {
    ClaimsReceipt {
        acted: rec.acted.into_iter().map(|(id, ord)| (id.0, ord)).collect(),
        created: rec.created.into_iter().map(|id| id.0).collect(),
        step: step.into(),
        revert: false,
    }
}

async fn apply_action(
    tx: &mut WriteTx,
    program_id: &str,
    unit: &ResolvedUnit,
    ledger_claims: &[ClaimLedgerEntry],
    context: &Value,
    user: &EventProvenance,
) -> Result<ClaimsReceipt> {
    let parsed: ClaimsContext = serde_json::from_value(context.clone())?;
    match parsed {
        ClaimsContext::Claim { claim_id, event } => {
            let id = ClaimId(claim_id);
            match &event {
                ClaimEvent::UserAction {
                    action: UserAction::Edit { before, after },
                    ..
                } => {
                    let plan =
                        published_edit::plan(ledger_claims, &id, before, after, &unit.before_body)?;
                    if plan.new_body != unit.after_body {
                        return Err(AppError::InvalidState(MOVED_UNDER_ACTION.into()));
                    }
                    let rec =
                        published_edit::record_in(tx, program_id, &unit.lesson_id, &plan, user)
                            .await?;
                    Ok(to_recorded(rec, "written_over"))
                }
                ClaimEvent::UserAction {
                    action: UserAction::Remove,
                    ..
                } => {
                    let plan = published_remove::plan(ledger_claims, &id, &unit.before_body)?;
                    if plan.new_body != unit.after_body {
                        return Err(AppError::InvalidState(MOVED_UNDER_ACTION.into()));
                    }
                    let rec =
                        published_remove::record_in(tx, program_id, &unit.lesson_id, &plan, user)
                            .await?;
                    Ok(to_recorded(rec, "taken_out"))
                }
                ClaimEvent::UserAction {
                    action: UserAction::PutBack { .. } | UserAction::RestoreOriginal { .. },
                    ..
                } => {
                    let target = ledger_claims
                        .iter()
                        .find(|entry| entry.claim_id() == &id)
                        .ok_or_else(|| {
                            AppError::NotFound("No statement with that id in this lesson.".into())
                        })?;
                    let plan =
                        published_restore::plan(ledger_claims, target, &event, &unit.before_body)?;
                    if plan.new_body != unit.after_body {
                        return Err(AppError::InvalidState(MOVED_UNDER_ACTION.into()));
                    }
                    let rec =
                        published_restore::record_in(tx, program_id, &unit.lesson_id, &plan, user)
                            .await?;
                    Ok(to_recorded(rec, "written_back"))
                }
                _ => Err(AppError::InternalError(
                    "Unexpected claim event in claims reactor context.".into(),
                )),
            }
        }
        ClaimsContext::Sentence { sentence } => {
            let plan = published_remove::plan_sentence(
                ledger_claims,
                unit.unit_index,
                &sentence,
                &unit.before_body,
            )?;
            if plan.new_body != unit.after_body {
                return Err(AppError::InvalidState(MOVED_UNDER_ACTION.into()));
            }
            let rec =
                published_remove::record_in(tx, program_id, &unit.lesson_id, &plan, user).await?;
            Ok(to_recorded(rec, "taken_out"))
        }
    }
}

async fn apply_revert(
    tx: &mut WriteTx,
    program_id: &str,
    unit: &ResolvedUnit,
    ledger_claims: &[ClaimLedgerEntry],
    change: &Change<'_>,
    receipt: &ClaimsReceipt,
    user: &EventProvenance,
) -> Result<ClaimsReceipt> {
    for (claim_id, ordinal) in &receipt.acted {
        let entry = ledger_claims
            .iter()
            .find(|entry| entry.claim_id().0 == *claim_id)
            .ok_or_else(|| AppError::InvalidState(ACTED_SINCE.into()))?;
        if entry.undo_target() != Some(*ordinal) {
            return Err(AppError::InvalidState(ACTED_SINCE.into()));
        }
        append_in(
            tx,
            &unit.lesson_id,
            entry.claim_id(),
            user.clone(),
            ClaimEvent::Undone {
                event_ordinal: *ordinal,
            },
        )
        .await?;
    }
    let machine = user.as_machine();
    for claim_id in &receipt.created {
        let superseded = ClaimEvent::Superseded {
            successor: None,
            by: Actor::User,
        };
        append_in(
            tx,
            &unit.lesson_id,
            &ClaimId(claim_id.clone()),
            machine.clone(),
            superseded,
        )
        .await?;
    }
    let written_set: HashSet<String> = receipt.created.iter().cloned().collect();
    let mut all_claims: HashSet<String> = receipt.acted.iter().map(|(id, _)| id.clone()).collect();
    all_claims.extend(receipt.created.iter().cloned());
    update_report(tx, program_id, &unit.lesson_id, |report| {
        match receipt.step.as_str() {
            "taken_out" => report.taken_back(unit.unit_index, &unit.after_body, &all_claims),
            "written_over" | "written_back" => {
                report.written_out(unit.unit_index, &unit.after_body, &all_claims)
            }
            "rewritten" => {
                let reverted_id = match change.origin {
                    EditOrigin::Revert { edit_id } => edit_id.as_str(),
                    _ => "",
                };
                report.rewritten_back(
                    unit.unit_index,
                    &unit.after_title,
                    &unit.after_body,
                    reverted_id,
                    &written_set,
                );
            }
            _ => {}
        }
    })
    .await?;
    Ok(ClaimsReceipt {
        acted: Vec::new(),
        created: Vec::new(),
        step: "reverted".into(),
        revert: true,
    })
}

#[async_trait]
impl EditReactor for ClaimsReactor {
    fn name(&self) -> &'static str {
        keys::CLAIMS_REACTOR
    }

    fn kinds(&self) -> &'static [&'static str] {
        &[keys::SECTION, keys::QUESTION]
    }

    async fn preview(
        &self,
        unit: &mut Reading<'_>,
        change: &Change<'_>,
    ) -> Result<Vec<Consequence>> {
        let Some(resolved) = resolve_unit(&mut *unit, change).await? else {
            return Ok(Vec::new());
        };
        let ledger = load_in(&mut *unit, &resolved.lesson_id).await?;
        if ledger.review.is_none() {
            return Ok(Vec::new());
        }
        let plan = plan_rewrite(&ledger.claims, &resolved);
        Ok(checked_consequences(&plan.ending))
    }

    async fn apply(&self, tx: &mut WriteTx, change: &Change<'_>) -> Result<Reaction> {
        let Some(unit) = resolve_unit(&mut tx.reading(), change).await? else {
            return Ok(Reaction::default());
        };
        let ledger = load_in(&mut *tx, &unit.lesson_id).await?;
        let Some(review) = &ledger.review else {
            findings_repo::mark_part_changed(
                tx,
                &unit.lesson_id,
                unit.site,
                unit.site_index,
                change.at,
            )
            .await?;
            return Ok(Reaction {
                affected: vec![affected::CLAIMS.into()],
                ..Reaction::default()
            });
        };
        let user = provenance(review, change.at);
        if let Some(receipt) = reverted_receipt_of(change) {
            let reverted = apply_revert(
                tx,
                &review.program_id,
                &unit,
                &ledger.claims,
                change,
                &receipt,
                &user,
            )
            .await?;
            findings_repo::mark_part_changed(
                tx,
                &unit.lesson_id,
                unit.site,
                unit.site_index,
                change.at,
            )
            .await?;
            bump_revision(tx, &unit.lesson_id).await?;
            return Ok(Reaction {
                receipt: Some(serde_json::to_value(&reverted)?),
                affected: vec![affected::CLAIMS.into()],
                consequences: Vec::new(),
                follow_up: None,
            });
        }
        if let Some(context) = change.context {
            let receipt = apply_action(
                tx,
                &review.program_id,
                &unit,
                &ledger.claims,
                context,
                &user,
            )
            .await?;
            findings_repo::mark_part_changed(
                tx,
                &unit.lesson_id,
                unit.site,
                unit.site_index,
                change.at,
            )
            .await?;
            bump_revision(tx, &unit.lesson_id).await?;
            return Ok(Reaction {
                receipt: Some(serde_json::to_value(&receipt)?),
                affected: vec![affected::CLAIMS.into()],
                consequences: Vec::new(),
                follow_up: None,
            });
        }
        let plan = plan_rewrite(&ledger.claims, &unit);
        let consequences = checked_consequences(&plan.ending);
        let written_refs: Vec<(&str, UnitKind, &Written)> = plan
            .written
            .iter()
            .map(|(field, kind, item)| (*field, *kind, item))
            .collect();
        let EditRecord {
            planned,
            acted,
            created,
        } = record_edit(
            tx,
            &unit.lesson_id,
            unit.unit_index,
            &written_refs,
            &plan.ending,
            &user,
        )
        .await?;
        let ended: HashSet<String> = plan
            .ending
            .iter()
            .map(|item| item.entry.claim_id().0.clone())
            .collect();
        let report_written: Vec<(&str, &ClaimId)> = plan
            .written
            .iter()
            .map(|(_, _, item)| (item.text.as_str(), &item.successor))
            .collect();
        update_report(tx, &review.program_id, &unit.lesson_id, |report| {
            report.rewritten(&crate::content_verification::Rewritten {
                unit: unit.unit_index,
                new_title: &unit.after_title,
                new_body: &unit.after_body,
                ended: &ended,
                written: &report_written,
                at: change.at,
                edit_id: change.edit_id,
            })
        })
        .await?;
        findings_repo::mark_part_changed(
            tx,
            &unit.lesson_id,
            unit.site,
            unit.site_index,
            change.at,
        )
        .await?;
        bump_revision(tx, &unit.lesson_id).await?;
        let moved = settle_planned_in(tx, &ledger, &unit.lesson_id, &planned).await?;
        let follow_up = (!moved.is_empty()).then(|| json!({ "kinds": moved }));
        let receipt = ClaimsReceipt {
            acted,
            created,
            step: "rewritten".into(),
            revert: false,
        };
        Ok(Reaction {
            receipt: Some(serde_json::to_value(&receipt)?),
            affected: vec![affected::CLAIMS.into()],
            consequences,
            follow_up,
        })
    }

    async fn after_commit(&self, follow_up: &Value) -> Result<()> {
        let parsed: FollowUp = serde_json::from_value(follow_up.clone())?;
        redecide_kinds(&self.database, parsed.kinds).await?;
        Ok(())
    }
}
