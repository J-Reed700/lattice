//! The bookkeeping of a section repair's remaining work: which claims it is
//! for, which the user resolved meanwhile, and what is left after a request.
use super::claim_ledger::ClaimLedger;
use super::*;
use lattice_core::domain::learning::claim_review::ClaimId;

/// The claim ids of `items` (failed claims or example claims).
pub(super) fn claim_ids(items: &[Value]) -> Vec<ClaimId> {
    items
        .iter()
        .filter_map(|item| item.get("claimId").and_then(Value::as_str))
        .map(|id| ClaimId(id.to_owned()))
        .collect()
}

/// The example claims recorded for `unit`.
pub(super) fn section_examples(context: &Value, unit: usize) -> Vec<Value> {
    context
        .get("exampleClaims")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|example| example.get("unit").and_then(Value::as_u64) == Some(unit as u64))
        .cloned()
        .collect()
}

/// A request's failed claims or examples on their last attempt: the repair
/// removes the assertion whole (`repeatedDefect`, §4 escalation).
pub(super) fn escalated(items: &[Value]) -> bool {
    items
        .iter()
        .any(|item| item.get("repeatedDefect").and_then(Value::as_bool) == Some(true))
}

/// Whether a section repair's failure is its answer's, so the claims it was
/// for use their attempt and the loop goes on (§3 `Rejected`), rather than an
/// outage or a cancellation, which stop the job.
pub(super) fn rejected(error: &AppError) -> bool {
    matches!(
        error,
        AppError::InvalidInput(_) | AppError::PromptExceedsWindow { .. }
    )
}

/// Takes a sent group's claims, and with the section's first request its
/// failed examples, out of the work that remains. The group's claims are the
/// section's next ones in recorded order.
pub(super) fn drop_group(context: &mut Value, unit: usize, sent: usize, first: bool) {
    let Some(object) = context.as_object_mut() else {
        return;
    };
    if let Some(items) = object.get_mut("failedClaims").and_then(Value::as_array_mut) {
        let mut settled = 0;
        items.retain(|item| {
            let ours = item.get("unit").and_then(Value::as_u64) == Some(unit as u64);
            let keep = !ours || settled >= sent;
            settled += usize::from(ours && !keep);
            keep
        });
    }
    if first {
        for key in ["executions", "exampleClaims"] {
            if let Some(items) = object.get_mut(key).and_then(Value::as_array_mut) {
                items.retain(|item| item.get("unit").and_then(Value::as_u64) != Some(unit as u64));
            }
        }
    }
}

/// Withdraws from the remaining work every claim the ledger no longer lets
/// the machine repair, and returns them (§9).
pub(super) fn withdraw_resolved(context: &mut Value, ledger: &ClaimLedger) -> Vec<ClaimId> {
    let mut withdrawn = Vec::new();
    let Some(object) = context.as_object_mut() else {
        return withdrawn;
    };
    for key in ["failedClaims", "exampleClaims"] {
        if let Some(items) = object.get_mut(key).and_then(Value::as_array_mut) {
            items.retain(|item| {
                let Some(id) = item.get("claimId").and_then(Value::as_str) else {
                    return true;
                };
                let id = ClaimId(id.to_owned());
                let keep = ledger.may_repair(&id);
                if !keep {
                    withdrawn.push(id);
                }
                keep
            });
        }
    }
    let examples: Vec<Value> = object
        .get("exampleClaims")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(items) = object.get_mut("executions").and_then(Value::as_array_mut) {
        items.retain(|item| {
            examples
                .iter()
                .any(|example| example.get("unit") == item.get("unit"))
        });
    }
    withdrawn
}

/// The claims a repair context was sent for: its failed claims and examples.
pub(super) fn context_claims(context: &Value) -> Vec<ClaimId> {
    ["failedClaims", "exampleClaims"]
        .into_iter()
        .filter_map(|key| context.get(key).and_then(Value::as_array))
        .flat_map(|items| claim_ids(items))
        .collect()
}

/// The claims the remaining repair work is for.
pub(super) fn ledger_targets(context: &Value) -> usize {
    ["failedClaims", "exampleClaims"]
        .iter()
        .filter_map(|key| context.get(*key).and_then(Value::as_array))
        .map(Vec::len)
        .sum()
}
