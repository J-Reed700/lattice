//! When a lesson's teaching review stops repairing. Every open finding is sent
//! to one repair. A repair is charged to each finding it was sent (`charge`,
//! with what became of its attempt at it), not when it is sent: a finding
//! charged `REPAIRS_PER_FINDING` repairs stops being repaired, as does one the
//! user holds or one held during this review. The review ends when no
//! repairable finding is left, and a pass held up by structure is followed by
//! one that reviews the content. Stopped findings wait for the user, and the
//! lesson publishes: no teaching-review finding fails a lesson job.
//!
//! An attempt that changed nothing for a finding is a miss: no change named it,
//! or none of its changes could be placed. A miss is charged only when the
//! finding's attempt before it was a miss too, so of any two attempts in a row
//! one is charged and a finding is sent at most twice `REPAIRS_PER_FINDING`
//! repairs.
//!
//! A finding is followed by its id (`FindingId`): what raised it and what it
//! is about, never its wording. Findings with one id are one finding; each id
//! has its own count of repairs. Every repair can leave a new defect with a
//! new id, so a new finding about words an earlier repair already worked on
//! starts at that repair's count, and the review still ends. A new reviewer
//! finding with no passage has no words to count, so it starts at the count
//! of the reviewer's findings with no passage already at its place.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::review_findings::{
    sentences_in, sentences_of, subject_of, FindingId, FindingSite, Held, Origin, ReviewFinding,
    SetAside,
};

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
    /// Its edit overlapped another finding's and was left out: not an attempt.
    Deferred,
}

impl Outcome {
    /// The attempt changed nothing for the finding.
    fn is_miss(&self) -> bool {
        matches!(self, Self::Unnamed | Self::NotApplied(_))
    }

    /// What became of the attempt, for the user's log.
    fn in_words(&self) -> String {
        match self {
            Self::Applied => "its edits were applied and the defect was still found".to_owned(),
            Self::NotApplied(reasons) => {
                format!("its edits could not be applied: {}", reasons.join(" "))
            }
            Self::Reverted => {
                "its edits were put back: they touched a sentence that must stay".to_owned()
            }
            Self::Unnamed => "the repair made no change for it".to_owned(),
            Self::Unusable(reason) => format!("the repair's answer could not be used: {reason}"),
            Self::Deferred => "its edit overlapped another finding's and was left out".to_owned(),
        }
    }
}

/// What a finding's repairs came to, for the log of one the user is left with.
pub(super) struct Account {
    /// The repairs it started with: words an earlier repair worked on.
    pub inherited: u8,
    /// Why the machine did not repair it; empty when it was not held.
    pub held: String,
    /// Each attempt's outcome in plain words, in order.
    pub attempts: String,
}

pub(super) enum Next {
    /// Nothing is left to repair; these findings wait for the user.
    Done(Vec<SetAside>),
    /// Repair these: every finding that can still be repaired.
    Repair(Vec<ReviewFinding>),
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
    /// What became of every repair's attempt at each finding, in order.
    attempts: BTreeMap<FindingId, Vec<Outcome>>,
    /// The repairs each finding started with.
    inherited: BTreeMap<FindingId, u8>,
    /// Why each finding held during this review was held.
    held_because: BTreeMap<FindingId, &'static str>,
    /// Sentence digest to the repairs those words have been through.
    worked: BTreeMap<String, u8>,
    /// For each place, the most repairs a reviewer finding with no passage
    /// there has been charged.
    passage_less: BTreeMap<FindingSite, u8>,
    /// The latest pass's findings, one per id.
    latest: Vec<ReviewFinding>,
    /// The digests of the sentences each of the latest findings is about.
    words: BTreeMap<FindingId, Vec<String>>,
    /// The digests of the sentences the user's stored reviewer findings are
    /// about: the machine does not repair a finding about them.
    stored: BTreeSet<String>,
}

/// What decides stopping, kept with the draft a repair produced.
#[derive(Deserialize, Serialize)]
struct Kept {
    spent: Vec<(FindingId, u8)>,
    last: Vec<(FindingId, Outcome)>,
    worked: BTreeMap<String, u8>,
    #[serde(default)]
    passage_less: Vec<(FindingSite, u8)>,
    #[serde(default)]
    attempts: Vec<(FindingId, Vec<Outcome>)>,
    #[serde(default)]
    inherited: Vec<(FindingId, u8)>,
}

/// Whether a reviewer's finding has no passage to count the repairs of.
fn passage_less(finding: &ReviewFinding) -> bool {
    matches!(finding.origin, Origin::Reviewer { .. }) && finding.passage.is_none()
}

impl Passes {
    pub fn new(held: Held) -> Self {
        let stored = held
            .reviewer
            .iter()
            .filter_map(|(finding, _)| finding.passage.as_deref())
            .flat_map(|passage| sentences_in(&Value::String(passage.to_owned())))
            .map(|sentence| subject_of(&sentence))
            .collect();
        Self {
            held,
            stored,
            ..Self::default()
        }
    }

    /// The state that decides stopping, to save with the draft the latest
    /// repair produced.
    pub fn saved(&self) -> serde_json::Result<Value> {
        serde_json::to_value(Kept {
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
            passage_less: self
                .passage_less
                .iter()
                .map(|(site, count)| (*site, *count))
                .collect(),
            attempts: self
                .attempts
                .iter()
                .map(|(id, outcomes)| (id.clone(), outcomes.clone()))
                .collect(),
            inherited: self
                .inherited
                .iter()
                .map(|(id, count)| (id.clone(), *count))
                .collect(),
        })
    }

    /// Takes up the state a review cut short saved with this draft; state
    /// that cannot be read, such as state saved before finding ids, is none.
    pub fn resume(&mut self, saved: &str) {
        if let Ok(kept) = serde_json::from_str::<Kept>(saved) {
            self.spent = kept.spent.into_iter().collect();
            self.last = kept.last.into_iter().collect();
            self.worked = kept.worked;
            self.passage_less = kept.passage_less.into_iter().collect();
            self.attempts = kept.attempts.into_iter().collect();
            self.inherited = kept.inherited.into_iter().collect();
        }
    }

    /// The machine no longer repairs this finding.
    fn stopped(&self, finding: &ReviewFinding) -> bool {
        let id = finding.id();
        self.held.ids.contains(&id)
            || self.held.places.contains(&finding.place())
            || self.spent.get(&id).copied().unwrap_or_default() >= REPAIRS_PER_FINDING
    }

    /// Holds these findings for the user, `why`: the machine does not repair
    /// them (a finding about a sentence a live claim stands on, §10.3).
    pub fn hold(&mut self, ids: impl IntoIterator<Item = FindingId>, why: &'static str) {
        for id in ids {
            self.held_because.entry(id.clone()).or_insert(why);
            self.held.ids.insert(id);
        }
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
        self.words.clear();
        for finding in &current {
            let id = finding.id();
            let about: Vec<String> = sentences_of(candidate, finding)
                .iter()
                .map(|sentence| subject_of(sentence))
                .collect();
            let mut start = about
                .iter()
                .filter_map(|digest| self.worked.get(digest).copied())
                .max()
                .unwrap_or_default();
            self.words.insert(id.clone(), about.clone());
            if self.spent.contains_key(&id) {
                continue;
            }
            if matches!(finding.origin, Origin::Reviewer { .. })
                && about.iter().any(|digest| self.stored.contains(digest))
            {
                start = REPAIRS_PER_FINDING;
            }
            if passage_less(finding) {
                let site = finding.site.unwrap_or(FindingSite::Lesson);
                start = start.max(self.passage_less.get(&site).copied().unwrap_or_default());
            }
            if start > 0 {
                self.inherited.insert(id.clone(), start);
                self.spent.insert(id, start);
            }
        }
        self.latest = current;
        let open: Vec<_> = self
            .latest
            .iter()
            .filter(|finding| !self.stopped(finding))
            .cloned()
            .collect();
        if open.is_empty() {
            return Next::Done(self.set_aside());
        }
        Next::Repair(open)
    }

    /// A repair was sent `finding` and this became of its attempt at it: the
    /// repair is charged to it, unless its edit was only left out for
    /// overlapping another finding's, or the attempt was a miss and the one
    /// before it (not counting those left out) was not. Of two attempts in a
    /// row one is charged, so the review still ends.
    pub fn charge(&mut self, finding: &ReviewFinding, outcome: Outcome) {
        let id = finding.id();
        let attempts = self.attempts.entry(id.clone()).or_default();
        let missed_before = attempts
            .iter()
            .rev()
            .find(|past| **past != Outcome::Deferred)
            .is_some_and(Outcome::is_miss);
        let free_miss = outcome.is_miss() && !missed_before;
        attempts.push(outcome.clone());
        if outcome == Outcome::Deferred || free_miss {
            self.last.insert(id, outcome);
            return;
        }
        let spent = self.spent.entry(id.clone()).or_default();
        *spent += 1;
        let own = *spent;
        for digest in self.words.get(&id).into_iter().flatten() {
            let held = self.worked.entry(digest.clone()).or_default();
            *held = (*held).max(own);
        }
        if passage_less(finding) {
            let site = finding.site.unwrap_or(FindingSite::Lesson);
            let known = self.passage_less.entry(site).or_default();
            *known = (*known).max(own);
        }
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
            Outcome::Deferred => "The last repair's edit for this finding overlapped an edit for another finding and was left out; give it again.".to_owned(),
        })
    }

    /// What `finding`'s repairs came to.
    pub fn account(&self, finding: &ReviewFinding) -> Account {
        let id = finding.id();
        let held = if let Some(why) = self.held_because.get(&id) {
            (*why).to_owned()
        } else if self.held.ids.contains(&id) || self.held.places.contains(&finding.place()) {
            "already with the user".to_owned()
        } else {
            String::new()
        };
        Account {
            inherited: self.inherited.get(&id).copied().unwrap_or_default(),
            held,
            attempts: self
                .attempts
                .get(&id)
                .into_iter()
                .flatten()
                .map(Outcome::in_words)
                .collect::<Vec<_>>()
                .join(" | "),
        }
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

    /// The latest pass's findings, each with the repairs it was charged: what
    /// waits for the user when the review stops.
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
    use std::collections::BTreeSet;

    fn section(index: usize, note: &str) -> ReviewFinding {
        ReviewFinding::reviewer(
            u32::try_from(index).unwrap(),
            Some(FindingSite::Section(index)),
            note,
        )
    }

    /// A plan finding at section 0: every call is the same finding.
    fn plan(note: &str) -> ReviewFinding {
        ReviewFinding::plan(PlanCheck::Used, "an idea", FindingSite::Section(0), note)
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
            Next::Repair(send) => {
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
            Next::Repair(_) => panic!("expected the review to stop"),
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
        // Nothing was kept, so the finding gets a repair as a new one.
        assert_eq!(
            sent(&mut passes, &Value::Null, vec![section(3, "x")]),
            vec![section(3, "").id()]
        );
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
        let saved = passes.saved().unwrap().to_string();
        let mut resumed = Passes::default();
        resumed.resume(&saved);
        assert!(!resumed.repairable(&section(0, "")));
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
        passes.hold([held.id()], "a test");
        assert!(!passes.repairable(&held));
        assert_eq!(
            done(passes.next(&Value::Null, vec![held.clone()])),
            vec![(held.id(), 0)]
        );
    }

    #[test]
    fn two_notes_with_one_id_are_one_finding() {
        let mut passes = Passes::default();
        let Next::Repair(send) = passes.next(&Value::Null, vec![plan("First."), plan("Second.")])
        else {
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
    fn a_finding_about_words_a_reverted_repair_was_charged_for_starts_at_that_charge() {
        let candidate = serde_json::json!({"blocks":[{"body":"A first sentence. A second sentence."}],"questions":[]});
        let about = |number| {
            ReviewFinding::reviewer(number, Some(FindingSite::Section(0)), "It is wrong.")
                .with_passage("A first sentence.")
        };
        let mut passes = Passes::default();
        let first = about(1);
        assert!(matches!(
            passes.next(&candidate, vec![first.clone()]),
            Next::Repair(_)
        ));
        passes.charge(&first, Outcome::Reverted);
        let again = about(2);
        assert!(matches!(
            passes.next(&candidate, vec![again.clone()]),
            Next::Repair(_)
        ));
        assert_eq!(
            passes.spent.get(&again.id()),
            Some(&1),
            "the defect was charged one repair before this finding named it"
        );
    }

    #[test]
    fn a_reviewer_finding_about_words_a_stored_finding_is_about_is_not_repaired() {
        let candidate = serde_json::json!({"blocks":[{"body":"A first sentence. A second sentence."}],"questions":[]});
        let about = |number, words| {
            ReviewFinding::reviewer(number, Some(FindingSite::Section(0)), "It is wrong.")
                .with_passage(words)
        };
        let held = Held {
            reviewer: vec![(about(1, "A first sentence."), String::new())],
            next_number: 2,
            ..Held::default()
        };
        let mut passes = Passes::new(held);
        let restated = about(2, "A first sentence.");
        assert!(matches!(
            passes.next(&candidate, vec![restated.clone()]),
            Next::Done(_)
        ));
        let other = about(3, "A second sentence.");
        assert!(matches!(
            passes.next(&candidate, vec![other]),
            Next::Repair(_)
        ));
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
    fn the_summaries_of_two_notes_with_one_id_stay_together() {
        let mut passes = Passes::default();
        let Next::Repair(send) = passes.next(
            &Value::Null,
            vec![
                plan("First.").with_summary("One problem."),
                plan("Second.").with_summary("Another problem."),
                plan("Third.").with_summary("One problem."),
                plan("Fourth."),
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
        let about = |sentence: &str| plan("A note.").with_sentence(sentence);
        let merged = |findings: Vec<ReviewFinding>| {
            let mut passes = Passes::default();
            let Next::Repair(send) = passes.next(&Value::Null, findings) else {
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
        assert_eq!(merged(vec![about("One."), plan("Other.")]), None);
        assert_eq!(merged(vec![plan("Other."), about("One.")]), None);
    }

    #[test]
    fn a_finding_that_was_sent_is_not_charged_until_the_repair_is_settled() {
        let mut passes = Passes::default();
        let Next::Repair(send) = passes.next(&Value::Null, vec![section(0, "A")]) else {
            panic!("expected a repair");
        };
        assert!(passes.spent.is_empty());
        passes.charge(&send[0], Outcome::Applied);
        assert_eq!(passes.spent.get(&send[0].id()), Some(&1));
        assert_eq!(passes.last.get(&send[0].id()), Some(&Outcome::Applied));
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
        let saved = passes.saved().unwrap().to_string();
        let mut resumed = Passes::default();
        resumed.resume(&saved);
        assert_eq!(resumed.last.get(&finding.id()), Some(&Outcome::Reverted));
        assert_eq!(resumed.spent.get(&finding.id()), Some(&1));
    }

    /// A reviewer finding numbered `number`, at section 0, with no passage.
    fn without_passage(number: u32) -> ReviewFinding {
        ReviewFinding::reviewer(number, Some(FindingSite::Section(0)), "A defect.")
    }

    #[test]
    fn a_new_reviewer_finding_with_no_passage_starts_at_the_count_of_the_one_before() {
        let mut passes = Passes::default();
        sent(&mut passes, &Value::Null, vec![without_passage(1)]);
        let second = without_passage(2);
        assert_eq!(
            sent(&mut passes, &Value::Null, vec![second.clone()]),
            vec![second.id()]
        );
        assert_eq!(passes.spent.get(&second.id()), Some(&2));
        assert!(!passes.repairable(&second));
    }

    #[test]
    fn a_new_reviewer_finding_with_a_passage_does_not_start_at_the_count_of_the_one_before() {
        let mut passes = Passes::default();
        sent(&mut passes, &Value::Null, vec![without_passage(1)]);
        let second = without_passage(2).with_passage("Some words.");
        sent(&mut passes, &Value::Null, vec![second.clone()]);
        assert_eq!(passes.spent.get(&second.id()), Some(&1));
        assert!(passes.repairable(&second));
    }

    #[test]
    fn a_plan_finding_does_not_start_at_the_count_of_a_reviewer_finding() {
        let mut passes = Passes::default();
        sent(&mut passes, &Value::Null, vec![without_passage(1)]);
        let other = plan("Another defect.");
        sent(&mut passes, &Value::Null, vec![other.clone()]);
        assert_eq!(passes.spent.get(&other.id()), Some(&1));
    }

    #[test]
    fn the_count_of_findings_with_no_passage_survives_a_saved_review() {
        let mut passes = Passes::default();
        sent(&mut passes, &Value::Null, vec![without_passage(1)]);
        let saved = passes.saved().unwrap().to_string();
        let mut resumed = Passes::default();
        resumed.resume(&saved);
        let second = without_passage(2);
        sent(&mut resumed, &Value::Null, vec![second.clone()]);
        assert_eq!(resumed.spent.get(&second.id()), Some(&2));
    }

    #[test]
    fn state_saved_before_findings_with_no_passage_were_counted_still_resumes() {
        let id = serde_json::to_value(section(3, "").id()).unwrap();
        let saved = serde_json::json!({"spent": [[id, 1]], "last": [], "worked": {}}).to_string();
        let mut passes = Passes::default();
        passes.resume(&saved);
        assert_eq!(passes.spent.get(&section(3, "").id()), Some(&1));
        assert!(passes.passage_less.is_empty());
    }

    #[test]
    fn an_attempt_left_out_for_overlap_is_no_repair_and_is_told_to_try_again() {
        let mut passes = Passes::default();
        let finding = own(0, "A first sentence.");
        for _ in 0..3 {
            passes.charge(&finding, Outcome::Deferred);
        }
        assert_eq!(passes.spent.get(&finding.id()), None);
        assert!(passes.worked.is_empty());
        assert!(passes.repairable(&finding));
        assert_eq!(
            passes.last_attempt(&finding.id()).as_deref(),
            Some("The last repair's edit for this finding overlapped an edit for another finding and was left out; give it again.")
        );
        passes.charge(&finding, Outcome::Applied);
        assert_eq!(passes.spent.get(&finding.id()), Some(&1));
    }

    #[test]
    fn a_deferred_attempt_survives_a_saved_review() {
        let mut passes = Passes::default();
        let finding = own(0, "A first sentence.");
        passes.charge(&finding, Outcome::Deferred);
        let saved = passes.saved().unwrap().to_string();
        let mut resumed = Passes::default();
        resumed.resume(&saved);
        assert_eq!(resumed.last.get(&finding.id()), Some(&Outcome::Deferred));
        assert_eq!(
            resumed.attempts.get(&finding.id()),
            Some(&vec![Outcome::Deferred])
        );
    }

    #[test]
    fn the_account_of_a_finding_lists_its_attempts_in_order_and_why_it_was_held() {
        let mut passes = Passes::default();
        let finding = own(0, "A first sentence.");
        passes.charge(
            &finding,
            Outcome::NotApplied(vec!["It did not occur.".to_owned()]),
        );
        passes.charge(&finding, Outcome::Deferred);
        passes.hold([finding.id()], "a sentence a live claim stands on");
        let account = passes.account(&finding);
        assert_eq!(account.inherited, 0);
        assert_eq!(account.held, "a sentence a live claim stands on");
        assert_eq!(
            account.attempts,
            "its edits could not be applied: It did not occur. | its edit overlapped another finding's and was left out"
        );
        let user_held = section(4, "kept as written");
        let with_user = Passes::new(Held {
            places: BTreeSet::from([user_held.place()]),
            ..Held::default()
        });
        assert_eq!(with_user.account(&user_held).held, "already with the user");
    }

    #[test]
    fn a_finding_that_starts_above_zero_says_so_in_its_account() {
        let candidate = serde_json::json!({"blocks":[{"body":"A first sentence. A second sentence."}],"questions":[]});
        let mut passes = Passes::default();
        let first = own(0, "A first sentence.");
        sent(&mut passes, &candidate, vec![first.clone()]);
        passes.worked_on(&first.id(), ["A second sentence.".to_owned()]);
        let second = own(0, "A second sentence.");
        sent(&mut passes, &candidate, vec![second.clone()]);
        assert_eq!(passes.account(&second).inherited, 1);
    }
}

#[cfg(test)]
#[path = "passes_refusal_tests.rs"]
mod refusal_tests;

#[cfg(test)]
#[path = "passes_misses_tests.rs"]
mod misses_tests;
