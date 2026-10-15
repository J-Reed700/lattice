//! Which catalog documents the planner is shown.
//!
//! The planner used to be shown the whole catalog in filename order, cut off
//! wherever the window ran out. On a 379-document vault that was the first
//! eighty-odd files alphabetically, whatever the question, at 15k tokens that
//! a 9B could not prefill in time. It is now shown the documents a local search
//! ranks for the request, each with its reading-order neighbours, and for a
//! request to learn or start at the beginning, the front matter of the books
//! those documents belong to. The plan is still validated against the whole
//! catalog, so reading order is worked out from every document in the book,
//! not just the ones the planner saw.

use std::collections::{HashMap, HashSet};

use tracing::warn;

use lattice_core::application::contracts::search::CorpusDocument;

use super::super::{select_informative_terms, tokenize_keyword_terms};
use super::{
    fused_search, navigation_only, numbered_series_prefix, whole_document_intent,
    LibrarySearchTrait, SearchResultDto,
};

/// Passages the candidate search reads. Fusion keeps at most four passages a
/// document, so this reaches at least ten documents and usually twenty or more.
const CANDIDATE_SEARCH_PASSAGES: usize = 40;

/// Informative terms carried from the conversation into the follow-up search.
const CONVERSATION_TERMS: usize = 12;

/// Documents a local search ranks for this request, best first.
///
/// The request alone, and, when there is conversation, the request with the
/// conversation's informative terms: "is there any way round it?" names
/// nothing a search can find, and the turn before it does. The two rankings
/// are interleaved so neither crowds the other out. A failed search leaves an
/// empty ranking, and the planner then sees the catalog in its own order.
pub(in super::super) async fn candidate_documents(
    hybrid: &dyn LibrarySearchTrait,
    question: &str,
    history: Option<&str>,
    document_ids: &HashSet<String>,
) -> Vec<String> {
    let mut queries = vec![question.to_string()];
    if let Some(history) = history.filter(|history| !history.trim().is_empty()) {
        let terms = select_informative_terms(tokenize_keyword_terms(history), CONVERSATION_TERMS);
        if !terms.is_empty() {
            queries.push(format!("{question} {}", terms.join(" ")));
        }
    }
    let mut rankings = Vec::new();
    for query in &queries {
        match fused_search(hybrid, query, document_ids, CANDIDATE_SEARCH_PASSAGES).await {
            Ok(results) => rankings.push(documents_in_rank_order(&results)),
            Err(error) => {
                warn!(%error, "Planner shortlist search failed; showing the catalog in its own order")
            }
        }
    }
    interleave(rankings)
}

fn documents_in_rank_order(results: &[SearchResultDto]) -> Vec<String> {
    let mut seen = HashSet::new();
    results
        .iter()
        .filter_map(|result| result.document_id.clone())
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

fn interleave(rankings: Vec<Vec<String>>) -> Vec<String> {
    let longest = rankings.iter().map(Vec::len).max().unwrap_or(0);
    let mut seen = HashSet::new();
    let mut merged = Vec::new();
    for rank in 0..longest {
        for ranking in &rankings {
            if let Some(id) = ranking.get(rank) {
                if seen.insert(id.as_str()) {
                    merged.push(id.clone());
                }
            }
        }
    }
    merged
}

/// What a document is read in sequence with: its book when it has one,
/// otherwise a filename series such as "Manual 01", "Manual 02".
#[derive(Clone, PartialEq, Eq, Hash)]
enum Structure {
    Group(String),
    Series(String),
}

fn structure_of(document: &CorpusDocument) -> Option<Structure> {
    match &document.source_context {
        Some(context) => Some(Structure::Group(context.group.id.clone())),
        None => numbered_series_prefix(&document.name).map(Structure::Series),
    }
}

fn is_introduction(document: &CorpusDocument) -> bool {
    document.name.to_lowercase().contains("introduction")
        || document
            .opening
            .trim()
            .to_lowercase()
            .starts_with("introduction")
}

/// The documents a request to start at the beginning would open with: the
/// introduction, the first chapter, and the first document that is more than
/// a contents page or change log. `members` is in reading order.
fn front_matter<'a>(members: &[&'a CorpusDocument]) -> Vec<&'a CorpusDocument> {
    let introduction = members.iter().find(|d| is_introduction(d)).copied();
    let first_chapter = members
        .iter()
        .filter(|d| d.chapter_number.is_some())
        .min_by_key(|d| d.chapter_number)
        .copied();
    let first_substantive = members.iter().find(|d| !navigation_only(d)).copied();
    [introduction, first_chapter, first_substantive]
        .into_iter()
        .flatten()
        .collect()
}

/// Catalog documents in the order the planner should be offered them, best
/// first. The prompt takes rows from the front until its token budget runs
/// out; the budget is what the planner can read in time (see `health`), so a
/// fast planner is shown more of the catalog than a slow one.
///
/// Each search hit brings its reading-order neighbours, the documents either
/// side of it in its book or series, since a passage often continues into the
/// next file. A request to learn or start at the beginning also brings the
/// front matter of each book a hit belongs to. The rest of the catalog follows
/// in its own order, so any document the budget has room for can be opened.
/// With no ranking at all the catalog keeps its own order, which is how the
/// planner always saw it.
pub(super) fn shortlist<'a>(
    catalog: &'a [CorpusDocument],
    candidates: &[String],
    question: &str,
) -> Vec<&'a CorpusDocument> {
    if candidates.is_empty() {
        return catalog.iter().collect();
    }
    let by_id: HashMap<&str, &CorpusDocument> = catalog
        .iter()
        .map(|document| (document.id.as_str(), document))
        .collect();
    // The catalog is already in reading order: grouped by book, by explicit
    // position within an ordered book, and by name otherwise.
    let mut structures: HashMap<Structure, Vec<&CorpusDocument>> = HashMap::new();
    for document in catalog {
        if let Some(structure) = structure_of(document) {
            structures.entry(structure).or_default().push(document);
        }
    }
    let learning = whole_document_intent(question);
    let mut seen = HashSet::new();
    let mut ordered = Vec::new();
    let mut push = |document: &'a CorpusDocument, ordered: &mut Vec<&'a CorpusDocument>| {
        if seen.insert(document.id.as_str()) {
            ordered.push(document);
        }
    };
    let mut introduced = HashSet::new();
    for id in candidates {
        let Some(document) = by_id.get(id.as_str()).copied() else {
            continue;
        };
        push(document, &mut ordered);
        let Some(structure) = structure_of(document) else {
            continue;
        };
        let Some(members) = structures.get(&structure) else {
            continue;
        };
        if learning && introduced.insert(structure.clone()) {
            for document in front_matter(members) {
                push(document, &mut ordered);
            }
        }
        if let Some(index) = members.iter().position(|member| member.id == document.id) {
            let before = index.checked_sub(1).and_then(|before| members.get(before));
            for neighbour in [before, members.get(index + 1)].into_iter().flatten() {
                push(neighbour, &mut ordered);
            }
        }
    }
    for document in catalog {
        push(document, &mut ordered);
    }
    ordered
}
