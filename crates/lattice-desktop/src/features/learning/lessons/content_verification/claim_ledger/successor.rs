//! Which claim continues a repaired one (claim sources design §9). A
//! repaired claim's id continues only in new text inside the part of its
//! field the repair changed, never in a sentence that was there before; the
//! other claims that repair wrote are its children and start with what the
//! parent had left, so a rewrite or a split sentence adds no attempts.

use super::RepairRecord;
use crate::features::learning::lessons::content_verification::repair::sentences;
use crate::features::learning::reference_collection::shared_terms;
use lattice_core::domain::learning::claim_review::{ClaimId, ExtractedClaim, MACHINE_ATTEMPTS};
use serde_json::Value;
use std::collections::HashMap;

/// What the repairs of one pass left: each orphaned target's successor (an
/// index into the extracted claims), the children born of each repair, and
/// the changed text of the fields whose repaired claim found no successor.
#[derive(Debug, Default)]
pub(super) struct Heirs {
    pub(super) successors: Vec<(ClaimId, usize)>,
    /// Extracted claim index → its parent and the attempts it inherits.
    pub(super) children: HashMap<usize, (ClaimId, u8)>,
    /// A target with no successor → the text its repair wrote in its field
    /// (`None` when the repair only took text out).
    pub(super) rewritten: HashMap<ClaimId, Option<String>>,
}

/// The bytes of `current` a change from `previous` touched: between their
/// common prefix and common suffix, on character boundaries. Empty when the
/// change only removed text (or nothing changed).
pub(super) fn changed_region(previous: &str, current: &str) -> (usize, usize) {
    let prefix: usize = previous
        .chars()
        .zip(current.chars())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8())
        .sum();
    let room = previous.len().min(current.len()) - prefix;
    let mut suffix = 0;
    for (a, b) in previous.chars().rev().zip(current.chars().rev()) {
        if a != b || suffix + a.len_utf8() > room {
            break;
        }
        suffix += a.len_utf8();
    }
    (prefix, current.len() - suffix)
}

/// The region widened to the whole sentences of `current` it touches: the
/// text a claim in the changed part was extracted from.
fn sentence_region(current: &str, (start, end): (usize, usize)) -> (usize, usize) {
    sentences(current)
        .into_iter()
        .filter(|&(from, to)| from < end.max(start + 1) && to > start)
        .fold((start, end), |(lo, hi), (from, to)| {
            (lo.min(from), hi.max(to))
        })
}

/// The heirs of `repairs` (claims `orphaned` by the identity rules only):
/// for each, the unowned extracted claims of its field whose quote is new
/// text inside the field's changed region; the one sharing most words with
/// the sentence the repair replaced continues it (ties to the earliest), the
/// rest are its children. `previous` is the lesson before the repairs,
/// `current` after; `field_of` names a claim's field; `field_text` reads one.
pub(super) fn heirs(
    repairs: &[RepairRecord],
    orphaned: &[ClaimId],
    (extracted, owned): (&[ExtractedClaim], &[bool]),
    (previous, current): (Option<&Value>, &Value),
    field_of: impl Fn(&ClaimId) -> Option<String>,
    field_text: impl Fn(&Value, usize, &str) -> Option<String>,
) -> Heirs {
    let mut heirs = Heirs::default();
    let mut taken: Vec<bool> = owned.to_vec();
    for record in repairs.iter().filter(|r| orphaned.contains(&r.claim_id)) {
        let Some(field) = field_of(&record.claim_id) else {
            continue;
        };
        let before = previous.and_then(|previous| field_text(previous, record.unit, &field));
        let after = field_text(current, record.unit, &field);
        let (Some(before), Some(after)) = (before, after) else {
            continue;
        };
        let region = changed_region(&before, &after);
        let (start, end) = sentence_region(&after, region);
        let qualifies = |claim: &ExtractedClaim| {
            claim.location.unit_index == record.unit
                && claim.location.field == field
                && !before.contains(&claim.location.quote)
                && claim
                    .location
                    .span
                    .is_some_and(|(from, to)| from >= start && to <= end)
        };
        let mut found: Vec<usize> = extracted
            .iter()
            .enumerate()
            .filter(|(index, claim)| !taken[*index] && qualifies(claim))
            .map(|(index, _)| index)
            .collect();
        found.sort_by_key(|index| {
            let claim = &extracted[*index];
            (
                std::cmp::Reverse(shared_terms(&claim.statement, &record.before)),
                claim.location.span.map_or(usize::MAX, |(from, _)| from),
            )
        });
        let attempts_left = MACHINE_ATTEMPTS.saturating_sub(record.attempt);
        let mut found = found.into_iter();
        match found.next() {
            Some(successor) => {
                taken[successor] = true;
                heirs.successors.push((record.claim_id.clone(), successor));
            }
            None => {
                let written = after.get(start..end).filter(|_| region.0 < region.1);
                heirs.rewritten.insert(
                    record.claim_id.clone(),
                    written.map(|text| text.trim().to_owned()),
                );
            }
        }
        for child in found {
            taken[child] = true;
            heirs
                .children
                .insert(child, (record.claim_id.clone(), attempts_left));
        }
    }
    heirs
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::domain::learning::claim_review::{ClaimLocation, UnitKind};
    use serde_json::json;

    fn lesson(body: &str) -> Value {
        json!({ "blocks": [{ "kind": "explanation", "body": body }], "questions": [] })
    }

    fn field_text(lesson: &Value, unit: usize, field: &str) -> Option<String> {
        lesson
            .pointer(&format!("/blocks/{unit}/{field}"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    }

    fn extracted(body: &str, sentence: &str) -> ExtractedClaim {
        ExtractedClaim {
            location: ClaimLocation::locate(0, UnitKind::Block, "body", sentence, body),
            statement: sentence.into(),
        }
    }

    fn record(id: &str, before: &str, attempt: u8) -> RepairRecord {
        RepairRecord {
            claim_id: ClaimId(id.into()),
            unit: 0,
            attempt,
            escalated: false,
            before: before.into(),
        }
    }

    const QUOTED: &str = "Source 0 states: the schema names each argument.";
    const REQUIRED: &str = "The `required` array lists the arguments a call must give.";

    #[test]
    fn a_repaired_claims_id_stays_with_its_own_rewritten_sentence() {
        // The reported case: the quotation claim is repaired; the field also
        // holds the `required` sentence verbatim from before, a claim no one
        // owned. The id goes to the sentence the repair wrote, never to it.
        let before = format!("{QUOTED} {REQUIRED}");
        let rewritten = "A tool schema names each argument and its type.";
        let after = format!("{rewritten} {REQUIRED}");
        let claims = [extracted(&after, rewritten), extracted(&after, REQUIRED)];
        let kept = heirs(
            &[record("quoted", QUOTED, 1)],
            &[ClaimId("quoted".into())],
            (&claims, &[false, false]),
            (Some(&lesson(&before)), &lesson(&after)),
            |_| Some("body".into()),
            field_text,
        );
        assert_eq!(kept.successors, vec![(ClaimId("quoted".into()), 0)]);
        assert!(kept.children.is_empty(), "the old sentence is no child");

        // The repair took the sentence out and wrote nothing: removed.
        let claims = [extracted(REQUIRED, REQUIRED)];
        let heirs_of_removal = heirs(
            &[record("quoted", QUOTED, 1)],
            &[ClaimId("quoted".into())],
            (&claims, &[false]),
            (Some(&lesson(&before)), &lesson(REQUIRED)),
            |_| Some("body".into()),
            field_text,
        );
        assert!(heirs_of_removal.successors.is_empty());
        assert_eq!(
            heirs_of_removal.rewritten.get(&ClaimId("quoted".into())),
            Some(&None)
        );

        // It rewrote the sentence into one no claim continues: rewritten,
        // with the text it wrote.
        let opinion = "Schemas are a matter of taste.";
        let after = format!("{opinion} {REQUIRED}");
        let claims = [extracted(&after, REQUIRED)];
        let heirs_of_rewrite = heirs(
            &[record("quoted", QUOTED, 1)],
            &[ClaimId("quoted".into())],
            (&claims, &[false]),
            (Some(&lesson(&before)), &lesson(&after)),
            |_| Some("body".into()),
            field_text,
        );
        assert!(heirs_of_rewrite.successors.is_empty());
        assert_eq!(
            heirs_of_rewrite.rewritten.get(&ClaimId("quoted".into())),
            Some(&Some(opinion.to_owned()))
        );
    }

    #[test]
    fn children_of_a_repair_inherit_the_parents_remaining_attempts() {
        // A failed sentence rewritten into two: the one sharing most words
        // with it continues the claim, the other is its child, with what the
        // parent has left after this attempt.
        let before = format!("{QUOTED} {REQUIRED}");
        let first = "The schema names each argument.";
        let second = "Each argument also has a type.";
        let after = format!("{first} {second} {REQUIRED}");
        let claims = [
            extracted(&after, first),
            extracted(&after, second),
            extracted(&after, REQUIRED),
        ];
        let heirs = heirs(
            &[record("quoted", QUOTED, 1)],
            &[ClaimId("quoted".into())],
            (&claims, &[false, false, true]),
            (Some(&lesson(&before)), &lesson(&after)),
            |_| Some("body".into()),
            field_text,
        );
        assert_eq!(heirs.successors, vec![(ClaimId("quoted".into()), 0)]);
        assert_eq!(
            heirs.children.get(&1),
            Some(&(ClaimId("quoted".into()), MACHINE_ATTEMPTS - 1))
        );
        assert_eq!(heirs.children.len(), 1);
    }

    #[test]
    fn the_changed_region_is_between_the_common_prefix_and_suffix() {
        let before = "Heat it. Water boils at 100 °C. Then cool it.";
        let after = "Heat it. Water boils at 100 °C at sea level. Then cool it.";
        let (start, end) = changed_region(before, after);
        assert_eq!(&after[start..end], " at sea level");
        assert_eq!(sentence_region(after, (start, end)), (9, 45));
        assert_eq!(changed_region(before, before), (before.len(), before.len()));
        let removed = "Heat it. Then cool it.";
        let (start, end) = changed_region(before, removed);
        assert_eq!(start, end, "a removal writes no text");
    }
}
