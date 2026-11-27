use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};

use super::{
    Gathered, LLMPort, LearningSourceDto, ReferenceCollection, ReferencePassage, Result, Topic,
};
use crate::reference_collection::{item_tokens, PASSAGE_CHARS};

/// How many passages each search retrieves: the evidence room shared among
/// every search, in passages of the size retrieval returns. A passage's
/// tokens are measured on the saved text with this model's tokenizer; when
/// the sources hold less text than one passage, the sample's tokens per
/// character are scaled up to a passage.
pub(super) fn depth(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'_>,
    topics: &[Topic],
    room: usize,
) -> usize {
    let searches: usize = topics.iter().map(|topic| topic.queries.len()).sum();
    (room / (searches.max(1) * passage_tokens(llm, references))).max(1)
}

pub(super) fn passage_tokens(llm: &dyn LLMPort, references: &ReferenceCollection<'_>) -> usize {
    let sample: String = references
        .sources
        .iter()
        .flat_map(|source| source.excerpt.chars())
        .take(PASSAGE_CHARS)
        .collect();
    let characters = sample.chars().count();
    if characters == 0 {
        return 1;
    }
    (llm.count_tokens(&sample) * PASSAGE_CHARS)
        .div_ceil(characters)
        .max(1)
}

/// The index of `passage` in `store`, adding it unless a passage of the same
/// source already spans any of its bytes.
fn intern(store: &mut Vec<ReferencePassage>, passage: ReferencePassage) -> usize {
    if let Some(index) = store.iter().position(|known| known.overlaps(&passage)) {
        return index;
    }
    store.push(passage);
    store.len() - 1
}

/// Run every search of `topic` against all saved references and merge the
/// results by rank, each passage once.
pub(super) async fn retrieve(
    references: &ReferenceCollection<'_>,
    topic: &Topic,
    depth: usize,
    store: &mut Vec<ReferencePassage>,
) -> Result<Vec<usize>> {
    let mut ranked = Vec::new();
    for query in &topic.queries {
        ranked.extend(
            references
                .retrieve(query, depth)
                .await?
                .into_iter()
                .enumerate(),
        );
    }
    // Stable: equal ranks keep the plan's search order.
    ranked.sort_by_key(|(rank, _)| *rank);
    let mut found = Vec::new();
    for (_, passage) in ranked {
        let index = intern(store, passage);
        if !found.contains(&index) {
            found.push(index);
        }
    }
    Ok(found)
}

/// Fill `room` with passages round-robin across topics, so every topic is
/// represented before any topic gets another passage. Each topic offers the
/// passages judged to cover it first, then its other passages, best first.
/// Returns the passages in the order they were added.
pub(in crate::lessons::generation) fn pack(
    llm: &dyn LLMPort,
    gathered: &Gathered<'_>,
    references: &ReferenceCollection<'_>,
    room: usize,
) -> Vec<usize> {
    let offers: Vec<Vec<usize>> = gathered
        .candidates
        .iter()
        .zip(&gathered.covering)
        .map(|(candidates, covering)| {
            let mut offer: Vec<usize> = candidates
                .iter()
                .filter(|passage| covering.contains(passage))
                .copied()
                .collect();
            for passage in covering.iter().chain(candidates) {
                if !offer.contains(passage) {
                    offer.push(*passage);
                }
            }
            offer
        })
        .collect();
    let mut cursors = vec![0; offers.len()];
    let mut added = Vec::new();
    let mut taken = HashSet::new();
    let mut sources = HashSet::new();
    let mut remaining = room;
    loop {
        let mut offered = false;
        for (offer, cursor) in offers.iter().zip(cursors.iter_mut()) {
            // A topic's turn takes its next passage not already packed.
            while offer.get(*cursor).is_some_and(|p| taken.contains(p)) {
                *cursor += 1;
            }
            let Some(&index) = offer.get(*cursor) else {
                continue;
            };
            *cursor += 1;
            offered = true;
            let Some(passage) = gathered.passages.get(index) else {
                continue;
            };
            let mut cost = item_tokens(llm, &json!(passage.text));
            let new_source = !sources.contains(passage.source_id.as_str());
            if new_source {
                let source = references
                    .sources
                    .iter()
                    .find(|source| source.id == passage.source_id);
                cost += item_tokens(
                    llm,
                    &json!({"sourceIndex":added.len(),"title":source.map(|s| s.title.as_str()),"url":source.and_then(|s| s.url.as_deref()),"excerpt":""}),
                );
            }
            if cost > remaining {
                continue;
            }
            remaining -= cost;
            taken.insert(index);
            if new_source {
                sources.insert(passage.source_id.as_str());
            }
            added.push(index);
        }
        if !offered {
            break;
        }
    }
    added
}

/// The writer's sources and its map of topics to them. Each source's excerpt
/// is its packed passages in document order; each topic names the sources
/// whose packed passages were judged to cover it.
pub(in crate::lessons::generation) fn assemble(
    gathered: &Gathered<'_>,
    topics: &[Topic],
    references: &ReferenceCollection<'_>,
    added: &[usize],
) -> (Vec<LearningSourceDto>, Vec<Value>) {
    let packed: HashSet<usize> = added.iter().copied().collect();
    let passage = |index: &usize| gathered.passages.get(*index);
    let mut order: Vec<&str> = Vec::new();
    for found in added.iter().filter_map(passage) {
        if !order.contains(&found.source_id.as_str()) {
            order.push(found.source_id.as_str());
        }
    }
    let mut sources = Vec::new();
    let mut index_of: HashMap<&str, usize> = HashMap::new();
    for source_id in order {
        let Some(source) = references
            .sources
            .iter()
            .find(|source| source.id == source_id)
        else {
            continue;
        };
        let mut passages: Vec<&ReferencePassage> = added
            .iter()
            .filter_map(passage)
            .filter(|passage| passage.source_id == source_id)
            .collect();
        passages.sort_by_key(|passage| passage.start_byte);
        let mut source = source.clone();
        source.excerpt = passages
            .iter()
            .map(|passage| passage.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        index_of.insert(source_id, sources.len());
        sources.push(source);
    }
    let map = topics
        .iter()
        .zip(&gathered.covering)
        .map(|(topic, covering)| {
            let mut indexes: Vec<usize> = covering
                .iter()
                .filter(|index| packed.contains(index))
                .filter_map(passage)
                .filter_map(|found| index_of.get(found.source_id.as_str()).copied())
                .collect();
            indexes.sort_unstable();
            indexes.dedup();
            json!({"topic":topic.topic,"coveringSourceIndexes":indexes})
        })
        .collect();
    (sources, map)
}
