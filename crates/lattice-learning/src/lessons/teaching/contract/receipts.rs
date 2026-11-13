//! What a review has seen taught. An item the check found taught keeps the
//! sentences of the lesson its words lie in; while every one of them is still
//! in the lesson unchanged, the item is not asked about again, so a later
//! pass cannot unsay it and spend a repair.
use super::{Hit, Item, Part};
use lattice_core::shared::text::same_text;
use serde_json::Value;
use std::collections::BTreeMap;

/// The sentences each taught item was found in, by item id.
#[derive(Default)]
pub(crate) struct Receipts {
    sentences: BTreeMap<String, Vec<String>>,
}

impl Receipts {
    /// The receipts, to keep with the draft.
    pub(in crate::lessons::teaching) fn saved(&self) -> serde_json::Result<Value> {
        serde_json::to_value(&self.sentences)
    }

    /// Takes up the receipts a review saved with the job's draft; a saved string without them leaves them as they were.
    pub(in crate::lessons::teaching) fn resume(&mut self, saved: &str) {
        let kept = serde_json::from_str::<Value>(saved)
            .ok()
            .and_then(|saved| saved.get("receipts").cloned())
            .and_then(|kept| serde_json::from_value::<BTreeMap<String, Vec<String>>>(kept).ok());
        if let Some(kept) = kept {
            self.sentences = kept;
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

    /// Keep the sentences `hit` lies in as `item`'s receipt.
    pub(super) fn record(&mut self, item: &Item, hit: &Hit) {
        let Some(text) = hit.part.texts.get(hit.found.text) else {
            return;
        };
        let lying_in: Vec<String> =
            same_text::sentences_over(text, (hit.found.start, hit.found.end))
                .into_iter()
                .filter_map(|(start, end)| text.get(start..end))
                .map(str::to_owned)
                .collect();
        if !lying_in.is_empty() {
            self.sentences.insert(item.id.clone(), lying_in);
        }
    }
}
