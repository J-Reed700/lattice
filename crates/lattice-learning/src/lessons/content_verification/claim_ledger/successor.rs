//! Which claim continues a repaired one (claim sources design §9). A
//! repaired claim's id continues only in new text over the part of its
//! field the repair changed, never in a sentence that was there before, and a
//! narrowed claim's in nothing the narrowing wrote. No claim is born of a
//! repair: what else it wrote is put back before the pass records anything
//! (acceptance design §2.6).

use super::fields::field_text;
use super::RepairRecord;
use crate::reference_collection::shared_terms;
use lattice_core::domain::learning::acceptance::Walked;
use lattice_core::domain::learning::claim_review::{ClaimId, ClaimLedgerEntry, ExtractedClaim};
use lattice_core::shared::text::sentences;
use serde_json::Value;
use std::collections::HashMap;

/// What the repairs of one pass left: each orphaned target's successor (an
/// index into the extracted claims), and the changed text of the fields
/// whose repaired claim found no successor.
#[derive(Debug, Default)]
pub(super) struct Heirs {
    pub(super) successors: Vec<(ClaimId, usize)>,
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

/// Whether `claim` is text a repair wrote in `field` of section `unit`: not
/// in the field as it was `before`, and in the sentences `region` of the
/// repaired field that the repair changed.
fn written(
    claim: &ExtractedClaim,
    unit: usize,
    field: &str,
    before: &str,
    (start, end): (usize, usize),
) -> bool {
    claim.location.unit_index == unit
        && claim.location.field == field
        && !before.contains(&claim.location.quote)
        && claim
            .location
            .span
            .is_some_and(|(from, to)| from < end && to > start)
}

/// What the last step leaves for the pass that reads the lesson: whether
/// that pass comes after another, the lesson as it was before the repairs or
/// edits, the place in its graph of each statement the ledger holds, by
/// section and statement, and the claims the ledger holds that are not ended.
#[derive(Default)]
pub(in crate::lessons::content_verification) struct Rewrites {
    later: bool,
    previous: Option<Value>,
    walked: HashMap<(usize, String), Walked>,
    held: Vec<ClaimLedgerEntry>,
}

impl Rewrites {
    pub(in crate::lessons::content_verification) fn new(
        later: bool,
        previous: Option<Value>,
        walked: HashMap<(usize, String), Walked>,
    ) -> Self {
        Self {
            later,
            previous,
            walked,
            held: Vec::new(),
        }
    }

    /// These with `held`, the claims the ledger holds that are not ended:
    /// an extraction that misses one whose sentence stands keeps it.
    pub(in crate::lessons::content_verification) fn holding(
        mut self,
        held: Vec<ClaimLedgerEntry>,
    ) -> Self {
        self.held = held;
        self
    }

    /// The claims the ledger holds that are not ended.
    pub(in crate::lessons::content_verification) fn held(
        &self,
    ) -> &[ClaimLedgerEntry] {
        &self.held
    }

    /// Where the ledger has `statement` of section `unit` in its class's
    /// graph, when it holds it and the claim has a class.
    pub(in crate::lessons::content_verification) fn walked(
        &self,
        unit: usize,
        statement: &str,
    ) -> Option<Walked> {
        self.walked.get(&(unit, statement.to_owned())).copied()
    }

    /// Whether this pass is the first to read `claim`: the first pass reads
    /// the whole lesson, a later one only the sentences written since the
    /// last, by a repair or by the user.
    pub(in crate::lessons::content_verification) fn is_new(
        &self,
        current: &Value,
        claim: &ExtractedClaim,
    ) -> bool {
        if !self.later {
            return true;
        }
        let (unit, field) = (claim.location.unit_index, claim.location.field.as_str());
        let Some(after) = field_text(current, unit, field) else {
            return false;
        };
        let Some(before) = self
            .previous
            .as_ref()
            .and_then(|previous| field_text(previous, unit, field))
        else {
            return false;
        };
        let region = sentence_region(&after, changed_region(&before, &after));
        written(claim, unit, field, &before, region)
    }
}

/// The heirs of `repairs` (claims orphaned by the identity rules): for each,
/// the unowned extracted claims of its field whose quote is new text in the
/// changed region (claim sources design §18.4 iv). The one sharing most words
/// with the replaced sentence continues it. A narrowing takes its claim out of
/// its sentence, so nothing it wrote continues a narrowed claim.
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
        let qualifies =
            |claim: &ExtractedClaim| written(claim, record.unit, &field, &before, (start, end));
        let successor = extracted
            .iter()
            .enumerate()
            .filter(|_| !record.narrowing)
            .filter(|(index, claim)| taken.get(*index) == Some(&false) && qualifies(claim))
            .min_by_key(|(_, claim)| {
                (
                    std::cmp::Reverse(shared_terms(&claim.statement, &record.before)),
                    claim.location.span.map_or(usize::MAX, |(from, _)| from),
                )
            })
            .map(|(index, _)| index);
        match successor {
            Some(successor) => {
                if let Some(slot) = taken.get_mut(successor) {
                    *slot = true;
                }
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
            rides_on: None,
            step: None,
            narrowing: false,
        }
    }

    const QUOTED: &str = "Source 0 states: the schema names each argument.";
    const REQUIRED: &str = "The `required` array lists the arguments a call must give.";

    #[test]
    fn a_repaired_claims_id_stays_with_its_own_rewritten_sentence() {
        // The quotation claim is repaired; the field also holds the `required`
        // sentence verbatim from before, a claim no one owned. The id goes to the
        // sentence the repair wrote, never to it.
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
    fn a_repaired_sentence_among_verified_ones_continues_its_claim_alone() {
        // One passage of three located claims, V1, F and V2; the repair
        // rewrote F (claim sources design §18.4 iv).
        const V1: &str =
            "Mitochondria produce most of a cell's ATP through oxidative phosphorylation.";
        const F: &str = "A typical human cell holds exactly twelve mitochondria.";
        const V2: &str = "Their inner membrane folds into cristae that enlarge its surface.";
        let fixed = "A human cell holds hundreds to thousands of mitochondria.";
        let before = format!("{V1} {F} {V2}");
        let after = format!("{V1} {fixed} {V2}");
        let found = |claims: &[ExtractedClaim]| {
            heirs(
                &[record("f", F, 1)],
                &[ClaimId("f".into())],
                (claims, &[false, false, false]),
                (Some(&lesson(&before)), &lesson(&after)),
                |_| Some("body".into()),
                field_text,
            )
        };
        // Even unowned, the verified sentences stood before the repair: they
        // are not its successor.
        let kept = found(&[
            extracted(&after, V1),
            extracted(&after, fixed),
            extracted(&after, V2),
        ]);
        assert_eq!(kept.successors, vec![(ClaimId("f".into()), 1)]);

        // A successor the locator could not narrow, located by its whole new
        // passage, reaches past the rewritten sentence and still continues it.
        let by_passage = ExtractedClaim {
            location: ClaimLocation::locate(0, UnitKind::Block, "body", after.as_str(), &after),
            statement: fixed.into(),
        };
        let kept = found(&[extracted(&after, V1), by_passage, extracted(&after, V2)]);
        assert_eq!(kept.successors, vec![(ClaimId("f".into()), 1)]);
    }

    #[test]
    fn a_narrowings_claim_continues_in_nothing_the_narrowing_wrote() {
        // A narrowing takes its claim out of the sentence and keeps the rest:
        // what it wrote there is never the claim said anew.
        let before = format!("{QUOTED} {REQUIRED}");
        let kept = "A tool schema lists each argument by name.";
        let after = format!("{kept} {REQUIRED}");
        let claims = [extracted(&after, kept), extracted(&after, REQUIRED)];
        let narrowing = RepairRecord {
            narrowing: true,
            ..record("quoted", QUOTED, 2)
        };

        let heirs = heirs(
            &[narrowing],
            &[ClaimId("quoted".into())],
            (&claims, &[false, true]),
            (Some(&lesson(&before)), &lesson(&after)),
            |_| Some("body".into()),
            field_text,
        );

        assert!(heirs.successors.is_empty(), "{:?}", heirs.successors);
        assert_eq!(
            heirs.rewritten.get(&ClaimId("quoted".into())),
            Some(&Some(kept.to_owned()))
        );
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
