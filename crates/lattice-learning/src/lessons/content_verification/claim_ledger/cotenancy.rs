//! Which statements stand on one sentence: a rewrite of the sentence changes
//! them all (claim classes design §3.2), and what the cut of one of them takes
//! out of it depends on who else stands on its sentence (acceptance design
//! §2.3, §2.6).

use super::fields::field_text;
use super::located::{located, located_quote};
use super::{is_example, stand_together, ClaimLedger, Place};
use lattice_core::domain::learning::acceptance::Goal;
use lattice_core::domain::learning::claim_review::{
    span_of, ClaimEvent, ClaimId, ClaimLedgerEntry, RepairOutcome,
};
use lattice_core::shared::text::sentences;
use serde_json::Value;

/// What the cut of a statement takes out of its field. `ClaimLedger::cut_of`
/// decides it once, and the repair's request, its records and the last attempt
/// all read that decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::lessons::content_verification) enum CutOf<'t> {
    /// The sentence that states the statement, every copy of it: no other live
    /// statement stands on it, so the code cuts it.
    Sentence(&'t str),
    /// One narrowing rewrite by the model takes the statement out of its
    /// sentence and keeps the others; nothing is cut this round.
    Narrow,
    /// No cut is made: the statement is unknown, or its narrowing was asked and
    /// not applied. The statement ends Unverified; when it stands in a question
    /// the lesson can lose, its question goes instead.
    Refused,
}

/// Whether a narrowing rewrite of the statement of `entry` was asked and not
/// applied.
fn narrowing_spent(entry: &ClaimLedgerEntry) -> bool {
    entry.events().iter().any(|record| {
        matches!(
            record.event,
            ClaimEvent::RepairAttempted {
                goal: Some(Goal::KeepTheOthers),
                outcome: RepairOutcome::Rejected { .. },
                ..
            }
        )
    })
}

impl ClaimLedger {
    /// The live claims standing on the same sentence as `claim_id`: those in
    /// its unit and field whose quote holds its quote or is held by it. A
    /// rewrite of the sentence changes them all (claim classes design §3.2).
    pub(in crate::lessons::content_verification) fn cotenants(
        &self,
        claim_id: &ClaimId,
    ) -> Vec<ClaimId> {
        let Some(entry) = self.entry(claim_id) else {
            return Vec::new();
        };
        let here = Place::of(entry.location());
        self.entries
            .iter()
            .filter(|other| other.claim_id() != claim_id && !other.state().is_gone())
            .filter(|other| stand_together(&here, &Place::of(other.location())))
            .map(|other| other.claim_id().clone())
            .collect()
    }

    /// Whether a live statement other than `claim_id` stands on `sentence`, a
    /// sentence of `text`, the text of `claim_id`'s field: the sentence its own
    /// statement names in its quote is this one, holds it or is held by it. A
    /// cut of the sentence takes that statement's words out with it. A code
    /// example's claim stands on its code, not on a sentence.
    fn shares_sentence(&self, claim_id: &ClaimId, text: &str, sentence: &str) -> bool {
        let Some(entry) = self.entry(claim_id) else {
            return false;
        };
        let (unit, field) = (entry.location().unit_index, entry.location().field.as_str());
        let here = Place {
            unit,
            field,
            quote: sentence,
        };
        self.entries
            .iter()
            .filter(|other| other.claim_id() != claim_id)
            .filter(|other| !other.state().is_gone() && !is_example(other))
            .filter(|other| other.location().unit_index == unit && other.location().field == field)
            .any(|other| {
                let own = located_quote(other.location(), other.statement(), text);
                stand_together(
                    &here,
                    &Place {
                        unit,
                        field,
                        quote: &own,
                    },
                )
            })
    }

    /// What the cut of the statement `claim_id` takes out of `text`, the text
    /// of its field (acceptance design §2.3, §2.6): the sentence it stands
    /// on, which `located` gives, when that is one sentence of the field that
    /// no other live statement stands on. Otherwise, while its
    /// narrowing is unspent, one narrowing rewrite; and once a narrowing was
    /// asked and not applied, no cut at all. The one decision
    /// the repair's records and request and the last attempt all read.
    pub(in crate::lessons::content_verification) fn cut_of<'t>(
        &self,
        claim_id: &ClaimId,
        text: &'t str,
    ) -> CutOf<'t> {
        let Some(entry) = self.entry(claim_id) else {
            return CutOf::Refused;
        };
        let place = entry.location();
        let (sentence, span) = located(place, entry.statement(), text);
        let alone = Some(sentence.as_str())
            .filter(|sentence| sentences(sentence).len() == 1)
            .and_then(|sentence| {
                span_of(text, sentence)
                    .or(span.filter(|&(start, end)| text.get(start..end) == Some(sentence)))
            })
            .and_then(|(start, end)| text.get(start..end))
            .filter(|cut| !self.shares_sentence(claim_id, text, cut));
        match alone {
            Some(sentence) => CutOf::Sentence(sentence),
            None if narrowing_spent(entry) => CutOf::Refused,
            None => CutOf::Narrow,
        }
    }

    /// Whether the cut of the statement `claim_id` is a narrowing rewrite in
    /// the lesson `candidate`: `cut_of` of the text of its field.
    pub(in crate::lessons::content_verification) fn narrows(
        &self,
        candidate: &Value,
        claim_id: &ClaimId,
    ) -> bool {
        let Some(place) = self.entry(claim_id).map(ClaimLedgerEntry::location) else {
            return false;
        };
        field_text(candidate, place.unit_index, &place.field)
            .is_some_and(|text| matches!(self.cut_of(claim_id, &text), CutOf::Narrow))
    }

    /// The sentences of the claims still standing, each with its section, but
    /// `claims`: what the lesson keeps of its sentences when these claims lose
    /// theirs.
    pub(in crate::lessons::content_verification) fn live_sentences_but(
        &self,
        claims: &[ClaimId],
    ) -> Vec<(usize, String)> {
        self.entries
            .iter()
            .filter(|entry| !entry.state().is_gone() && !claims.contains(entry.claim_id()))
            .map(|entry| (entry.location().unit_index, entry.location().quote.clone()))
            .collect()
    }

    /// `claims` and the claims standing on their sentences.
    pub(in crate::lessons::content_verification) fn with_cotenants(
        &self,
        claims: &[ClaimId],
    ) -> Vec<ClaimId> {
        let mut all = claims.to_vec();
        for id in claims {
            for mate in self.cotenants(id) {
                if !all.contains(&mate) {
                    all.push(mate);
                }
            }
        }
        all
    }
}
