//! Which catalog documents the planner is shown: the documents a local search
//! ranks for the request, each with its reading-order neighbours, and for a
//! request to start at the beginning, the front matter of their books. The
//! plan is still validated against the whole catalog, so reading order is
//! worked out from every document in a book.

use std::collections::{HashMap, HashSet};

use tracing::warn;

use lattice_core::application::contracts::search::{
    CorpusDocument, ReciprocalRankFusion, WeightedRanking,
};
use lattice_core::shared::constants::{
    DEFAULT_KEYWORD_FUSION_WEIGHT, DEFAULT_RRF_K, DEFAULT_VECTOR_FUSION_WEIGHT,
};

use super::super::{select_informative_terms, tokenize_keyword_terms};
use super::{
    navigation_only, numbered_series_prefix, whole_document_intent, HybridSearchTrait,
    RankedBranch, SearchResultDto,
};

/// Informative terms carried from the conversation into the follow-up search.
const CONVERSATION_TERMS: usize = 12;

/// Documents a local search ranks for this request, best first, up to `rows`
/// a query. The request is searched alone and, when there is conversation,
/// with the conversation's informative terms, the two rankings interleaved; a
/// failed search leaves an empty ranking.
pub(in super::super) async fn candidate_documents(
    hybrid: &dyn HybridSearchTrait,
    question: &str,
    history: Option<&str>,
    document_ids: &HashSet<String>,
    rows: usize,
) -> Vec<String> {
    if rows == 0 {
        return Vec::new();
    }
    let mut queries = vec![question.to_string()];
    if let Some(history) = history.filter(|history| !history.trim().is_empty()) {
        let terms = select_informative_terms(tokenize_keyword_terms(history), CONVERSATION_TERMS);
        if !terms.is_empty() {
            queries.push(format!("{question} {}", terms.join(" ")));
        }
    }
    let mut rankings = Vec::new();
    for query in &queries {
        // The allow-list already is the space plus this chat's attachments.
        let found = hybrid
            .search_branches(
                query,
                None,
                Some(document_ids),
                rows,
                DEFAULT_VECTOR_FUSION_WEIGHT,
                DEFAULT_KEYWORD_FUSION_WEIGHT,
            )
            .await;
        if found.answered == 0 {
            warn!("Planner shortlist search failed; showing the catalog in its own order");
            continue;
        }
        rankings.push(rank_documents(&found.branches, rows));
    }
    interleave(rankings)
}

/// Up to `limit` documents, best first, fused at document granularity: each
/// branch's passages collapse to their documents in the order they first
/// appear, and those rankings fuse by weighted reciprocal rank with the
/// library search's weights and `k`. A document that holds every top passage
/// in both branches heads the list once, and the next documents follow by
/// their own best evidence.
fn rank_documents(branches: &[RankedBranch], limit: usize) -> Vec<String> {
    let rankings = branches
        .iter()
        .map(|branch| WeightedRanking::new(branch.weight, documents_in_rank_order(&branch.results)))
        .collect();
    ReciprocalRankFusion::new(DEFAULT_RRF_K)
        .fuse_ranked(rankings)
        .into_iter()
        .take(limit)
        .map(|fused| fused.id)
        .collect()
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
/// first; the prompt takes rows from the front until its budget runs out. Each
/// hit brings its reading-order neighbours, and a request to start at the
/// beginning the front matter of each hit's book; the rest of the catalog
/// follows in its own order.
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

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::application::contracts::search::BranchKind;

    fn passage(id: &str, document: &str) -> SearchResultDto {
        SearchResultDto {
            id: id.into(),
            title: document.into(),
            content: id.into(),
            score: 1.0,
            path: None,
            document_id: Some(document.into()),
            position: None,
            vector_score: None,
            bm25_score: None,
            vector_rank: None,
            bm25_rank: None,
            metadata: HashMap::new(),
        }
    }

    fn branch(kind: BranchKind, weight: f32, passages: &[(&str, &str)]) -> RankedBranch {
        RankedBranch {
            kind,
            weight,
            results: passages
                .iter()
                .map(|(id, document)| passage(id, document))
                .collect(),
        }
    }

    /// A long document that holds every top passage in both branches is one
    /// row of the shortlist, not all of it.
    #[test]
    fn a_document_dominating_both_branches_is_ranked_once_and_the_next_follow() {
        let branches = vec![
            branch(
                BranchKind::Vector,
                0.7,
                &[
                    ("a0", "a"),
                    ("a1", "a"),
                    ("a2", "a"),
                    ("a3", "a"),
                    ("b0", "b"),
                    ("c0", "c"),
                ],
            ),
            branch(
                BranchKind::Lexical,
                0.3,
                &[
                    ("a4", "a"),
                    ("a5", "a"),
                    ("a6", "a"),
                    ("c1", "c"),
                    ("b1", "b"),
                ],
            ),
        ];
        assert_eq!(rank_documents(&branches, 8), ["a", "b", "c"]);
        assert_eq!(rank_documents(&branches, 2), ["a", "b"]);
    }
}
