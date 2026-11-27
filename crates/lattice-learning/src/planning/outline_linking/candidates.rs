use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::Result;
use lattice_core::shared::hash::sha256_hex;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use super::spans::{citable_span, is_prose};
use super::{CitationReceipt, CitationReceipts, ItemKind, LinkLog, POLICY};
use crate::{
    dto::LearningSourceDto,
    generation,
    outline_draft::OutlineDraft,
    reference_collection::{ReferenceCollection, ReferencePassage},
};

/// Every citable field of a candidate with its JSON pointer and kind.
pub(super) fn fields(candidate: &Value, module: Option<usize>) -> Vec<(String, ItemKind)> {
    let mut out = Vec::new();
    for (index, module_value) in candidate["modules"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        if module.is_some_and(|only| only != index) {
            continue;
        }
        out.push((format!("/modules/{index}/summary"), ItemKind::Summary));
        for (k, _) in module_value["outcomes"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            out.push((format!("/modules/{index}/outcomes/{k}"), ItemKind::Outcome));
        }
        for (j, _) in module_value["lessons"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            out.push((format!("/modules/{index}/lessons/{j}"), ItemKind::Lesson));
        }
    }
    out
}

pub(super) fn strip(field: &mut Value) {
    if let Some(object) = field.as_object_mut() {
        object.insert("sourceIndex".into(), Value::Null);
        object.insert("quote".into(), json!(""));
    }
}

pub(super) fn cite(field: &mut Value, index: usize, quote: &str) {
    if let Some(object) = field.as_object_mut() {
        object.insert("sourceIndex".into(), json!(index));
        object.insert("quote".into(), json!(quote));
    }
}

pub(super) fn citation(field: &Value) -> Option<(usize, &str)> {
    let index = usize::try_from(field["sourceIndex"].as_u64()?).ok()?;
    Some((index, field["quote"].as_str()?))
}

/// The content hash of each source, in index order. Receipts bind to the
/// text that was judged, not to a source id that could be re-captured.
pub(crate) fn source_hashes(sources: &[LearningSourceDto]) -> Vec<String> {
    sources
        .iter()
        .map(|source| sha256_hex(source.excerpt.trim()))
        .collect()
}

pub(crate) fn receipt_key(
    model: &str,
    kind: ItemKind,
    text: &str,
    source_hash: &str,
    quote: &str,
) -> String {
    sha256_hex(
        json!({"policy":POLICY,"model":model,"kind":kind.name(),"text":text.trim(),"source":source_hash,"quote":quote.trim()})
            .to_string(),
    )
}

/// The key of a declined candidate set: the same item under the same model,
/// offered exactly these passages, is not asked about again.
pub(super) fn declined_key(
    model: &str,
    kind: ItemKind,
    text: &str,
    candidates: &[Candidate],
) -> String {
    sha256_hex(
        json!({"policy":POLICY,"model":model,"kind":kind.name(),"text":text.trim(),"declined":candidates.iter().map(|c| json!({"source":c.hash,"quote":c.quote})).collect::<Vec<_>>()})
            .to_string(),
    )
}

/// Whether `field` carries a citation the chooser accepted for its item.
pub(crate) fn is_verified(
    receipts: &CitationReceipts,
    model: &str,
    kind: ItemKind,
    field: &Value,
    sources: &[LearningSourceDto],
    hashes: &[String],
) -> bool {
    let (Some(text), Some((index, quote))) = (kind.text(field), citation(field)) else {
        return false;
    };
    let Some(hash) = hashes.get(index) else {
        return false;
    };
    if generation::optional_citation(sources, Some(index), quote).is_err() {
        return false;
    }
    receipts
        .get(&receipt_key(model, kind, text, hash, quote))
        .is_some_and(|receipt| receipt.covers)
}

/// Remove every citation whose quote does not match its source exactly.
/// Returns how many were removed. Typography should be restored first.
pub(crate) fn strip_unmatched(
    candidate: &mut Value,
    sources: &[LearningSourceDto],
    module: Option<usize>,
) -> u32 {
    let mut removed = 0;
    for (path, _) in fields(candidate, module) {
        let Some(field) = candidate.pointer_mut(&path) else {
            continue;
        };
        let index = field["sourceIndex"]
            .as_u64()
            .and_then(|i| usize::try_from(i).ok());
        let quote = field["quote"].as_str().unwrap_or_default();
        if index.is_none() && quote.trim().is_empty() {
            continue;
        }
        if generation::optional_citation(sources, index, quote).is_err() {
            strip(field);
            removed += 1;
        }
    }
    removed
}

/// How many items in the candidate currently carry a citation.
pub fn cited_count(candidate: &Value) -> u32 {
    fields(candidate, None)
        .iter()
        .filter(|(path, _)| {
            candidate
                .pointer(path)
                .and_then(|field| field["sourceIndex"].as_u64())
                .is_some()
        })
        .count() as u32
}

/// How many items in the candidate can carry a citation.
pub fn item_count(candidate: &Value) -> u32 {
    fields(candidate, None).len() as u32
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Candidate {
    pub(super) source_index: usize,
    pub(super) hash: String,
    pub(super) quote: String,
}

pub(super) struct Pending {
    pub(super) path: String,
    pub(super) kind: ItemKind,
    pub(super) text: String,
    pub(super) module_title: String,
    pub(super) lesson_title: Option<String>,
    pub(super) candidates: Vec<Candidate>,
    /// The author's exact-match citation, also present in `candidates`.
    pub(super) author: Option<Candidate>,
}

impl Pending {
    fn query(&self) -> String {
        let mut query = format!("{} {}", self.text, self.module_title);
        if let Some(title) = &self.lesson_title {
            query.push(' ');
            query.push_str(title);
        }
        query
    }
}

/// A passage's quote for one item: its source's index, the quote, and the
/// tokens it costs once offered. `None` when the passage has no citable span.
type Offer = Option<(usize, String, usize)>;

/// Each passage's offer for one item, worked out the first time retrieval
/// measures it and reused for every later page and for the candidate list.
struct Offers<'a> {
    llm: &'a dyn LLMPort,
    sources: &'a [LearningSourceDto],
    query: &'a str,
    known: Mutex<HashMap<(String, usize, usize), Offer>>,
}

impl Offers<'_> {
    fn of<T>(&self, passage: &ReferencePassage, read: impl FnOnce(&Offer) -> T) -> T {
        let mut known = self.known.lock().unwrap_or_else(PoisonError::into_inner);
        let offer = known
            .entry((
                passage.source_id.clone(),
                passage.start_byte,
                passage.end_byte,
            ))
            .or_insert_with(|| {
                let (index, source) = self
                    .sources
                    .iter()
                    .enumerate()
                    .find(|(_, source)| source.id == passage.source_id)?;
                let quote = citable_span(passage, &source.excerpt, self.query)?;
                let tokens = self.llm.count_tokens(&json!(quote).to_string()) + 8;
                Some((index, quote, tokens))
            });
        read(offer)
    }
}

/// Offer the item every retrieved passage its share of the chooser's window
/// holds, best first, then the author's own citation.
pub(super) async fn candidates_for(
    llm: &dyn LLMPort,
    pending: &mut Pending,
    collection: &ReferenceCollection<'_>,
    sources: &[LearningSourceDto],
    hashes: &[String],
    share: usize,
) -> Result<()> {
    let query = pending.query();
    let offers = Offers {
        llm,
        sources,
        query: &query,
        known: Mutex::default(),
    };
    let found = collection
        .retrieve_within(&query, |passages| {
            passages
                .iter()
                .map(|passage| {
                    offers.of(passage, |offer| {
                        offer.as_ref().map_or(0, |(_, _, tokens)| *tokens)
                    })
                })
                .sum::<usize>()
                <= share
        })
        .await?;
    for passage in found {
        let Some((source_index, quote)) = offers.of(&passage, |offer| {
            offer
                .as_ref()
                .map(|(index, quote, _)| (*index, quote.clone()))
        }) else {
            continue;
        };
        let Some(hash) = hashes.get(source_index) else {
            continue;
        };
        let candidate = Candidate {
            source_index,
            hash: hash.clone(),
            quote,
        };
        if !pending.candidates.contains(&candidate) {
            pending.candidates.push(candidate);
        }
    }
    if let Some(author) = &pending.author {
        // A heading, link or table row the author quoted is not evidence.
        if author.quote.lines().any(is_prose) && !pending.candidates.contains(author) {
            pending.candidates.push(author.clone());
        }
    }
    Ok(())
}

/// Record one chooser decision on the draft. The chosen passage, if any,
/// becomes the item's citation with a positive receipt; a declined set gets a
/// negative receipt; an author citation that was not chosen is removed.
pub(super) fn apply(
    draft: &mut OutlineDraft,
    model: &str,
    item: &Pending,
    chosen: Option<&Candidate>,
    reason: String,
    log: &mut LinkLog,
) {
    let now = chrono::Utc::now().timestamp_millis();
    match chosen {
        Some(candidate) => {
            if let Some(field) = draft.candidate.pointer_mut(&item.path) {
                cite(field, candidate.source_index, &candidate.quote);
            }
            draft.citation_receipts.insert(
                receipt_key(
                    model,
                    item.kind,
                    &item.text,
                    &candidate.hash,
                    &candidate.quote,
                ),
                CitationReceipt {
                    covers: true,
                    reason,
                    checked_at: now,
                },
            );
            log.linked += 1;
            if item
                .author
                .as_ref()
                .is_some_and(|author| author != candidate)
            {
                log.removed_unchosen += 1;
            }
        }
        None => {
            if let Some(field) = draft.candidate.pointer_mut(&item.path) {
                strip(field);
            }
            draft.citation_receipts.insert(
                declined_key(model, item.kind, &item.text, &item.candidates),
                CitationReceipt {
                    covers: false,
                    reason,
                    checked_at: now,
                },
            );
            if item.author.is_some() {
                log.removed_unchosen += 1;
            }
        }
    }
}
