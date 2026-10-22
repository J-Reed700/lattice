//! When a lesson's teaching review stops repairing. Repairs continue while
//! each recheck shows at least one earlier finding fixed. A finding that
//! survives two repairs stops being repaired; when a pass fixes none, every
//! remaining one stops. Stopped findings wait for the user, and the lesson
//! publishes: no teaching-review finding fails a lesson job.
//!
//! A finding is followed by its kind and place (`FindingKey`), never its
//! wording. Each repair is sent at most `PER_REPAIR` findings; the others
//! carry to the next pass.
use std::collections::{BTreeMap, BTreeSet};

use crate::features::learning::review_findings::{FindingKey, ReviewFinding, SetAside};

/// Findings sent to one repair, as many as one request handles well.
pub(super) const PER_REPAIR: usize = 6;

/// Repairs a finding may survive before it stops being repaired.
const REPAIRS_PER_FINDING: u8 = 2;

pub(super) enum Next {
    /// Nothing is left to repair; these findings wait for the user.
    Done(Vec<SetAside>),
    /// Repair `send`; `carried` waits for the next pass. Both are rechecked.
    Repair {
        send: Vec<ReviewFinding>,
        carried: Vec<ReviewFinding>,
    },
}

#[derive(Default)]
pub(super) struct Passes {
    /// Findings the user already has, for text that has not changed since.
    held: BTreeSet<FindingKey>,
    /// Repairs each finding was sent to.
    repairs: BTreeMap<FindingKey, u8>,
    /// The findings that were open when the last repair ran.
    open_at_repair: Option<BTreeSet<FindingKey>>,
    /// The latest pass's findings, one per place.
    latest: Vec<ReviewFinding>,
}

impl Passes {
    pub fn new(held: BTreeSet<FindingKey>) -> Self {
        Self {
            held,
            ..Self::default()
        }
    }

    /// The machine no longer repairs this finding.
    fn stopped(&self, key: &FindingKey) -> bool {
        self.held.contains(key)
            || self.repairs.get(key).copied().unwrap_or_default() >= REPAIRS_PER_FINDING
    }

    /// Holds these findings for the user: the machine does not repair them
    /// (a finding about a sentence a live claim stands on, §10.3).
    pub fn hold(&mut self, keys: impl IntoIterator<Item = FindingKey>) {
        self.held.extend(keys);
    }

    /// Whether a deterministic finding at `key` still blocks the content
    /// review: one the machine stopped on no longer does.
    pub fn repairable(&self, key: &FindingKey) -> bool {
        !self.stopped(key)
    }

    /// Decide on one pass's findings, in the order the review gave them.
    pub fn next(&mut self, findings: Vec<ReviewFinding>) -> Next {
        // One finding per place: two notes about one place are one finding.
        let mut current: Vec<ReviewFinding> = Vec::new();
        for finding in findings {
            match current
                .iter_mut()
                .find(|known| known.key() == finding.key())
            {
                Some(known) => {
                    if !known.note.contains(finding.note.as_str()) {
                        known.note = format!("{} {}", known.note, finding.note);
                    }
                    known.passage = known.passage.take().or(finding.passage);
                }
                None => current.push(finding),
            }
        }
        let keys: BTreeSet<_> = current.iter().map(ReviewFinding::key).collect();
        let fixed = self
            .open_at_repair
            .take()
            .is_none_or(|open| open.iter().any(|key| !keys.contains(key)));
        self.latest = current;
        let mut open: Vec<_> = self
            .latest
            .iter()
            .filter(|finding| !self.stopped(&finding.key()))
            .cloned()
            .collect();
        if !fixed || open.is_empty() {
            return Next::Done(self.set_aside());
        }
        // Findings not yet repaired go first, so carried ones get their turn.
        open.sort_by_key(|finding| self.repairs.get(&finding.key()).copied());
        self.open_at_repair = Some(open.iter().map(ReviewFinding::key).collect());
        let carried = open.split_off(open.len().min(PER_REPAIR));
        for finding in &open {
            *self.repairs.entry(finding.key()).or_default() += 1;
        }
        Next::Repair {
            send: open,
            carried,
        }
    }

    /// Stop now: the latest pass's findings wait for the user, as when a
    /// repair's answer cannot be used.
    pub fn set_aside(&self) -> Vec<SetAside> {
        self.latest
            .iter()
            .map(|finding| SetAside {
                finding: finding.clone(),
                repairs: self
                    .repairs
                    .get(&finding.key())
                    .copied()
                    .unwrap_or_default(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic, clippy::indexing_slicing)]
    use super::*;
    use crate::features::learning::review_findings::{FindingKind, FindingSite};

    fn section(index: usize, note: &str) -> ReviewFinding {
        ReviewFinding::new(
            FindingKind::Teaching,
            Some(FindingSite::Section(index)),
            note,
        )
    }

    fn sent(next: Next) -> Vec<FindingKey> {
        match next {
            Next::Repair { send, .. } => send.iter().map(ReviewFinding::key).collect(),
            Next::Done(_) => panic!("expected a repair"),
        }
    }

    fn done(next: Next) -> Vec<(FindingKey, u8)> {
        match next {
            Next::Done(set_aside) => set_aside
                .iter()
                .map(|s| (s.finding.key(), s.repairs))
                .collect(),
            Next::Repair { .. } => panic!("expected the review to stop"),
        }
    }

    #[test]
    fn a_clean_first_pass_publishes_with_nothing_set_aside() {
        assert!(done(Passes::default().next(vec![])).is_empty());
    }

    #[test]
    fn a_pass_that_fixes_something_continues_even_with_its_wording_rewritten() {
        let mut passes = Passes::default();
        assert_eq!(
            sent(passes.next(vec![section(0, "A"), section(1, "B")])).len(),
            2
        );
        // Section 1 is fixed; section 0 survives one repair, reworded.
        let again = sent(passes.next(vec![section(0, "A, said differently")]));
        assert_eq!(again, vec![section(0, "").key()]);
        // Fixed at the second repair: the lesson publishes clean.
        assert!(done(passes.next(vec![])).is_empty());
    }

    #[test]
    fn a_finding_that_survives_two_repairs_is_set_aside_while_others_continue() {
        let mut passes = Passes::default();
        sent(passes.next(vec![section(0, "A"), section(1, "B"), section(2, "C")]));
        sent(passes.next(vec![section(0, "A"), section(1, "B")]));
        // B was fixed, so the review goes on; A survived two repairs and
        // stops; D is new and is repaired.
        let third = sent(passes.next(vec![section(0, "A"), section(3, "D")]));
        assert_eq!(third, vec![section(3, "").key()]);
        // D fixed; A is the only finding left and it stays set aside.
        assert_eq!(
            done(passes.next(vec![section(0, "A")])),
            vec![(section(0, "").key(), 2)]
        );
    }

    #[test]
    fn a_pass_that_fixes_nothing_sets_every_remaining_finding_aside() {
        let mut passes = Passes::default();
        sent(passes.next(vec![section(0, "A"), section(1, "B")]));
        let stopped =
            done(passes.next(vec![section(0, "A"), section(1, "B"), section(2, "C, new")]));
        assert_eq!(
            stopped,
            vec![
                (section(0, "").key(), 1),
                (section(1, "").key(), 1),
                (section(2, "").key(), 0),
            ]
        );
    }

    #[test]
    fn nothing_is_dropped_findings_beyond_one_repair_carry_to_the_next() {
        let mut passes = Passes::default();
        let findings: Vec<_> = (0..9).map(|i| section(i, "x")).collect();
        let Next::Repair { send, carried } = passes.next(findings.clone()) else {
            panic!("expected a repair");
        };
        assert_eq!(send.len(), PER_REPAIR);
        assert_eq!(carried.len(), 3);
        // One fixed: the carried three are sent before any second attempt.
        let second = sent(passes.next(findings[1..].to_vec()));
        assert_eq!(
            &second[..3],
            &[
                section(6, "").key(),
                section(7, "").key(),
                section(8, "").key()
            ]
        );
    }

    #[test]
    fn a_finding_the_user_already_holds_is_never_repaired() {
        let held = section(4, "kept as written").key();
        let mut passes = Passes::new(BTreeSet::from([held]));
        assert!(!passes.repairable(&held));
        assert_eq!(
            done(passes.next(vec![section(4, "still there")])),
            vec![(held, 0)]
        );
    }

    #[test]
    fn two_notes_about_one_place_are_one_finding() {
        let mut passes = Passes::default();
        let Next::Repair { send, .. } =
            passes.next(vec![section(0, "First."), section(0, "Second.")])
        else {
            panic!("expected a repair");
        };
        assert_eq!(send.len(), 1);
        assert_eq!(send[0].note, "First. Second.");
    }
}
