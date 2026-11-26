//! The reviewer's findings on one material across the passes of a review: a
//! verdict stands while the material is unchanged and the reviewer is given
//! the same inputs. After the reviewer has seen the material, a change asks
//! about every open finding and has it read only the sections that changed,
//! by each part's digest; no wording is read.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::dto::LearningSourceDto;
use crate::review_findings::{
    part_at, sentences_in, sentences_of, subject_of, FindingId, FindingSite, Held, ReviewFinding,
};
use crate::teaching_review::{passages_of, Asked, Judged, Scope};
use lattice_core::domain::learning::claim_review::text_sha256;
use lattice_core::shared::hash::sha256_hex;

/// Fields of a lesson block or question that carry only source citations;
/// teaching review does not check citations, so editing them alone leaves a
/// part's review digest unchanged.
const CITATION_FIELDS: [&str; 3] = ["sourceIndex", "quote", "citations"];

/// What the reviewer is asked in one pass.
pub(super) struct Round {
    /// The sections whose text changed since the reviewer saw them.
    pub sections: Vec<usize>,
    /// The questions whose text changed since the reviewer saw them.
    #[cfg(test)]
    pub questions: Vec<usize>,
    /// Whether those are every section.
    pub whole: bool,
    /// Whether no verdict stands, so this is a first review of the material
    /// under what the reviewer is given.
    pub first: bool,
    /// Every open finding, to judge again.
    pub asked: Vec<Asked>,
}

impl Round {
    pub fn scope(&self) -> Scope<'_> {
        Scope {
            sections: (!self.whole).then_some(self.sections.as_slice()),
            asked: (!(self.first && self.asked.is_empty())).then_some(self.asked.as_slice()),
        }
    }
}

/// Each reference's id and the digest of its excerpt.
pub(crate) type Versions = Vec<(String, String)>;

/// Each source's id and the digest of its excerpt, sorted.
pub(crate) fn versions(sources: &[LearningSourceDto]) -> Versions {
    let mut versions: Versions = sources
        .iter()
        .map(|source| (source.id.clone(), sha256_hex(&source.excerpt)))
        .collect();
    versions.sort();
    versions
}

/// Whether a review reached under the references `recorded` still stands
/// with `current` ones: a reference added does not unsettle it, one changed or
/// removed does. The one rule for it, for every review kept across references.
pub(crate) fn references_hold(recorded: &[(String, String)], current: &[(String, String)]) -> bool {
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

/// A copy of a block or question without its citation-only fields.
fn without_part_citations(part: &Value) -> Value {
    let Some(fields) = part.as_object() else {
        return part.clone();
    };
    let mut stripped = fields.clone();
    for field in CITATION_FIELDS {
        stripped.shift_remove(field);
    }
    Value::Object(stripped)
}

/// A copy of `candidate` with citation-only fields removed from every block and
/// question, so citation-only edits produce the same lesson digest.
fn without_lesson_citations(candidate: &Value) -> Value {
    let Some(fields) = candidate.as_object() else {
        return candidate.clone();
    };
    let mut stripped = fields.clone();
    for array_key in ["blocks", "questions"] {
        if let Some(items) = stripped.get_mut(array_key).and_then(Value::as_array_mut) {
            for item in items {
                *item = without_part_citations(item);
            }
        }
    }
    Value::Object(stripped)
}

/// The review digest of the part at `site`, ignoring citation-only fields.
fn part_digest(candidate: &Value, site: FindingSite) -> String {
    match site {
        FindingSite::Lesson => text_sha256(&without_lesson_citations(candidate).to_string()),
        FindingSite::Section(_) | FindingSite::Question(_) => part_at(candidate, site)
            .map(|part| text_sha256(&without_part_citations(part).to_string()))
            .unwrap_or_default(),
    }
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
        .map(|site| (site, part_digest(candidate, site)))
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
        self.reviewed.get(&site) != Some(&part_digest(candidate, site))
    }

    /// Whether any section or question that was previously reviewed was removed.
    fn part_removed(&self, block_count: usize, question_count: usize) -> bool {
        self.reviewed.keys().any(|site| match site {
            FindingSite::Section(index) => *index >= block_count,
            FindingSite::Question(index) => *index >= question_count,
            FindingSite::Lesson => false,
        })
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

    fn shown(&self, candidate: &Value, finding: &ReviewFinding) -> Asked {
        Asked {
            id: finding.id(),
            site: site_of(finding),
            passage_id: self.passage_now(candidate, finding).map(|(id, _)| id),
            note: finding.note.clone(),
        }
    }

    /// What the reviewer is asked of `candidate`; none when it has seen
    /// exactly this material (or only citation-only fields changed) under what
    /// it is given, so its findings stand. Otherwise it judges every open
    /// finding again: a change anywhere may mend a defect elsewhere.
    pub fn round(&self, candidate: &Value) -> Option<Round> {
        if !self.changed(candidate, FindingSite::Lesson) {
            return None;
        }
        let array_len = |field: &str| {
            candidate
                .get(field)
                .and_then(Value::as_array)
                .map_or(0, Vec::len)
        };
        let block_count = array_len("blocks");
        let question_count = array_len("questions");
        let sections: Vec<usize> = (0..block_count)
            .filter(|index| self.changed(candidate, FindingSite::Section(*index)))
            .collect();
        let questions: Vec<usize> = (0..question_count)
            .filter(|index| self.changed(candidate, FindingSite::Question(*index)))
            .collect();
        let whole = sections.len() == block_count;
        let asked: Vec<Asked> = self
            .open
            .iter()
            .map(|finding| self.shown(candidate, finding))
            .collect();
        let is_lesson = candidate.get("blocks").is_some() || candidate.get("questions").is_some();
        if is_lesson
            && !self.reviewed.is_empty()
            && sections.is_empty()
            && questions.is_empty()
            && asked.is_empty()
            && !self.part_removed(block_count, question_count)
        {
            return None;
        }
        Some(Round {
            sections,
            #[cfg(test)]
            questions,
            whole,
            first: self.reviewed.is_empty(),
            asked,
        })
    }

    /// Takes the reviewer's verdicts into the open findings, numbers what is
    /// new, and returns the new findings, not yet open. An open finding with
    /// no verdict the reply could give stays as it stood.
    pub fn judged(
        &mut self,
        candidate: &Value,
        round: &Round,
        judged: Judged,
    ) -> Vec<ReviewFinding> {
        let Judged { verdicts, reported } = judged;
        for verdict in verdicts {
            let Some(position) = self
                .open
                .iter()
                .position(|finding| finding.id() == verdict.id)
            else {
                continue;
            };
            if !verdict.present {
                let gone = self.open.remove(position);
                crate::lessons::trace::decided_finding(
                    crate::lessons::trace::Stage::TeachingReview,
                    &gone,
                    "no longer present",
                    "the reviewer judged the defect gone",
                );
                self.about.remove(&verdict.id);
                continue;
            }
            let now = self
                .open
                .get(position)
                .and_then(|finding| self.passage_now(candidate, finding));
            let Some(finding) = self.open.get_mut(position) else {
                continue;
            };
            if let Some((_, text)) = now {
                finding.passage = Some(text);
            }
            crate::lessons::trace::decided_finding(
                crate::lessons::trace::Stage::TeachingReview,
                finding,
                "still present",
                "the reviewer judged it again",
            );
            let written = sentence_digests(&sentences_of(candidate, finding));
            self.about.entry(verdict.id).or_default().extend(written);
        }
        let present: BTreeSet<FindingId> = round
            .asked
            .iter()
            .map(|asked| asked.id.clone())
            .filter(|id| self.open.iter().any(|finding| finding.id() == *id))
            .collect();
        let mut fresh = Vec::new();
        for report in reported {
            if report
                .continues
                .as_ref()
                .is_some_and(|id| present.contains(id))
            {
                tracing::info!(
                    "A report continues an open finding that is still present and is dropped"
                );
                continue;
            }
            let folded = report.passage_id.as_deref().is_some_and(|passage| {
                round.asked.iter().any(|asked| {
                    asked.passage_id.as_deref() == Some(passage) && present.contains(&asked.id)
                })
            });
            if folded {
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
            let changed = self.changed(candidate, site_of(&finding))
                || finding
                    .also
                    .is_some_and(|also| self.changed(candidate, also));
            if !changed {
                tracing::info!(
                    part = %site_of(&finding).label(),
                    "A new finding is about text the reviewer had already judged and is not kept"
                );
                crate::lessons::trace::decided_finding(
                    crate::lessons::trace::Stage::TeachingReview,
                    &finding,
                    "not kept",
                    "it is about text the reviewer had already judged",
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
