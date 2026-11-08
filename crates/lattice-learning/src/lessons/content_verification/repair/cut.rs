//! The machine's last attempt on a claim takes its text out of the lesson
//! (claim classes design §3.2): the sentence-level edit a repair makes, made
//! here, whole, in no request to the model, unless the claim's sentence says
//! more than it: then the model is asked to take the claim out of it (acceptance
//! design §2.3, §2.6).
use super::super::claim_ledger::{field_pointer, field_text, ClaimLedger, CutOf};
use super::super::repair_work::{withdraw, withdraw_resolved};
use super::super::sections::{answer_lost, gap_around_code, gaps, unit_label, unpublishable};
use super::super::*;
use super::goal_words;
use lattice_core::domain::learning::acceptance::{AcceptancePolicy, Goal, Outcome};
use lattice_core::domain::learning::claim_review::ClaimId;
use lattice_core::shared::text::sentences;

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

/// Tells the failed claims of `context` that are `narrowed` what the model is
/// asked of them: no last attempt for the code to take, and the narrowing goal.
fn ask_narrowing(context: &mut Value, narrowed: &[ClaimId]) {
    let Some(items) = context
        .get_mut("failedClaims")
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    for item in items {
        let named = item
            .get("claimId")
            .and_then(Value::as_str)
            .is_some_and(|id| narrowed.iter().any(|claim| claim.0 == id));
        let Some(fields) = item.as_object_mut().filter(|_| named) else {
            continue;
        };
        fields.remove("lastAttempt");
        fields.insert("goal".into(), json!(goal_words(Goal::KeepTheOthers)));
    }
}

/// Takes out of `context`'s lesson the text of every claim sent on its last
/// attempt, and those claims out of the work that remains: the code makes
/// the cut, so the model is never asked to. What a statement's cut takes is
/// the ledger's decision (`ClaimLedger::cut_of`), not the flag the request
/// carried: the sentence that states it, wherever the lesson writes that
/// sentence, when no other live statement stands on it; everything its quote
/// touches once a narrowing of it was not applied. Otherwise nothing is cut: the
/// statement stays among the claims sent, without `lastAttempt` and with the
/// narrowing goal, for the model to take it out of its sentence, and its
/// pending repair is marked so. The ledger keeps the sentences
/// left beside each cut for the section's flow repair, and records the
/// statements' cuts as the policy's step. A claim whose text cannot be taken
/// out, because it is the whole of its field or sits in a code block, ends
/// removed with its section, which goes out of the lesson (claim classes
/// design §3.10). Returns how many claims it dealt with, or the error that
/// stops the lesson when the cuts leave no claim of it standing.
pub(in crate::lessons::content_verification) async fn last_attempts(
    policy: &dyn AcceptancePolicy,
    context: &mut Value,
    ledger: &mut ClaimLedger,
) -> Result<usize> {
    ledger.sync().await?;
    let dropped = withdraw_resolved(context, ledger, policy);
    if !dropped.is_empty() {
        ledger.supersede(&dropped).await?;
    }
    let mut lesson = context
        .get("candidate")
        .cloned()
        .ok_or_else(|| invalid("Missing repair candidate."))?;
    let before = lesson.clone();
    let cuts_before = ledger.cuts.clone();
    let (mut taken, mut refused, mut narrowed) = (Vec::new(), Vec::new(), Vec::new());
    // The lesson and its cuts once the examples' cuts are made and the
    // statements' are not.
    let mut after_examples = None;
    for (key, example) in [("exampleClaims", true), ("failedClaims", false)] {
        for id in last(context, key) {
            let Some(place) = ledger.entry(&id).map(|entry| entry.location().clone()) else {
                continue;
            };
            let pointer = field_pointer(&lesson, place.unit_index, &place.field);
            let text = pointer
                .as_deref()
                .and_then(|pointer| lesson.pointer(pointer))
                .and_then(Value::as_str)
                .map(str::to_owned);
            let decision = text
                .as_deref()
                .filter(|_| !example)
                .map(|text| ledger.cut_of(&id, text));
            if matches!(decision, Some(CutOf::Narrow)) {
                narrowed.push(id);
                continue;
            }
            let cut = text.as_deref().map_or(Cut::Absent, |text| {
                let stated = match decision {
                    Some(CutOf::Sentence(sentence)) => Some(sentence),
                    _ => None,
                };
                out(example, text, stated.unwrap_or(&place.quote))
            });
            let label = unit_label(&before, place.unit_index);
            match cut {
                Cut::Done(after) => {
                    if let (Some(text), Some(label)) = (text.as_deref(), label) {
                        let lost = !example && place.field == "body" && answer_lost(text, &after);
                        let places = if example {
                            gap_around_code(&place.field, text, &after)
                        } else {
                            gaps(&place.field, text, &after)
                        };
                        ledger.cuts.take(place.unit_index, label, (places, lost));
                    }
                    if let Some(slot) = pointer
                        .as_deref()
                        .and_then(|pointer| lesson.pointer_mut(pointer))
                    {
                        *slot = Value::String(after);
                    }
                    taken.push(id);
                }
                Cut::Absent => taken.push(id),
                Cut::Refused => {
                    if let Some(label) = label {
                        ledger.cuts.go(place.unit_index, label);
                    }
                    refused.push(id);
                }
            }
        }
        if example {
            after_examples = Some((lesson.clone(), ledger.cuts.clone()));
        }
    }
    // A narrowed statement is not cut: the model takes it out of its sentence
    // in this repair, and the pass after it records what became of it.
    if !narrowed.is_empty() {
        ledger.mark_narrowing(&narrowed).await?;
        ask_narrowing(context, &narrowed);
    }
    // A claim whose text went with a cut sentence is no work any more: the
    // pass after the repair finds it gone.
    let cut_by = |ledger: &ClaimLedger, lesson: &Value, taken: &[ClaimId], refused: &[ClaimId]| {
        let vanished = |id: &ClaimId| {
            ledger.entry(id).is_some_and(|entry| {
                let place = entry.location();
                let holds = |whole: &Value| {
                    field_text(whole, place.unit_index, &place.field)
                        .is_some_and(|text| text.contains(&place.quote))
                };
                holds(&before) && !holds(lesson)
            })
        };
        ledger
            .entries()
            .iter()
            .map(|entry| entry.claim_id())
            .filter(|id| taken.contains(id) || refused.contains(id) || vanished(id))
            .cloned()
            .collect::<Vec<ClaimId>>()
    };
    let mut cut = cut_by(ledger, &lesson, &taken, &refused);
    // Cuts that would leave the lesson unpublishable are not made: the
    // statements keep their sentences and are published not verified. The
    // examples' own cuts are made, unless they alone would leave it so; an
    // example is never marked.
    let statements: Vec<ClaimId> = taken
        .iter()
        .chain(&refused)
        .filter(|id| ledger.is_statement(id))
        .cloned()
        .collect();
    let mut marked = 0;
    if !statements.is_empty()
        && (unpublishable(&lesson, &ledger.cuts).is_some()
            || (refused.is_empty() && ledger.nothing_left(&cut)))
    {
        let example_ids: Vec<ClaimId> = taken
            .iter()
            .chain(&refused)
            .filter(|id| !statements.contains(id))
            .cloned()
            .collect();
        let (lesson_ex, cuts_ex) =
            after_examples.unwrap_or_else(|| (before.clone(), cuts_before.clone()));
        let all_cuts = std::mem::replace(&mut ledger.cuts, cuts_ex);
        let examples_refused = refused.iter().any(|id| !statements.contains(id));
        let examples_stop = !example_ids.is_empty()
            && (unpublishable(&lesson_ex, &ledger.cuts).is_some()
                || (!examples_refused && ledger.nothing_left(&example_ids)));
        if examples_stop {
            ledger.cuts = all_cuts;
        } else {
            taken.retain(|id| !statements.contains(id));
            refused.retain(|id| !statements.contains(id));
            lesson = lesson_ex;
            withdraw(context, |id| statements.contains(id));
            ledger.drop_repairs(&statements).await?;
            ledger.save_cuts().await?;
            marked = ledger.publish_unverified(&statements, None).await?;
            cut = cut_by(ledger, &lesson, &taken, &refused);
        }
    }
    // A claim in a section that goes is no work either: the section's
    // statements leave the checks and end removed with it.
    let goes = |id: &ClaimId| {
        ledger
            .entry(id)
            .is_some_and(|entry| ledger.cuts.going.contains_key(&entry.location().unit_index))
    };
    withdraw(context, |id| cut.contains(id) || goes(id));
    if let Some(slot) = context.get_mut("candidate") {
        *slot = lesson;
    }
    // A statement's cut, taken or refused, is the policy's step, which ends it
    // removed; an example's refused cut ends it removed with its section, and
    // its cut is found by the pass after.
    let (cut_statements, _): (Vec<ClaimId>, Vec<ClaimId>) = taken
        .iter()
        .cloned()
        .partition(|id| ledger.is_statement(id));
    let (refused_statements, examples): (Vec<ClaimId>, Vec<ClaimId>) = refused
        .iter()
        .cloned()
        .partition(|id| ledger.is_statement(id));
    if !examples.is_empty() {
        ledger.remove(&examples).await?;
    }
    if !refused_statements.is_empty() {
        ledger
            .cut(policy, &refused_statements, Outcome::CannotCut)
            .await?;
    }
    if !cut_statements.is_empty() {
        ledger.cut(policy, &cut_statements, Outcome::Cut).await?;
    }
    if !taken.is_empty() || !refused.is_empty() {
        ledger.save_cuts().await?;
    }
    if refused.is_empty() && !taken.is_empty() && ledger.nothing_left(&cut) {
        let text = context
            .get("candidate")
            .map_or_else(String::new, Value::to_string);
        return Err(ledger.stop_cut_out(&cut, &text).await?);
    }
    Ok(taken.len() + refused.len() + marked)
}

#[cfg(test)]
#[path = "cut_tests.rs"]
mod tests;
