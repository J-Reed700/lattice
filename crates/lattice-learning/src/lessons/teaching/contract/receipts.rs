//! What a review has seen taught, and what the lesson-against-itself check
//! (`LessonContractFind`) already cleared. An item the check found taught keeps
//! the sentences of the lesson its words lie in; while every one of them is
//! still in the lesson unchanged, the item is not asked about again, so a later
//! pass cannot unsay it and spend a repair.
use super::reading::Hit;
use super::{Item, Part};
use lattice_core::shared::hash::sha256_hex;
use lattice_core::shared::text::same_text;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Normalized sentence fingerprint of one part of a lesson when
/// `LessonContractFind` passed cleanly.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(super) struct PartDigest {
    pub(super) id: String,
    #[serde(default)]
    pub(super) correct_index: Option<u64>,
    pub(super) sentences: Vec<String>,
}

/// Persisted state of a clean `LessonContractFind` pass.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(super) struct FindReceipt {
    pub(super) course_digest: String,
    pub(super) parts: Vec<PartDigest>,
}

/// How `LessonContractFind` should run given any prior clean `FindReceipt`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum FindPlan {
    /// No prior clean find receipt, course inputs changed, or every sentence changed.
    Full,
    /// Normalized lesson sentences (and question keys) match a prior clean pass.
    Skip,
    /// Every remaining sentence existed in the prior clean lesson (pure deletion/reorder):
    /// scope `usedBeforeTaught` to parts at or after `from_part_order`, while still
    /// checking contradictions and dangling `unseen` references across remaining text.
    DeletionOnly { from_part_order: usize },
    /// Some sentences were added/edited (`Δ`): scope `usedBeforeTaught` to `from_part_order..`,
    /// `contradiction` to pairs involving `Δ`, and `unseen` to `changed_parts`.
    Edited {
        from_part_order: usize,
        changed_parts: Vec<usize>,
        delta_by_part: Vec<Vec<usize>>,
    },
}

/// Normalized digest of one sentence.
pub(super) fn sentence_digest(sentence: &str) -> String {
    sha256_hex(same_text::fold(sentence))
}

#[derive(Default, Deserialize, Serialize)]
struct SavedReceipts {
    sentences: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    find: Option<FindReceipt>,
}

/// The sentences each taught item was found in, by item id, and the last clean
/// `LessonContractFind` fingerprint.
#[derive(Default)]
pub(crate) struct Receipts {
    sentences: BTreeMap<String, Vec<String>>,
    find: Option<FindReceipt>,
}

impl Receipts {
    /// The receipts, to keep with the draft.
    pub(in crate::lessons::teaching) fn saved(&self) -> serde_json::Result<Value> {
        serde_json::to_value(SavedReceipts {
            sentences: self.sentences.clone(),
            find: self.find.clone(),
        })
    }

    /// Takes up the receipts a review saved with the job's draft; a saved string without them leaves them as they were.
    pub(in crate::lessons::teaching) fn resume(&mut self, saved: &str) {
        let Some(kept) = serde_json::from_str::<Value>(saved)
            .ok()
            .and_then(|saved| saved.get("receipts").cloned())
        else {
            return;
        };
        if let Ok(full) = serde_json::from_value::<SavedReceipts>(kept.clone()) {
            self.sentences = full.sentences;
            self.find = full.find;
        } else if let Ok(legacy) = serde_json::from_value::<BTreeMap<String, Vec<String>>>(kept) {
            self.sentences = legacy;
            self.find = None;
        }
    }

    /// Forget every receipt that has a sentence the lesson no longer holds as it was.
    pub(super) fn lapse(&mut self, parts: &[Part]) {
        self.sentences.retain(|_, receipted| {
            receipted.iter().all(|sentence| {
                parts
                    .iter()
                    .flat_map(|part| &part.texts)
                    .any(|text| text.contains(sentence.as_str()))
            })
        });
    }

    /// Whether `item` has a receipt that still stands.
    pub(super) fn holds(&self, item: &Item) -> bool {
        self.sentences.contains_key(&item.id)
    }

    /// Keep the sentences `hits` are as `item`'s receipt.
    pub(super) fn record(&mut self, item: &Item, hits: &[Hit]) {
        let lying_in: Vec<String> = hits
            .iter()
            .filter_map(|hit| {
                let text = hit.part.texts.get(hit.found.text)?;
                same_text::sentences_over(text, (hit.found.start, hit.found.end))
                    .into_iter()
                    .map(|(start, end)| text.get(start..end).map(str::to_owned))
                    .collect::<Option<Vec<String>>>()
            })
            .flatten()
            .collect();
        if !lying_in.is_empty() {
            self.sentences.insert(item.id.clone(), lying_in);
        }
    }

    /// Records a clean `LessonContractFind` pass for `course_digest` and `parts`.
    pub(super) fn record_find(&mut self, course_digest: String, parts: Vec<PartDigest>) {
        self.find = Some(FindReceipt {
            course_digest,
            parts,
        });
    }

    /// Plans how `LessonContractFind` should run against `current` under `course_digest`.
    pub(super) fn find_plan(&self, course_digest: &str, current: &[PartDigest]) -> FindPlan {
        let Some(prev) = &self.find else {
            return FindPlan::Full;
        };
        if prev.course_digest != course_digest {
            return FindPlan::Full;
        }
        if prev.parts == current {
            return FindPlan::Skip;
        }
        let shared = prev.parts.len().min(current.len());
        let from_part_order = (0..shared)
            .find(|index| prev.parts.get(*index) != current.get(*index))
            .unwrap_or(shared);
        if from_part_order >= current.len() {
            // Trailing parts were removed: earlier parts cannot have new
            // usedBeforeTaught, but an earlier sentence could point "below" at
            // a removed part (`unseen`) or lose a qualifier/exception that
            // prevented a contradiction, so still run the deletion-scoped check
            // with empty `focusParts`.
            return FindPlan::DeletionOnly {
                from_part_order: current.len(),
            };
        }
        let known_sentences: BTreeSet<&str> = prev
            .parts
            .iter()
            .flat_map(|part| part.sentences.iter().map(String::as_str))
            .collect();
        let mut changed_parts = Vec::new();
        let mut delta_by_part = Vec::with_capacity(current.len());
        let mut total_sentences = 0usize;
        let mut delta_sentences = 0usize;
        for (index, part) in current.iter().enumerate() {
            total_sentences += part.sentences.len();
            let key_changed = prev
                .parts
                .get(index)
                .is_some_and(|before| before.correct_index != part.correct_index);
            let delta: Vec<usize> = if key_changed {
                (0..part.sentences.len()).collect()
            } else {
                part.sentences
                    .iter()
                    .enumerate()
                    .filter(|(_, digest)| !known_sentences.contains(digest.as_str()))
                    .map(|(sent_idx, _)| sent_idx)
                    .collect()
            };
            if !delta.is_empty() {
                changed_parts.push(index);
                delta_sentences += delta.len();
            }
            delta_by_part.push(delta);
        }
        if changed_parts.is_empty() {
            let has_focus_sentences = current
                .get(from_part_order..)
                .is_some_and(|rest| rest.iter().any(|part| !part.sentences.is_empty()));
            if has_focus_sentences {
                FindPlan::DeletionOnly { from_part_order }
            } else {
                FindPlan::Skip
            }
        } else if from_part_order == 0
            && changed_parts.len() == current.len()
            && delta_sentences == total_sentences
        {
            FindPlan::Full
        } else {
            FindPlan::Edited {
                from_part_order,
                changed_parts,
                delta_by_part,
            }
        }
    }
}
