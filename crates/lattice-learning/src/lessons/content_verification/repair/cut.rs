//! The machine's last attempt on a claim takes its text out of the lesson
//! (claim classes design §3.2): the sentence-level edit a repair makes, made
//! here, whole, in no request to the model, unless the claim's sentence says
//! more than it: then the model is asked to take the claim out of it (acceptance
//! design §2.3, §2.6).
use super::super::claim_ledger::{field_pointer, field_text, ClaimLedger, CutOf};
use super::super::repair_work::{withdraw, withdraw_resolved};
use super::super::sections::{
    answer_lost, gap_around_code, gaps, left_with_too_little, restore_units, unit_label,
    unpublishable,
};
use super::super::*;
use super::goal_words;
use lattice_core::shared::text::same_text::{overlaps, stands};
use crate::lessons::shape::ShapePolicy;
use lattice_core::domain::learning::acceptance::{AcceptancePolicy, Goal, Outcome};
use lattice_core::domain::learning::claim_review::{ClaimId, CutPlace};
use lattice_core::shared::text::segmentation::{code_fences, in_code_fence};
use lattice_core::shared::text::{item_marker_end, sentences};

/// What taking a claim's text out of its field comes to.
enum Cut {
    /// The field without the text.
    Done(String),
    /// The text is not in the field: it is out already.
    Absent,
    /// The field would be left empty, or a code block broken.
    Refused,
}

/// The separator that keeps more line breaks, the first on a tie.
fn stronger<'t>(first: &'t str, second: &'t str) -> &'t str {
    if second.matches('\n').count() > first.matches('\n').count() {
        second
    } else {
        first
    }
}

/// The list marker and the whitespace after it that open the sentence at
/// `span`, when that sentence opens its line.
fn marker_of(text: &str, span: (usize, usize)) -> Option<&str> {
    let lead = text.get(..span.0)?.rsplit('\n').next()?;
    let sentence = text.get(span.0..span.1)?;
    sentence
        .get(..item_marker_end(sentence)?)
        .filter(|_| lead.trim().is_empty())
}

/// The sentences of `text` the first occurrence of `quote` touches, as indices
/// into `spans`, with that occurrence; `None` when the quote is blank or absent.
fn touched(
    text: &str,
    spans: &[(usize, usize)],
    quote: &str,
) -> Option<((usize, usize), Vec<usize>)> {
    let at = text.find(quote).filter(|_| !quote.trim().is_empty())?;
    let found = (at, at + quote.len());
    let touching = spans
        .iter()
        .enumerate()
        .filter(|(_, span)| overlaps(**span, found))
        .map(|(index, _)| index)
        .collect();
    Some((found, touching))
}

/// `text` without the sentences the first occurrence of `quote` touches; the
/// sentences left keep the separators a paragraph break needs, and the first
/// sentence kept on the line of a cut list item takes the item's marker.
fn sentence(text: &str, quote: &str) -> Cut {
    let spans = sentences(text);
    let Some((found, touching)) = touched(text, &spans, quote) else {
        return Cut::Absent;
    };
    if !code_fences(quote).is_empty() || in_code_fence(text, found.0) {
        return Cut::Refused;
    }
    if touching.is_empty() {
        return Cut::Absent;
    }
    if touching.len() == spans.len() {
        return Cut::Refused;
    }
    let gone = |index: usize| touching.contains(&index);
    let first = spans.first().map_or(0, |span| span.0);
    let mut out = text.get(..first).unwrap_or_default().to_owned();
    // The separator before the next sentence kept: the strongest of those
    // from the last one kept up to it.
    let mut between: Option<&str> = None;
    let mut marker: Option<&str> = None;
    for (index, &(start, end)) in spans.iter().enumerate() {
        let next = spans.get(index + 1).map_or(text.len(), |span| span.0);
        let separator = text.get(end..next).unwrap_or_default();
        if gone(index) {
            between = between.map(|kept| stronger(kept, separator));
            marker = marker_of(text, (start, end)).or(marker);
            if separator.contains('\n') {
                marker = None;
            }
            continue;
        }
        out.push_str(between.unwrap_or_default());
        out.push_str(marker.take().unwrap_or_default());
        out.push_str(text.get(start..end).unwrap_or_default());
        between = Some(separator);
    }
    out.push_str(text.get(text.trim_end().len()..).unwrap_or_default());
    Cut::Done(out)
}

/// What cutting `quote` out of `text` takes, in the first place it occurs
/// (every sentence it touches), and the sentences kept on either side.
pub(super) fn cut_place(text: &str, quote: &str) -> Option<CutPlace> {
    let spans = sentences(text);
    let (_, touching) = touched(text, &spans, quote)?;
    let (first, last) = (*touching.first()?, *touching.last()?);
    let kept = |index: Option<usize>| {
        let &(start, stop) = spans.get(index?)?;
        text.get(start..stop).map(str::to_owned)
    };
    let opening = spans.get(first)?;
    let kept_marker = marker_of(text, *opening).filter(|_| {
        kept(last.checked_add(1)).is_some()
            && (first..=last).all(|index| {
                let next = spans.get(index + 1).map_or(text.len(), |span| span.0);
                let end = spans.get(index).map_or(0, |span| span.1);
                !text.get(end..next).unwrap_or_default().contains('\n')
            })
    });
    Some(CutPlace {
        sentence: text
            .get(opening.0 + kept_marker.map_or(0, str::len)..spans.get(last)?.1)?
            .to_owned(),
        before: kept(first.checked_sub(1)),
        after: kept(last.checked_add(1)),
    })
}

/// `text` without the fenced block whose code is `code`.
fn fence(text: &str, code: &str) -> Cut {
    let Some(at) = text.find(code).filter(|_| !code.trim().is_empty()) else {
        return Cut::Absent;
    };
    let block = code_fences(text)
        .into_iter()
        .find(|block| block.text == (at..at + code.len()));
    let Some((start, stop)) =
        block.and_then(|block| Some((block.opening.start, block.closing?.end)))
    else {
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
/// carried: the sentence that states it, in the first place the lesson writes
/// it, when no other live statement stands on it. Otherwise nothing is
/// cut: the statement stays among the claims sent, without `lastAttempt` and
/// with the narrowing goal, for the model to take it out of its sentence, and
/// its pending repair is marked so; once that narrowing was not applied, the
/// sentence stays with its other statements and the cut is refused. The ledger
/// keeps the sentences left beside each cut for the section's flow repair, and
/// records the statements' cuts as the policy's step. A statement whose text
/// cannot be taken out, because it is the whole of its field or sits in a code
/// block, takes its question out of the lesson when it stands in one, and ends
/// removed with it. In a section, or when the lesson cannot lose the question,
/// that statement stays in the lesson and is published not verified, as does
/// one whose cut would leave its section with too little to teach (the other
/// cuts are made); only a failed example's refused cut takes its section out
/// (claim classes design 3.10). Returns how many claims it dealt with, or the
/// error that stops the lesson when the cuts leave no claim of it standing.
pub(in crate::lessons::content_verification) async fn last_attempts(
    policy: &dyn AcceptancePolicy,
    shape: &dyn ShapePolicy,
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
    let sections = before
        .get("blocks")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    // The statements whose refused cut takes their question out, and the
    // questions that go only for this pass.
    let mut with_question = Vec::new();
    let mut asked_to_go = Vec::new();
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
            let label = unit_label(&before, place.unit_index);
            let in_question = place.unit_index >= sections;
            let decision = text
                .as_deref()
                .filter(|_| !example)
                .map(|text| ledger.cut_of(&id, text));
            if matches!(decision, Some(CutOf::Narrow)) {
                narrowed.push(id);
                continue;
            }
            if matches!(decision, Some(CutOf::Whole)) && ledger.is_statement(&id) {
                if let (true, Some(label)) = (in_question, label) {
                    if !ledger.cuts.going.contains_key(&place.unit_index) {
                        asked_to_go.push(place.unit_index);
                    }
                    ledger.cuts.go(place.unit_index, label);
                    with_question.push(id.clone());
                }
                refused.push(id);
                continue;
            }
            let cut = text.as_deref().map_or(Cut::Absent, |text| {
                let stated = match decision {
                    Some(CutOf::Sentence(sentence)) => Some(sentence),
                    _ => None,
                };
                out(example, text, stated.unwrap_or(&place.quote))
            });
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
                    if let (false, Some(text)) = (example, text.as_deref()) {
                        let quote = match decision {
                            Some(CutOf::Sentence(sentence)) => sentence,
                            _ => place.quote.as_str(),
                        };
                        if let Some(cut) = cut_place(text, quote) {
                            ledger.note_cut(&id, cut);
                        }
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
                    let statement = ledger.is_statement(&id);
                    if let (true, Some(label)) = (!statement || in_question, label) {
                        if statement && !ledger.cuts.going.contains_key(&place.unit_index) {
                            asked_to_go.push(place.unit_index);
                        }
                        ledger.cuts.go(place.unit_index, label);
                        if statement {
                            with_question.push(id.clone());
                        }
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
    // The questions marked to go keep their place when they would leave the
    // lesson unpublishable: the lesson loses none of them, and the cuts beside
    // them are made.
    if !asked_to_go.is_empty() && unpublishable(&lesson, &ledger.cuts, shape).is_some() {
        let mut kept = ledger.cuts.clone();
        for unit in &asked_to_go {
            kept.going.remove(unit);
            kept.unsettled.remove(unit);
        }
        if unpublishable(&lesson, &kept, shape).is_none() {
            ledger.cuts = kept;
            with_question.retain(|id| {
                ledger
                    .entry(id)
                    .is_some_and(|entry| !asked_to_go.contains(&entry.location().unit_index))
            });
        }
    }
    // A claim whose text went with a cut sentence is no work any more: the
    // pass after the repair finds it gone.
    let cut_by = |ledger: &ClaimLedger, lesson: &Value, taken: &[ClaimId], refused: &[ClaimId]| {
        let vanished = |id: &ClaimId| {
            ledger.entry(id).is_some_and(|entry| {
                let place = entry.location();
                let holds = |whole: &Value| {
                    field_text(whole, place.unit_index, &place.field)
                        .is_some_and(|text| stands(&text, &place.quote))
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
    // A statement whose cut leaves a section with too little to teach keeps
    // its sentence and is published not verified; the other statements' cuts
    // are made. The cuts that would leave the lesson unpublishable are held
    // back too. An example's cut is made, unless it alone would leave the
    // lesson so, and is never marked.
    let mut marked = 0;
    let (lesson_ex, cuts_ex) = after_examples
        .clone()
        .unwrap_or_else(|| (before.clone(), cuts_before.clone()));
    let little_from_examples = left_with_too_little(&lesson_ex, &cuts_ex, shape);
    let thinned: Vec<usize> = left_with_too_little(&lesson, &ledger.cuts, shape)
        .into_iter()
        .filter(|unit| !little_from_examples.contains(unit))
        .collect();
    let held: Vec<ClaimId> = taken
        .iter()
        .filter(|id| ledger.is_statement(id))
        .filter(|id| {
            ledger
                .entry(id)
                .is_some_and(|entry| thinned.contains(&entry.location().unit_index))
        })
        .cloned()
        .collect();
    if !held.is_empty() {
        restore_units(
            (&mut lesson, &mut ledger.cuts),
            (&lesson_ex, &cuts_ex),
            &thinned,
        );
        taken.retain(|id| !held.contains(id));
        withdraw(context, |id| held.contains(id));
        ledger.cut(policy, &held, Outcome::CannotCut).await?;
        ledger.save_cuts().await?;
        marked += held.len();
        cut = cut_by(ledger, &lesson, &taken, &refused);
    }
    let statements: Vec<ClaimId> = taken
        .iter()
        .chain(&refused)
        .filter(|id| ledger.is_statement(id))
        .cloned()
        .collect();
    if !statements.is_empty()
        && (unpublishable(&lesson, &ledger.cuts, shape).is_some()
            || (refused.is_empty() && ledger.nothing_left(&cut)))
    {
        let example_ids: Vec<ClaimId> = taken
            .iter()
            .chain(&refused)
            .filter(|id| !statements.contains(id))
            .cloned()
            .collect();
        let all_cuts = std::mem::replace(&mut ledger.cuts, cuts_ex);
        let examples_refused = refused.iter().any(|id| !statements.contains(id));
        let examples_stop = !example_ids.is_empty()
            && (unpublishable(&lesson_ex, &ledger.cuts, shape).is_some()
                || (!examples_refused && ledger.nothing_left(&example_ids)));
        if examples_stop {
            ledger.cuts = all_cuts;
        } else {
            taken.retain(|id| !statements.contains(id));
            refused.retain(|id| !statements.contains(id));
            lesson = lesson_ex;
            withdraw(context, |id| statements.contains(id));
            ledger.cut(policy, &statements, Outcome::CannotCut).await?;
            ledger.save_cuts().await?;
            marked += statements.len();
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
    let (mut cut_statements, _): (Vec<ClaimId>, Vec<ClaimId>) = taken
        .iter()
        .cloned()
        .partition(|id| ledger.is_statement(id));
    let (refused_statements, examples): (Vec<ClaimId>, Vec<ClaimId>) = refused
        .iter()
        .cloned()
        .partition(|id| ledger.is_statement(id));
    // A refused statement whose question goes is cut with it.
    let (with_it, refused_statements): (Vec<ClaimId>, Vec<ClaimId>) = refused_statements
        .into_iter()
        .partition(|id| with_question.contains(id));
    cut_statements.extend(with_it);
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
