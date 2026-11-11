//! The reviewer's findings on one material across the passes of a review: a
//! verdict stands while its text is unchanged and the reviewer is given the
//! same inputs. After the reviewer has seen the material it is asked only
//! about what changed since, by each part's digest; no wording is read.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::review_findings::{
    sentences_in, sentences_of, site_sha256, subject_of, FindingId, FindingSite, Held, Origin,
    ReviewFinding,
};
use crate::teaching_review::{label, passages_of, Asked, Judged, Scope};

/// What the reviewer is asked in one pass.
pub(super) struct Round {
    /// The sections whose text changed since the reviewer saw them.
    pub sections: Vec<usize>,
    /// Whether those are every section.
    pub whole: bool,
    /// Whether no verdict stands, so this is a first review of the material
    /// under what the reviewer is given.
    pub first: bool,
    /// The open findings to judge again.
    pub asked: Vec<Asked>,
    /// The open findings whose parts did not change.
    pub known: Vec<Asked>,
}

impl Round {
    pub fn scope(&self) -> Scope<'_> {
        Scope {
            sections: (!self.whole).then_some(self.sections.as_slice()),
            asked: (!(self.first && self.asked.is_empty())).then_some(self.asked.as_slice()),
            known: &self.known,
        }
    }
}

/// Each reference's id and the digest of its excerpt.
pub(super) type Versions = Vec<(String, String)>;

/// Whether a review reached under the references `recorded` still stands
/// with `current` ones: a reference added does not unsettle it, one changed or
/// removed does. The one rule for it, for every review kept across references.
pub(super) fn references_hold(recorded: &[(String, String)], current: &[(String, String)]) -> bool {
    recorded.iter().all(|version| current.contains(version))
}

/// The reviewer's findings on one material.
pub(super) struct Standing {
    /// Each part's digest when the reviewer last saw it.
    reviewed: BTreeMap<FindingSite, String>,
    /// The digest of what the reviewer was given besides the material; empty
    /// until a review.
    basis: String,
    /// The references the verdicts were reached under; none when not known.
    versions: Option<Versions>,
    /// The reviewer's findings that are open, in the order raised.
    open: Vec<ReviewFinding>,
    /// For each open finding, the digests of the sentences it is about: its
    /// passage's when it was raised, and what applied repairs wrote for it.
    about: BTreeMap<FindingId, BTreeSet<String>>,
    next_number: u32,
}

/// The state kept with a draft: maps are pairs, as JSON keys are strings.
#[derive(Deserialize, Serialize)]
struct Saved {
    reviewed: Vec<(FindingSite, String)>,
    #[serde(default)]
    basis: String,
    #[serde(default)]
    versions: Option<Versions>,
    open: Vec<ReviewFinding>,
    about: Vec<(FindingId, Vec<String>)>,
    next_number: u32,
}

fn site_of(finding: &ReviewFinding) -> FindingSite {
    finding.site.unwrap_or(FindingSite::Lesson)
}

/// The digest of every part of `candidate`: the whole, each section, each
/// question.
fn digests(candidate: &Value) -> BTreeMap<FindingSite, String> {
    let count = |field: &str| {
        candidate
            .get(field)
            .and_then(Value::as_array)
            .map_or(0, Vec::len)
    };
    std::iter::once(FindingSite::Lesson)
        .chain((0..count("blocks")).map(FindingSite::Section))
        .chain((0..count("questions")).map(FindingSite::Question))
        .map(|site| (site, site_sha256(candidate, site)))
        .collect()
}

fn sentence_digests<'a>(sentences: impl IntoIterator<Item = &'a String>) -> BTreeSet<String> {
    sentences
        .into_iter()
        .map(|sentence| subject_of(sentence))
        .collect()
}

impl Standing {
    /// A material whose stored reviewer findings, `held`, are open. What their
    /// review was given is not recorded, so the job's first review judges
    /// them again; none is raised twice.
    pub fn new(held: &Held) -> Self {
        Self {
            reviewed: BTreeMap::new(),
            basis: String::new(),
            versions: None,
            open: held
                .reviewer
                .iter()
                .map(|(finding, _)| finding.clone())
                .collect(),
            about: held
                .reviewer
                .iter()
                .filter_map(|(finding, _)| {
                    let passage = finding.passage.as_deref()?;
                    Some((
                        finding.id(),
                        sentence_digests(&sentences_in(&Value::String(passage.to_owned()))),
                    ))
                })
                .collect(),
            next_number: held.next_number.max(1),
        }
    }

    /// The state to keep with the draft a repair produced.
    pub fn saved(&self) -> serde_json::Result<Value> {
        serde_json::to_value(Saved {
            reviewed: self
                .reviewed
                .iter()
                .map(|(site, digest)| (*site, digest.clone()))
                .collect(),
            basis: self.basis.clone(),
            versions: self.versions.clone(),
            open: self.open.clone(),
            about: self
                .about
                .iter()
                .map(|(id, sentences)| (id.clone(), sentences.iter().cloned().collect()))
                .collect(),
            next_number: self.next_number,
        })
    }

    /// Takes up the state a review cut short saved with this draft. It holds
    /// every finding the reviewer had open, so it replaces what the stored
    /// rows gave; a saved string without it leaves the state as it was.
    pub fn resume(&mut self, saved: &str) {
        let kept = serde_json::from_str::<Value>(saved)
            .ok()
            .and_then(|saved| saved.get("reviewer").cloned())
            .and_then(|kept| serde_json::from_value::<Saved>(kept).ok());
        let Some(kept) = kept else {
            return;
        };
        self.reviewed = kept.reviewed.into_iter().collect();
        self.basis = kept.basis;
        self.versions = kept.versions;
        self.open = kept.open;
        self.about = kept
            .about
            .into_iter()
            .map(|(id, sentences)| (id, sentences.into_iter().collect()))
            .collect();
        self.next_number = self.next_number.max(kept.next_number);
    }

    /// Notes what the reviewer is given besides the material. When that is
    /// not what its verdicts were given, none stands: every part is
    /// reviewed again, and every open finding is asked about.
    pub fn under(&mut self, basis: &str) {
        if self.basis != basis {
            self.reviewed.clear();
            self.basis = basis.to_owned();
        }
    }

    /// Notes the references the reviewer is given. Verdicts reached under
    /// others stand while `references_hold`; saved state that does not say
    /// which they were is not trusted.
    pub fn against(&mut self, current: &[(String, String)]) {
        let stands = self
            .versions
            .as_deref()
            .is_some_and(|recorded| references_hold(recorded, current));
        if !stands {
            self.reviewed.clear();
        }
        self.versions = Some(current.to_vec());
    }

    /// Whether the part at `site` is not as the reviewer last saw it.
    fn changed(&self, candidate: &Value, site: FindingSite) -> bool {
        self.reviewed.get(&site) != Some(&site_sha256(candidate, site))
    }

    /// The passage `finding` is about in `candidate` now: the first of its
    /// section's whose text holds a sentence the finding is about.
    fn passage_now(&self, candidate: &Value, finding: &ReviewFinding) -> Option<(String, String)> {
        let Some(FindingSite::Section(index)) = finding.site else {
            return None;
        };
        let about = self.about.get(&finding.id())?;
        passages_of(candidate)
            .into_iter()
            .nth(index)?
            .into_iter()
            .find(|(_, text)| {
                sentences_in(&Value::String(text.clone()))
                    .iter()
                    .any(|sentence| about.contains(&subject_of(sentence)))
            })
    }

    /// Whether the reviewer's verdict on `finding` no longer stands: a part
    /// it is about changed since the reviewer saw it. A finding about the
    /// whole material is touched whenever the material changed.
    fn touched(&self, candidate: &Value, finding: &ReviewFinding) -> bool {
        self.changed(candidate, site_of(finding))
            || finding
                .also
                .is_some_and(|also| self.changed(candidate, also))
    }

    fn shown(&self, candidate: &Value, finding: &ReviewFinding) -> Asked {
        Asked {
            id: finding.id(),
            site: site_of(finding),
            passage_id: self.passage_now(candidate, finding).map(|(id, _)| id),
            note: finding.note.clone(),
        }
    }

    /// What the reviewer is asked of `candidate`; none when it has seen
    /// exactly this material under what it is given, so its findings stand.
    pub fn round(&self, candidate: &Value) -> Option<Round> {
        if !self.changed(candidate, FindingSite::Lesson) {
            return None;
        }
        let count = candidate
            .get("blocks")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        let sections: Vec<usize> = (0..count)
            .filter(|index| self.changed(candidate, FindingSite::Section(*index)))
            .collect();
        let whole = sections.len() == count;
        let (mut asked, mut known) = (Vec::new(), Vec::new());
        for finding in &self.open {
            let shown = self.shown(candidate, finding);
            if self.touched(candidate, finding) {
                asked.push(shown);
            } else {
                known.push(shown);
            }
        }
        Some(Round {
            sections,
            whole,
            first: self.reviewed.is_empty(),
            asked,
            known,
        })
    }

    /// Takes the reviewer's verdicts into the open findings, numbers what is
    /// new, and returns the new findings, not yet open.
    pub fn judged(
        &mut self,
        candidate: &Value,
        round: &Round,
        judged: Judged,
    ) -> Vec<ReviewFinding> {
        let Judged { verdicts, reported } = judged;
        // A report that names an open finding asked about is that finding.
        let continued: Vec<Option<FindingId>> = reported
            .iter()
            .map(|report| {
                report.continues.clone().filter(|id| {
                    round.asked.iter().any(|asked| asked.id == *id)
                        && self.open.iter().any(|finding| finding.id() == *id)
                })
            })
            .collect();
        let settled: Vec<(FindingId, bool, String)> = verdicts
            .into_iter()
            .map(|verdict| (verdict.id, verdict.present, verdict.note))
            .collect();
        let mut decided: BTreeMap<FindingId, (bool, String)> = settled
            .iter()
            .map(|(id, present, note)| (id.clone(), (*present, note.clone())))
            .collect();
        for id in continued.iter().flatten() {
            decided
                .entry(id.clone())
                .or_insert_with(|| (false, String::new()));
        }
        for (id, (present, note)) in decided {
            let Some(position) = self.open.iter().position(|finding| finding.id() == id) else {
                continue;
            };
            let mut reports = reported
                .iter()
                .zip(&continued)
                .filter(|(_, continues)| continues.as_ref() == Some(&id))
                .map(|(report, _)| report);
            let report = reports.next();
            for _ in reports {
                tracing::info!(
                    finding = %id.as_str(),
                    "A second report continues the same open finding and is not kept apart"
                );
            }
            if let Some(report) = report {
                if report.site.is_some() {
                    let number = match self.open.get(position).map(|finding| &finding.origin) {
                        Some(Origin::Reviewer { number }) => *number,
                        _ => continue,
                    };
                    let replaced = report.clone().finding(number);
                    self.about.insert(
                        id.clone(),
                        sentence_digests(&sentences_of(candidate, &replaced)),
                    );
                    if let Some(finding) = self.open.get_mut(position) {
                        *finding = replaced;
                    }
                } else {
                    let now = self
                        .open
                        .get(position)
                        .and_then(|finding| self.passage_now(candidate, finding))
                        .map(|(_, text)| text);
                    if let Some(finding) = self.open.get_mut(position) {
                        finding.note = report.note.clone();
                        finding.summary = report.summary.clone();
                        finding.passage = now;
                    }
                }
                continue;
            }
            if !present {
                self.open.remove(position);
                self.about.remove(&id);
                continue;
            }
            let now = self
                .open
                .get(position)
                .and_then(|finding| self.passage_now(candidate, finding))
                .map(|(_, text)| text);
            if let Some(finding) = self.open.get_mut(position) {
                if !note.is_empty() {
                    finding.note = note.clone();
                    finding.summary = note;
                }
                finding.passage = now;
            }
        }
        let mut fresh = Vec::new();
        for (report, continues) in reported.into_iter().zip(continued) {
            if continues.is_some() {
                continue;
            }
            let folded = report.passage_id.as_deref().and_then(|passage| {
                round.asked.iter().find(|asked| {
                    asked.passage_id.as_deref() == Some(passage)
                        && settled
                            .iter()
                            .any(|(id, present, _)| *present && *id == asked.id)
                })
            });
            if let Some(asked) = folded {
                let blank = settled
                    .iter()
                    .any(|(id, _, note)| *id == asked.id && note.is_empty());
                if let Some(finding) = self
                    .open
                    .iter_mut()
                    .find(|finding| finding.id() == asked.id)
                {
                    if blank {
                        finding.note = report.note;
                        finding.summary = report.summary;
                    }
                }
                continue;
            }
            fresh.push(report.finding(self.next_number));
            self.next_number += 1;
        }
        fresh
    }

    /// Opens the new findings that are to be kept, and notes the material as
    /// the reviewer has now seen it. A finding is kept when it is about the
    /// whole material or about a part, or a second part, that changed since
    /// the reviewer last saw it.
    pub fn admit(&mut self, candidate: &Value, established: Vec<ReviewFinding>) {
        for finding in established {
            if !self.touched(candidate, &finding) {
                tracing::info!(
                    part = %label(site_of(&finding)),
                    "A new finding is about text the reviewer had already judged and is not kept"
                );
                continue;
            }
            self.about.insert(
                finding.id(),
                sentence_digests(&sentences_of(candidate, &finding)),
            );
            self.open.push(finding);
        }
        self.reviewed = digests(candidate);
    }

    /// The reviewer's open findings, in the order raised.
    pub fn open(&self) -> &[ReviewFinding] {
        &self.open
    }

    /// An applied repair wrote or worked on these sentences for `id`: a
    /// finding follows them into the passage they are in.
    pub fn worked_on(&mut self, id: &FindingId, sentences: impl IntoIterator<Item = String>) {
        if self.open.iter().any(|finding| finding.id() == *id) {
            self.about
                .entry(id.clone())
                .or_default()
                .extend(sentences.into_iter().map(|sentence| subject_of(&sentence)));
        }
    }
}
