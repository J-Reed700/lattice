//! The machine's last attempt on a claim takes its text out of the lesson
//! (claim classes design §3.2): the sentence-level edit a repair makes, made
//! here, whole, in no request to the model.
use super::super::claim_ledger::{field_pointer, ClaimLedger};
use super::super::repair_work::{withdraw, withdraw_resolved};
use super::super::*;
use lattice_core::domain::learning::claim_review::ClaimId;
use lattice_core::shared::text::sentences;

/// Why a claim whose text cannot be taken out uses its attempt instead.
const KEPT: &str = "Taking this sentence out would leave nothing in its place, or it is part of a code block; it stays for you to resolve.";

/// What taking a claim's text out of its field comes to.
enum Cut {
    /// The field without the text.
    Done(String),
    /// The text is not in the field: it is out already.
    Absent,
    /// The field would be left empty, or a code block broken.
    Refused,
}

fn is_fence(line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with("```") || line.starts_with("~~~")
}

/// Whether the byte `at` of `text` lies between a fence line and its closing
/// one.
fn inside_fence(text: &str, at: usize) -> bool {
    text.get(..at)
        .is_some_and(|before| before.lines().filter(|line| is_fence(line)).count() % 2 == 1)
}

/// The separator that keeps more line breaks, the first on a tie.
fn stronger<'t>(first: &'t str, second: &'t str) -> &'t str {
    if second.matches('\n').count() > first.matches('\n').count() {
        second
    } else {
        first
    }
}

/// `text` without every sentence `quote` touches, wherever it occurs; the
/// sentences left keep the separators a paragraph break needs.
fn sentence(text: &str, quote: &str) -> Cut {
    let found: Vec<(usize, usize)> = text
        .match_indices(quote)
        .filter(|_| !quote.trim().is_empty())
        .map(|(at, _)| (at, at + quote.len()))
        .collect();
    if found.is_empty() {
        return Cut::Absent;
    }
    if quote.lines().any(is_fence) || found.iter().any(|&(at, _)| inside_fence(text, at)) {
        return Cut::Refused;
    }
    let spans = sentences(text);
    let gone: Vec<bool> = spans
        .iter()
        .map(|&(start, end)| found.iter().any(|&(from, to)| start < to && end > from))
        .collect();
    if !gone.contains(&true) {
        return Cut::Absent;
    }
    if !gone.contains(&false) {
        return Cut::Refused;
    }
    let first = spans.first().map_or(0, |span| span.0);
    let mut out = text.get(..first).unwrap_or_default().to_owned();
    // The separator before the next sentence kept: the strongest of those
    // from the last one kept up to it.
    let mut between: Option<&str> = None;
    for (index, &(start, end)) in spans.iter().enumerate() {
        let next = spans.get(index + 1).map_or(text.len(), |span| span.0);
        let separator = text.get(end..next).unwrap_or_default();
        if gone.get(index) == Some(&true) {
            between = between.map(|kept| stronger(kept, separator));
            continue;
        }
        out.push_str(between.unwrap_or_default());
        out.push_str(text.get(start..end).unwrap_or_default());
        between = Some(separator);
    }
    out.push_str(text.get(text.trim_end().len()..).unwrap_or_default());
    Cut::Done(out)
}

/// `text` without the fenced block whose code is `code`.
fn fence(text: &str, code: &str) -> Cut {
    let Some(at) = text.find(code).filter(|_| !code.trim().is_empty()) else {
        return Cut::Absent;
    };
    let opened = text.get(..at).and_then(|before| {
        let line = before.trim_end_matches('\n');
        let start = line.rfind('\n').map_or(0, |newline| newline + 1);
        is_fence(line.get(start..)?).then_some(start)
    });
    let closed = text.get(at + code.len()..).and_then(|after| {
        let line = after.split_inclusive('\n').next()?;
        is_fence(line).then_some(at + code.len() + line.len())
    });
    let (Some(start), Some(stop)) = (opened, closed) else {
        return Cut::Refused;
    };
    let (Some(before), Some(after)) = (text.get(..start), text.get(stop..)) else {
        return Cut::Refused;
    };
    let left = if after.trim().is_empty() {
        let tail = text.get(text.trim_end().len()..).unwrap_or_default();
        format!("{}{tail}", before.trim_end())
    } else if before.is_empty() || before.ends_with("\n\n") {
        format!("{before}{}", after.trim_start_matches('\n'))
    } else {
        format!("{before}{after}")
    };
    if left.trim().is_empty() {
        Cut::Refused
    } else {
        Cut::Done(left)
    }
}

fn out(example: bool, text: &str, quote: &str) -> Cut {
    if example {
        fence(text, quote)
    } else {
        sentence(text, quote)
    }
}

/// The claims of `context`'s `key` sent on their last attempt.
fn last(context: &Value, key: &str) -> Vec<ClaimId> {
    context
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item.get("lastAttempt").and_then(Value::as_bool) == Some(true))
        .filter_map(|item| item.get("claimId").and_then(Value::as_str))
        .map(|id| ClaimId(id.to_owned()))
        .collect()
}

/// Takes out of `context`'s lesson the text of every claim sent on its last
/// attempt, and those claims out of the work that remains: the code makes
/// the cut, so the model is never asked to. A claim whose text cannot be
/// taken out uses its attempt and waits for the user, as it does after a
/// repair that produced nothing. Returns how many claims it dealt with.
pub(super) async fn last_attempts(context: &mut Value, ledger: &mut ClaimLedger) -> Result<usize> {
    ledger.sync().await?;
    let dropped = withdraw_resolved(context, ledger);
    if !dropped.is_empty() {
        ledger.supersede(&dropped).await?;
    }
    let mut lesson = context
        .get("candidate")
        .cloned()
        .ok_or_else(|| invalid("Missing repair candidate."))?;
    let (mut taken, mut refused) = (Vec::new(), Vec::new());
    for (key, example) in [("failedClaims", false), ("exampleClaims", true)] {
        for id in last(context, key) {
            let Some(place) = ledger.entry(&id).map(|entry| entry.location().clone()) else {
                continue;
            };
            let pointer = field_pointer(&lesson, place.unit_index, &place.field);
            let cut = pointer
                .as_deref()
                .and_then(|pointer| lesson.pointer(pointer))
                .and_then(Value::as_str)
                .map_or(Cut::Absent, |text| out(example, text, &place.quote));
            match cut {
                Cut::Done(text) => {
                    if let Some(slot) = pointer
                        .as_deref()
                        .and_then(|pointer| lesson.pointer_mut(pointer))
                    {
                        *slot = Value::String(text);
                    }
                    taken.push(id);
                }
                Cut::Absent => taken.push(id),
                Cut::Refused => refused.push(id),
            }
        }
    }
    if let Some(slot) = context.get_mut("candidate") {
        *slot = lesson;
    }
    withdraw(context, |id| taken.contains(id) || refused.contains(id));
    if !refused.is_empty() {
        ledger.reject(&refused, KEPT).await?;
    }
    Ok(taken.len() + refused.len())
}

#[cfg(test)]
#[path = "cut_tests.rs"]
mod tests;
