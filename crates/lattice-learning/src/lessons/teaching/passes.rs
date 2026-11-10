//! When a lesson's teaching review stops repairing. Every finding gets a
//! repair. A repair is charged to each finding it was sent (`charge`, with
//! what became of its attempt at it), not when it is sent: a finding charged
//! `REPAIRS_PER_FINDING` repairs stops being repaired; when a repair fixes
//! none of the open findings, the findings that repair was sent stop, and
//! every finding not yet sent to a repair still gets one. The review ends
//! when no repairable finding is left. Stopped
//! findings wait for the user, and the lesson publishes: no teaching-review
//! finding fails a lesson job.
//!
//! A finding is followed by its id (`FindingId`): what raised it and what it
//! is about, never its wording. Findings with one id are one finding; each id
//! has its own count of repairs. Every repair can leave a new defect with a
//! new id, so a new finding about words an earlier repair already worked on
//! starts at that repair's count, and the review still ends. Each repair is
//! sent at most `PER_REPAIR` findings; the others carry to the next pass.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::review_findings::{sentences_of, subject_of, FindingId, Held, ReviewFinding, SetAside};

/// Findings sent to one repair, as many as one request handles well.
pub(super) const PER_REPAIR: usize = 6;

/// Attempts one finding is given: a first, and one retry that is told what
/// became of the first. The loop must end on a defect no repair fixes.
const REPAIRS_PER_FINDING: u8 = 2;

/// What became of one repair's attempt at a finding.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub(super) enum Outcome {
    /// A change naming it was applied and stands.
    Applied,
    /// It was named, and none of its changes could be applied.
    NotApplied(Vec<String>),
    /// Its changes were applied and put back: they touched a sentence that must stay.
    Reverted,
    /// No change named it.
    Unnamed,
    /// The repair's answer could not be used at all.
    Unusable(String),
}

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
    /// Findings the user already has, for text that has not changed since,
    /// and findings held during this review.
    held: Held,
    /// Repairs each finding was charged, or started with (words an earlier
    /// repair worked on).
    spent: BTreeMap<FindingId, u8>,
    /// What became of the last repair's attempt at each finding.
    last: BTreeMap<FindingId, Outcome>,
    /// Sentence digest to the repairs those words have been through.
    worked: BTreeMap<String, u8>,
    /// The findings that were open when the last repair ran.
    open_at_repair: Option<BTreeSet<FindingId>>,
    /// The findings the last repair was sent.
    sent_at_repair: BTreeSet<FindingId>,
    /// Findings a repair was sent and fixed none of the open findings.
    stalled: BTreeSet<FindingId>,
    /// The latest pass's findings, one per id.
    latest: Vec<ReviewFinding>,
}

/// What decides stopping, kept with the draft a repair produced.
#[derive(Deserialize, Serialize)]
struct Kept {
    spent: Vec<(FindingId, u8)>,
    last: Vec<(FindingId, Outcome)>,
    worked: BTreeMap<String, u8>,
    open_at_repair: Option<BTreeSet<FindingId>>,
    sent_at_repair: BTreeSet<FindingId>,
    stalled: BTreeSet<FindingId>,
}

impl Passes {
    pub fn new(held: Held) -> Self {
        Self {
            held,
            ..Self::default()
        }
    }

    /// The state that decides stopping, to save with the draft the latest
    /// repair produced.
    pub fn saved(&self) -> serde_json::Result<String> {
        serde_json::to_string(&Kept {
            spent: self
                .spent
                .iter()
                .map(|(id, count)| (id.clone(), *count))
                .collect(),
            last: self
                .last
                .iter()
                .map(|(id, outcome)| (id.clone(), outcome.clone()))
                .collect(),
            worked: self.worked.clone(),
            open_at_repair: self.open_at_repair.clone(),
            sent_at_repair: self.sent_at_repair.clone(),
            stalled: self.stalled.clone(),
        })
    }

    /// Takes up the state a review cut short saved with this draft; state
    /// that cannot be read, such as state saved before finding ids, is none.
    pub fn resume(&mut self, saved: &str) {
        if let Ok(kept) = serde_json::from_str::<Kept>(saved) {
            self.spent = kept.spent.into_iter().collect();
            self.last = kept.last.into_iter().collect();
            self.worked = kept.worked;
            self.open_at_repair = kept.open_at_repair;
            self.sent_at_repair = kept.sent_at_repair;
            self.stalled = kept.stalled;
        }
    }

    /// The machine no longer repairs this finding.
    fn stopped(&self, finding: &ReviewFinding) -> bool {
        let id = finding.id();
        self.held.ids.contains(&id)
            || self.held.places.contains(&finding.place())
            || self.stalled.contains(&id)
            || self.spent.get(&id).copied().unwrap_or_default() >= REPAIRS_PER_FINDING
    }

    /// Holds these findings for the user: the machine does not repair them
    /// (a finding about a sentence a live claim stands on, §10.3).
    pub fn hold(&mut self, ids: impl IntoIterator<Item = FindingId>) {
        self.held.ids.extend(ids);
    }

    /// Whether a deterministic finding still blocks the content review: one
    /// the machine stopped on no longer does.
    pub fn repairable(&self, finding: &ReviewFinding) -> bool {
        !self.stopped(finding)
    }

    /// Decide on one pass's findings, raised on `candidate`, in the order the
    /// review gave them.
    pub fn next(&mut self, candidate: &Value, findings: Vec<ReviewFinding>) -> Next {
        // One finding per id: two notes about one defect are one finding.
        let mut current: Vec<ReviewFinding> = Vec::new();
        for finding in findings {
            match current.iter_mut().find(|known| known.id() == finding.id()) {
                Some(known) => {
                    if !known.note.contains(finding.note.as_str()) {
                        known.note = format!("{} {}", known.note, finding.note);
                    }
                    if known.summary.is_empty() {
                        known.summary = finding.summary;
                    } else if !finding.summary.is_empty()
                        && !known.summary.lines().any(|line| line == finding.summary)
                    {
                        known.summary = format!("{}\n{}", known.summary, finding.summary);
                    }
                    known.passage = known.passage.take().or(finding.passage);
                    if known.sentence != finding.sentence {
                        known.sentence = None;
                    }
                }
                None => current.push(finding),
            }
        }
        for finding in &current {
            let id = finding.id();
            if self.spent.contains_key(&id) {
                continue;
            }
            let start = sentences_of(candidate, finding)
                .iter()
                .filter_map(|sentence| self.worked.get(&subject_of(sentence)).copied())
                .max()
                .unwrap_or_default();
            if start > 0 {
                self.spent.insert(id, start);
            }
        }
        let ids: BTreeSet<_> = current.iter().map(ReviewFinding::id).collect();
        let fixed = self
            .open_at_repair
            .take()
            .is_none_or(|open| open.iter().any(|id| !ids.contains(id)));
        self.latest = current;
        let last_sent = std::mem::take(&mut self.sent_at_repair);
        if !fixed {
            self.stalled.extend(last_sent);
        }
        let mut open: Vec<_> = self
            .latest
            .iter()
            .filter(|finding| !self.stopped(finding))
            .cloned()
            .collect();
        if open.is_empty() {
            return Next::Done(self.set_aside());
        }
        // Findings not yet repaired go first, so carried ones get their turn.
        open.sort_by_key(|finding| self.spent.get(&finding.id()).copied());
        self.open_at_repair = Some(open.iter().map(ReviewFinding::id).collect());
        let carried = open.split_off(open.len().min(PER_REPAIR));
        self.sent_at_repair = open.iter().map(ReviewFinding::id).collect();
        Next::Repair {
            send: open,
            carried,
        }
    }

    /// A repair was sent `finding` and this became of its attempt at it: the
    /// repair is charged to it.
    pub fn charge(&mut self, finding: &ReviewFinding, outcome: Outcome) {
        let id = finding.id();
        *self.spent.entry(id.clone()).or_default() += 1;
        self.last.insert(id, outcome);
    }

    /// What became of the last repair's attempt at `id`, told to its next one.
    pub fn last_attempt(&self, id: &FindingId) -> Option<String> {
        Some(match self.last.get(id)? {
            Outcome::Applied => "The last repair's edits for this finding were applied and the defect is still there.".to_owned(),
            Outcome::NotApplied(reasons) => format!(
                "The last repair's edits for this finding could not be applied: {}",
                reasons.join(" ")
            ),
            Outcome::Reverted => "The last repair's edits for this finding changed a sentence that must stay as it is; they were put back.".to_owned(),
            Outcome::Unnamed => "The last repair gave no edit for this finding.".to_owned(),
            Outcome::Unusable(reason) => {
                format!("The last repair's answer could not be used: {reason}")
            }
        })
    }

    /// An applied repair put these sentences through the repairs `id` had
    /// been charged: what it wrote, or the words it was applied for. A new
    /// finding about them starts at that count.
    pub fn worked_on(&mut self, id: &FindingId, sentences: impl IntoIterator<Item = String>) {
        let count = self.spent.get(id).copied().unwrap_or_default();
        for sentence in sentences {
            let held = self.worked.entry(subject_of(&sentence)).or_default();
            *held = (*held).max(count);
        }
    }

    /// The last repair's answer could not be used: the findings it was sent
    /// stop, and the others still get theirs. Whether any such finding is left.
    pub fn stop_sent(&mut self) -> bool {
        self.stalled
            .extend(std::mem::take(&mut self.sent_at_repair));
        self.open_at_repair = None;
        self.latest.iter().any(|finding| !self.stopped(finding))
    }

    /// Stop now: the latest pass's findings wait for the user, as when a
    /// repair's answer cannot be used.
    pub fn set_aside(&self) -> Vec<SetAside> {
        self.latest
            .iter()
            .map(|finding| SetAside {
                finding: finding.clone(),
                repairs: self.spent.get(&finding.id()).copied().unwrap_or_default(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic, clippy::indexing_slicing)]
    use super::*;
    use crate::review_findings::{FindingSite, PlanCheck};

    fn section(index: usize, note: &str) -> ReviewFinding {
        ReviewFinding::reviewer(Some(FindingSite::Section(index)), note)
    }

    /// A finding in section `index` about `words`.
    fn own(index: usize, words: &str) -> ReviewFinding {
        ReviewFinding::plan(
            PlanCheck::Used,
            subject_of(words),
            FindingSite::Section(index),
            words,
        )
        .with_passage(words)
    }

    /// The ids the next repair is sent, each charged as a repair that applied.
    fn sent(
        passes: &mut Passes,
        candidate: &Value,
        findings: Vec<ReviewFinding>,
    ) -> Vec<FindingId> {
        match passes.next(candidate, findings) {
            Next::Repair { send, .. } => {
                for finding in &send {
                    passes.charge(finding, Outcome::Applied);
                }
                send.iter().map(ReviewFinding::id).collect()
            }
            Next::Done(_) => panic!("expected a repair"),
        }
    }

    fn done(next: Next) -> Vec<(FindingId, u8)> {
        match next {
            Next::Done(set_aside) => set_aside
                .iter()
                .map(|s| (s.finding.id(), s.repairs))
                .collect(),
            Next::Repair { .. } => panic!("expected the review to stop"),
        }
    }

    #[test]
    fn every_open_finding_goes_to_one_repair() {
        let mut passes = Passes::default();
        let findings: Vec<_> = (0..9).map(|i| section(i, "x")).collect();
        assert_eq!(sent(&mut passes, &Value::Null, findings).len(), 9);
    }

    #[test]
    fn state_saved_with_fields_no_longer_kept_still_resumes() {
        let id = serde_json::to_value(section(3, "").id()).unwrap();
        let saved = serde_json::json!({
            "spent": [[id, 1]],
            "last": [[id, "Applied"]],
            "worked": {},
            "open_at_repair": [id],
            "sent_at_repair": [],
            "stalled": [],
        })
        .to_string();
        let mut passes = Passes::default();
        passes.resume(&saved);
        assert_eq!(passes.spent.get(&section(3, "").id()), Some(&1));
    }

    #[test]
    fn a_clean_first_pass_publishes_with_nothing_set_aside() {
        assert!(done(Passes::default().next(&Value::Null, vec![])).is_empty());
    }

    #[test]
    fn a_pass_that_fixes_something_continues_even_with_its_wording_rewritten() {
        let mut passes = Passes::default();
        assert_eq!(
            sent(
                &mut passes,
                &Value::Null,
                vec![section(0, "A"), section(1, "B")]
            )
            .len(),
            2
        );
        // Section 1 is fixed; section 0 survives one repair, reworded.
        let again = sent(
            &mut passes,
            &Value::Null,
            vec![section(0, "A, said differently")],
        );
        assert_eq!(again, vec![section(0, "").id()]);
        // Fixed at the second repair: the lesson publishes clean.
        assert!(done(passes.next(&Value::Null, vec![])).is_empty());
    }

    #[test]
    fn a_finding_that_survives_two_repairs_is_set_aside_while_others_continue() {
        let mut passes = Passes::default();
        sent(
            &mut passes,
            &Value::Null,
            vec![section(0, "A"), section(1, "B"), section(2, "C")],
        );
        sent(
            &mut passes,
            &Value::Null,
            vec![section(0, "A"), section(1, "B")],
        );
        // B was fixed, so the review goes on; A survived two repairs and
        // stops; D is new and is repaired.
        let third = sent(
            &mut passes,
            &Value::Null,
            vec![section(0, "A"), section(3, "D")],
        );
        assert_eq!(third, vec![section(3, "").id()]);
        // D fixed; A is the only finding left and it stays set aside.
        assert_eq!(
            done(passes.next(&Value::Null, vec![section(0, "A")])),
            vec![(section(0, "").id(), 2)]
        );
    }

    #[test]
    fn a_repair_that_fixes_nothing_stops_only_the_findings_it_was_sent() {
        let mut passes = Passes::default();
        sent(
            &mut passes,
            &Value::Null,
            vec![section(0, "A"), section(1, "B")],
        );
        let third = sent(
            &mut passes,
            &Value::Null,
            vec![section(0, "A"), section(1, "B"), section(2, "C, new")],
        );
        assert_eq!(third, vec![section(2, "").id()]);
        assert_eq!(
            done(passes.next(&Value::Null, vec![section(0, "A"), section(1, "B")])),
            vec![(section(0, "").id(), 1), (section(1, "").id(), 1)]
        );
    }

    #[test]
    fn findings_carried_past_a_repair_that_fixes_nothing_still_get_theirs() {
        let mut passes = Passes::default();
        let findings: Vec<_> = (0..9).map(|i| section(i, "x")).collect();
        assert_eq!(
            sent(&mut passes, &Value::Null, findings.clone()).len(),
            PER_REPAIR
        );
        let second = sent(&mut passes, &Value::Null, findings);
        assert_eq!(
            second,
            vec![
                section(6, "").id(),
                section(7, "").id(),
                section(8, "").id()
            ]
        );
    }

    #[test]
    fn state_saved_before_finding_ids_resumes_with_fresh_budgets() {
        let key = section(3, "").place();
        let old = serde_json::to_string(&serde_json::json!({
            "repairs": [(key, 1)],
            "open_at_repair": [key],
        }))
        .unwrap();
        let mut passes = Passes::default();
        passes.resume(&old);
        assert!(passes.spent.is_empty());
        assert!(passes.worked.is_empty());
        assert!(passes.open_at_repair.is_none());
        assert!(passes.sent_at_repair.is_empty());
        // Nothing was kept, so the finding gets a repair as a new one.
        assert_eq!(
            sent(&mut passes, &Value::Null, vec![section(3, "x")]),
            vec![section(3, "").id()]
        );
    }

    #[test]
    fn an_unusable_repair_stops_what_it_was_sent_and_leaves_the_rest() {
        let mut passes = Passes::default();
        let findings: Vec<_> = (0..8).map(|i| section(i, "x")).collect();
        sent(&mut passes, &Value::Null, findings.clone());
        assert!(passes.stop_sent());
        let next = sent(&mut passes, &Value::Null, findings);
        assert_eq!(next, vec![section(6, "").id(), section(7, "").id()]);
        assert!(!passes.stop_sent());
    }

    #[test]
    fn state_saved_mid_review_keeps_what_stopped() {
        let mut passes = Passes::default();
        sent(&mut passes, &Value::Null, vec![section(0, "A")]);
        sent(
            &mut passes,
            &Value::Null,
            vec![section(0, "A"), section(1, "B")],
        );
        let saved = passes.saved().unwrap();
        let mut resumed = Passes::default();
        resumed.resume(&saved);
        assert!(!resumed.repairable(&section(0, "")));
    }

    #[test]
    fn nothing_is_dropped_findings_beyond_one_repair_carry_to_the_next() {
        let mut passes = Passes::default();
        let findings: Vec<_> = (0..9).map(|i| section(i, "x")).collect();
        let Next::Repair { send, carried } = passes.next(&Value::Null, findings.clone()) else {
            panic!("expected a repair");
        };
        assert_eq!(send.len(), PER_REPAIR);
        assert_eq!(carried.len(), 3);
        for finding in &send {
            passes.charge(finding, Outcome::Applied);
        }
        // One fixed: the carried three are sent before any second attempt.
        let second = sent(&mut passes, &Value::Null, findings[1..].to_vec());
        assert_eq!(
            &second[..3],
            &[
                section(6, "").id(),
                section(7, "").id(),
                section(8, "").id()
            ]
        );
    }

    #[test]
    fn a_finding_the_user_already_holds_is_never_repaired() {
        let held = section(4, "kept as written");
        let mut passes = Passes::new(Held {
            places: BTreeSet::from([held.place()]),
            ..Held::default()
        });
        assert!(!passes.repairable(&held));
        assert_eq!(
            done(passes.next(&Value::Null, vec![section(4, "still there")])),
            vec![(held.id(), 0)]
        );
    }

    #[test]
    fn a_finding_held_during_the_review_is_never_repaired() {
        let held = section(4, "a protected sentence");
        let mut passes = Passes::default();
        passes.hold([held.id()]);
        assert!(!passes.repairable(&held));
        assert_eq!(
            done(passes.next(&Value::Null, vec![held.clone()])),
            vec![(held.id(), 0)]
        );
    }

    #[test]
    fn two_notes_about_one_place_are_one_finding() {
        let mut passes = Passes::default();
        let Next::Repair { send, .. } = passes.next(
            &Value::Null,
            vec![section(0, "First."), section(0, "Second.")],
        ) else {
            panic!("expected a repair");
        };
        assert_eq!(send.len(), 1);
        assert_eq!(send[0].note, "First. Second.");
    }

    #[test]
    fn two_findings_with_their_own_ids_at_one_place_stay_two() {
        let mut passes = Passes::default();
        let both = sent(
            &mut passes,
            &Value::Null,
            vec![own(0, "A first sentence."), own(0, "A second sentence.")],
        );
        assert_eq!(both.len(), 2);
    }

    #[test]
    fn a_new_finding_about_words_a_repair_worked_on_starts_at_that_repair_s_count() {
        let candidate = serde_json::json!({"blocks":[{"body":"A first sentence. A second sentence."}],"questions":[]});
        let mut passes = Passes::default();
        let first = own(0, "A first sentence.");
        sent(&mut passes, &candidate, vec![first.clone()]);
        passes.worked_on(&first.id(), ["A second sentence.".to_owned()]);
        let second = own(0, "A second sentence.");
        assert_eq!(
            sent(&mut passes, &candidate, vec![second.clone()]),
            vec![second.id()]
        );
        assert_eq!(
            passes.spent.get(&second.id()),
            Some(&2),
            "it started at the repair's count of 1 and was sent once more"
        );
    }

    #[test]
    fn a_new_finding_about_words_that_went_through_the_limit_is_not_repaired() {
        let candidate = serde_json::json!({"blocks":[
            {"body":"A first sentence."},
            {"body":"A filler."},
            {"body":"A second sentence."}
        ],"questions":[]});
        let mut passes = Passes::default();
        let first = own(0, "A first sentence.");
        sent(
            &mut passes,
            &candidate,
            vec![first.clone(), own(1, "A filler.")],
        );
        sent(&mut passes, &candidate, vec![first.clone()]);
        passes.worked_on(&first.id(), ["A second sentence.".to_owned()]);
        let second = own(2, "A second sentence.");
        assert_eq!(
            done(passes.next(&candidate, vec![second.clone()])),
            vec![(second.id(), 2)]
        );
    }

    #[test]
    fn the_summaries_of_two_notes_about_one_place_stay_together() {
        let mut passes = Passes::default();
        let Next::Repair { send, .. } = passes.next(
            &Value::Null,
            vec![
                section(0, "First.").with_summary("One problem."),
                section(0, "Second.").with_summary("Another problem."),
                section(0, "Third.").with_summary("One problem."),
                section(0, "Fourth."),
            ],
        ) else {
            panic!("expected a repair");
        };
        assert_eq!(send.len(), 1);
        assert_eq!(send[0].summary, "One problem.\nAnother problem.");
    }

    /// A note about one sentence lets the user take that sentence out, so a
    /// note that now stands for more than one has no sentence.
    #[test]
    fn a_merged_note_keeps_its_sentence_only_while_every_note_in_it_is_about_that_sentence() {
        let about = |sentence: &str| section(0, "A note.").with_sentence(sentence);
        let merged = |findings: Vec<ReviewFinding>| {
            let mut passes = Passes::default();
            let Next::Repair { send, .. } = passes.next(&Value::Null, findings) else {
                panic!("expected a repair");
            };
            assert_eq!(send.len(), 1);
            send[0].sentence.clone()
        };
        assert_eq!(
            merged(vec![about("One."), about("One.")]).as_deref(),
            Some("One.")
        );
        assert_eq!(merged(vec![about("One."), about("Two.")]), None);
        assert_eq!(merged(vec![about("One."), section(0, "Other.")]), None);
        assert_eq!(merged(vec![section(0, "Other."), about("One.")]), None);
    }

    #[test]
    fn a_finding_that_was_sent_is_not_charged_until_the_repair_is_settled() {
        let mut passes = Passes::default();
        let Next::Repair { send, .. } = passes.next(&Value::Null, vec![section(0, "A")]) else {
            panic!("expected a repair");
        };
        assert!(passes.spent.is_empty());
        passes.charge(&send[0], Outcome::Unnamed);
        assert_eq!(passes.spent.get(&send[0].id()), Some(&1));
        assert_eq!(passes.last.get(&send[0].id()), Some(&Outcome::Unnamed));
    }

    #[test]
    fn a_second_attempt_is_told_what_became_of_the_first() {
        let mut passes = Passes::default();
        let finding = section(0, "A");
        let id = finding.id();
        assert_eq!(passes.last_attempt(&id), None);
        let told = |passes: &mut Passes, outcome: Outcome| {
            passes.charge(&finding, outcome);
            passes.last_attempt(&id).unwrap()
        };
        assert_eq!(
            told(&mut passes, Outcome::Applied),
            "The last repair's edits for this finding were applied and the defect is still there."
        );
        assert_eq!(
            told(
                &mut passes,
                Outcome::NotApplied(vec!["edit 0 first.".into(), "edit 1 second.".into()])
            ),
            "The last repair's edits for this finding could not be applied: edit 0 first. edit 1 second."
        );
        assert_eq!(
            told(&mut passes, Outcome::Reverted),
            "The last repair's edits for this finding changed a sentence that must stay as it is; they were put back."
        );
        assert_eq!(
            told(&mut passes, Outcome::Unnamed),
            "The last repair gave no edit for this finding."
        );
        assert_eq!(
            told(&mut passes, Outcome::Unusable("it was empty.".into())),
            "The last repair's answer could not be used: it was empty."
        );
    }

    #[test]
    fn what_became_of_the_last_attempt_survives_a_saved_review() {
        let mut passes = Passes::default();
        let finding = section(0, "A");
        passes.charge(&finding, Outcome::Reverted);
        let saved = passes.saved().unwrap();
        let mut resumed = Passes::default();
        resumed.resume(&saved);
        assert_eq!(resumed.last.get(&finding.id()), Some(&Outcome::Reverted));
        assert_eq!(resumed.spent.get(&finding.id()), Some(&1));
    }
}
