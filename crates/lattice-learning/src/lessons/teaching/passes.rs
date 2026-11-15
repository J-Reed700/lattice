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
//! new id, so the repairs are also counted per kind of defect (what raised
//! the finding and which of its checks, never its place or subject) on the
//! words they worked on: a new finding about those words starts at the most
//! repairs of its own kind they have been through, and text a repair writes
//! carries every kind's count of the words it was written for. Along any chain
//! of findings about words a repair wrote, every charged repair raises one
//! kind's count on those words, a count stops at `REPAIRS_PER_FINDING`, and
//! the kinds are finitely many, so the review still ends. A new reviewer
//! finding with no passage has no words to count, so it starts at the count
//! of the reviewer's findings with no passage already at its place.
use std::collections::{BTreeMap, BTreeSet};

use crate::lessons::trace::{self, Stage};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::review_findings::{
    sentences_in, sentences_of, subject_of, FindingId, FindingSite, Held, Origin, ReviewFinding,
    SetAside,
};

/// Attempts one finding is given: a first, and one retry that is told what
/// became of the first. The loop must end on a defect no repair fixes.
pub(in crate::lessons::teaching) const REPAIRS_PER_FINDING: u8 = 2;

/// What became of one repair's attempt at a finding.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub(super) enum Outcome {
    /// A change naming it was applied and stands.
    Applied,
    /// It was named, and none of its changes could be applied.
    NotApplied(Vec<String>),
    /// Its changes were applied and put back: they touched a sentence that must stay.
    Reverted,
    /// Its correction as a whole was refused: it changed what must stay or what it was not asked to change.
    Refused(Vec<String>),
    /// No change named it.
    Unnamed,
    /// Its entry in the repair's answer gave a plan and no change.
    Declined(String),
    /// The repair's answer could not be used at all.
    Unusable(String),
    /// Its edit overlapped another finding's and was left out: not an attempt.
    Deferred,
}

impl Outcome {
    /// The attempt changed nothing for the finding.
    fn is_miss(&self) -> bool {
        matches!(
            self,
            Self::Unnamed | Self::Declined(_) | Self::NotApplied(_)
        )
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
            Self::Refused(reasons) => {
                format!("its correction was refused: {}", reasons.join(" "))
            }
            Self::Unnamed => "the repair made no change for it".to_owned(),
            Self::Declined(plan) => {
                format!("the repair gave no change for it; its plan was: \"{plan}\"")
            }
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
    /// Sentence digest to the repairs those words have been through, per kind
    /// of defect.
    worked: BTreeMap<String, BTreeMap<String, u8>>,
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
    worked: BTreeMap<String, BTreeMap<String, u8>>,
    #[serde(default)]
    passage_less: Vec<(FindingSite, u8)>,
    #[serde(default)]
    attempts: Vec<(FindingId, Vec<Outcome>)>,
    #[serde(default)]
    inherited: Vec<(FindingId, u8)>,
}

/// Raises `kind`'s count in `counts` to at least `count`.
fn raise(counts: &mut BTreeMap<String, u8>, kind: &str, count: u8) {
    if count > 0 {
        let held = counts.entry(kind.to_owned()).or_default();
        *held = (*held).max(count);
    }
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
            let kind = finding.origin.kind();
            let mut start = about
                .iter()
                .filter_map(|digest| self.worked.get(digest)?.get(&kind).copied())
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
        trace::decided_finding(
            Stage::TeachingRepair,
            finding,
            "charged",
            &format!("{outcome:?}"),
        );
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
            let counts = self.worked.entry(digest.clone()).or_default();
            raise(counts, &finding.origin.kind(), own);
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
            Outcome::Refused(reasons) => format!(
                "The last repair's correction for this finding was refused, and none of its edits were applied: {}",
                reasons.join(" ")
            ),
            Outcome::Unnamed => "The last repair gave no edit for this finding.".to_owned(),
            Outcome::Declined(plan) => format!("The last repair gave no change for this finding; its plan was: \"{plan}\""),
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

    /// An applied repair for `finding` put these sentences through the repairs
    /// of every kind the words it is about have been through, and through the
    /// repairs `finding` has been charged under its own kind: what it wrote,
    /// or the words it was applied for.
    pub fn worked_on(
        &mut self,
        finding: &ReviewFinding,
        sentences: impl IntoIterator<Item = String>,
    ) {
        let mut carried = BTreeMap::new();
        for counts in self
            .words
            .get(&finding.id())
            .into_iter()
            .flatten()
            .filter_map(|digest| self.worked.get(digest))
        {
            for (kind, count) in counts {
                raise(&mut carried, kind, *count);
            }
        }
        let spent = self.spent.get(&finding.id()).copied().unwrap_or_default();
        raise(&mut carried, &finding.origin.kind(), spent);
        for sentence in sentences {
            let counts = self.worked.entry(subject_of(&sentence)).or_default();
            for (kind, count) in &carried {
                raise(counts, kind, *count);
            }
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
#[path = "passes_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "passes_refusal_tests.rs"]
mod refusal_tests;

#[cfg(test)]
#[path = "passes_misses_tests.rs"]
mod misses_tests;

#[cfg(test)]
#[path = "passes_kinds_tests.rs"]
mod kinds_tests;
