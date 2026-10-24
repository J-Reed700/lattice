//! Searching the library with a plan: the fused first-pass search, section
//! context appended after its hits, and fusing a corrective pass into the first.
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::features::conversation::chat::ports::ChatRecords;
use lattice_core::application::contracts::search::{CorpusPassage, Spread};
use lattice_core::shared::error::{AppError, Result};

use super::{
    select_informative_terms, tokenize_keyword_terms, BranchKind, CorpusSearchPlan,
    HybridSearchTrait, RankedBranch, SearchRequestDto, SearchResponseDto, SearchResultDto,
    MAX_QUERY_WORDS,
};

/// One query searched exactly as first-pass retrieval searches it: the
/// library search orchestrator's branches, weights, RRF constant and
/// per-document cap.
///
/// The model's own `semantic_search` calls come through here. They used the
/// generic hybrid endpoint instead — even weights, a higher vector floor and a
/// lexical-support filter — so a follow-up search ranked the same space
/// differently from the search that opened the turn, and could miss passages
/// the first pass would have found.
pub(in crate::features::conversation::chat) async fn fused_search(
    hybrid: &dyn HybridSearchTrait,
    query: &str,
    document_ids: &HashSet<String>,
    limit: usize,
) -> Result<Vec<SearchResultDto>> {
    // The allow-list already is the space plus this chat's attachments. A
    // membership check on top would drop the attachments, which belong to no
    // space, from the keyword branch.
    Ok(hybrid
        .execute_scoped(
            SearchRequestDto::hybrid(query, limit),
            None,
            Some(document_ids),
        )
        .await?
        .results)
}

pub(in super::super) async fn retrieve(
    repository: &dyn ChatRecords,
    hybrid: &dyn HybridSearchTrait,
    question: &str,
    plan: &CorpusSearchPlan,
    scope: &super::super::SpaceDocumentScope,
    limit: usize,
) -> Result<SearchResponseDto> {
    let start = Instant::now();
    let catalog = repository.retrieval_catalog(&scope.document_ids).await?;
    let mut branches = Vec::new();
    let mut successful_branches = 0;
    let reading_ids: HashSet<_> = plan
        .opening_document_ids
        .iter()
        .filter(|id| scope.document_ids.contains(*id))
        .cloned()
        .collect();
    // Independent lexical and vector branches: a query/model failure cannot
    // silently discard successful retrieval from the other branch. Every query
    // searches the whole space, whatever the plan reads in order: these are the
    // only route to a document the planner did not choose.
    for query in &plan.queries {
        // The allow-list is the space plus this chat's attachments; see
        // `fused_search`.
        let query_branches = hybrid
            .search_branches(
                query,
                None,
                Some(&scope.document_ids),
                limit,
                lattice_core::shared::constants::DEFAULT_VECTOR_FUSION_WEIGHT,
                lattice_core::shared::constants::DEFAULT_KEYWORD_FUSION_WEIGHT,
            )
            .await;
        successful_branches += query_branches.answered;
        branches.extend(query_branches.branches);
    }

    // The documents the planner named: one more search inside just those, for
    // what the question asks of them, whether or not the plan also reads them
    // from the start. Their titles already put them on the shortlist, so title
    // words would only rank the passages that repeat the name; the rest of the
    // question and the planner's rewrites are what find the passage that
    // answers it.
    let mut scoped = Vec::new();
    if !reading_ids.is_empty() {
        let chosen_titles: Vec<&str> = catalog
            .iter()
            .filter(|document| reading_ids.contains(&document.id))
            .map(|document| document.name.as_str())
            .collect();
        if let Some(query) = scoped_query(question, &plan.queries, &chosen_titles) {
            let query_branches = hybrid
                .search_branches(
                    &query,
                    None,
                    Some(&reading_ids),
                    limit,
                    lattice_core::shared::constants::DEFAULT_VECTOR_FUSION_WEIGHT,
                    lattice_core::shared::constants::DEFAULT_KEYWORD_FUSION_WEIGHT,
                )
                .await;
            successful_branches += query_branches.answered;
            // Every passage here is already from a chosen document; a
            // per-document cap would only dilute the one that answers.
            scoped = hybrid.fuse(query_branches.branches, limit, Spread::ByRelevance);
        }
    }

    let mut results = Vec::new();
    let opening_ids = &plan.opening_document_ids;
    // Keep exact references from the user's request even if a planner rewrite
    // omits them. Rewrites can add follow-up references but cannot replace these.
    let identifiers = section_identifiers(&format!("{} {}", question, plan.queries.join(" ")));
    // The named documents and the whole space split the pool evenly: neither
    // has a prior claim on what answers the question. The named half holds the
    // exact section references, then the openings, then the scoped search; the
    // whole-space fusion follows it.
    let share = limit.div_ceil(2);
    if !identifiers.is_empty() {
        results.extend(
            repository
                .retrieval_section_passages(&identifiers, &scope.document_ids)
                .await?
                .into_iter()
                .map(|p| passage_result(p, "section_lookup")),
        );
    }
    if !reading_ids.is_empty() && results.len() < share {
        // Reading in order is one source, never the whole pool: a plan that
        // starts at the beginning reads each named document to an even part of
        // the half, any other plan reads each opening only. Round-robin order
        // lets the later evidence budget cut the tail fairly between documents.
        let depth = if plan.start_at_beginning {
            share.div_ceil(reading_ids.len())
        } else {
            1
        };
        let openings = repository
            .retrieval_openings(opening_ids, &scope.document_ids, depth)
            .await?;
        let remaining = share.saturating_sub(results.len());
        append_openings(&mut results, openings, opening_ids, remaining);
    }
    let mut seen_ids = HashSet::new();
    results.retain(|result| seen_ids.insert(result.id.clone()));
    let remaining = share.saturating_sub(results.len());
    results.extend(
        scoped
            .into_iter()
            .filter(|r| seen_ids.insert(r.id.clone()))
            .take(remaining),
    );
    if successful_branches == 0 && results.is_empty() {
        return Err(AppError::ServiceNotAvailable(
            "Document search failed in both vector and keyword retrieval".into(),
        ));
    }
    results.extend(
        hybrid
            .fuse(branches, limit, Spread::AcrossDocuments)
            .into_iter()
            .filter(|r| seen_ids.insert(r.id.clone())),
    );

    // Opening evidence is bounded separately; it must not be thrown away by
    // lexical overlap gates testing the original conversational instructions.
    results.retain(|r| {
        r.document_id
            .as_ref()
            .is_some_and(|id| scope.document_ids.contains(id))
    });
    let locations = repository
        .retrieval_locations(&results.iter().map(|r| r.id.clone()).collect::<Vec<_>>())
        .await?;
    for result in &mut results {
        if let Some((section, page, from_ocr)) = locations.get(&result.id) {
            result
                .metadata
                .insert("section".into(), serde_json::json!(section));
            result
                .metadata
                .insert("pageNumber".into(), serde_json::json!(page));
            if *from_ocr {
                result
                    .metadata
                    .insert("ocr".into(), serde_json::json!(true));
            }
        }
        if let Some(document) = catalog
            .iter()
            .find(|d| Some(&d.id) == result.document_id.as_ref())
        {
            result.title = document.name.clone();
        }
    }
    Ok(SearchResponseDto {
        total: results.len(),
        results,
        query_time_ms: start.elapsed().as_millis() as u64,
    })
}

fn append_openings(
    results: &mut Vec<SearchResultDto>,
    openings: Vec<CorpusPassage>,
    opening_ids: &[String],
    max_to_append: usize,
) {
    if max_to_append == 0 {
        return;
    }
    let mut groups: HashMap<String, Vec<_>> = HashMap::new();
    for passage in openings {
        groups
            .entry(passage.document_id.clone())
            .or_default()
            .push(passage);
    }
    let deepest = groups.values().map(Vec::len).max().unwrap_or(0);
    let initial_len = results.len();
    for offset in 0..deepest {
        for id in opening_ids {
            if results.len().saturating_sub(initial_len) >= max_to_append {
                return;
            }
            if let Some(p) = groups.get(id).and_then(|passages| passages.get(offset)) {
                results.push(passage_result(p.clone(), "document_opening"));
            }
        }
    }
}

pub(super) fn passage_result(p: CorpusPassage, method: &str) -> SearchResultDto {
    let mut metadata = HashMap::from([
        ("retrievalMethod".into(), serde_json::json!(method)),
        ("section".into(), serde_json::json!(p.section)),
        ("pageNumber".into(), serde_json::json!(p.page_number)),
    ]);
    if p.from_ocr {
        metadata.insert("ocr".into(), serde_json::json!(true));
    }
    SearchResultDto {
        id: p.id,
        title: p.name,
        content: p.content,
        score: 0.0,
        path: Some(p.path),
        document_id: Some(p.document_id),
        position: Some(p.chunk_index),
        vector_score: None,
        bm25_score: None,
        vector_rank: None,
        bm25_rank: None,
        metadata,
    }
}

pub(super) fn section_identifiers(question: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    question
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .filter_map(
            lattice_core::domain::value_objects::section_identifier::SectionIdentifier::from_query_token,
        )
        .map(|identifier| identifier.text)
        .filter(|identifier| seen.insert(*identifier))
        .take(4)
        .map(str::to_owned)
        .collect()
}

/// What a focused question asks of the documents the planner chose: the
/// informative terms of the question and then of the planner's queries, in
/// that order, without the words of those documents' titles. The queries
/// carry the paraphrase the user's own words may lack. `None` only when there
/// is nothing to search with at all; when nothing but title words remains,
/// the planner's first query is searched as it is.
fn scoped_query(question: &str, queries: &[String], chosen_titles: &[&str]) -> Option<String> {
    let title_terms: HashSet<String> = chosen_titles
        .iter()
        .flat_map(|title| tokenize_keyword_terms(title))
        .collect();
    let mut seen = HashSet::new();
    let remaining: Vec<String> = std::iter::once(question)
        .chain(queries.iter().map(String::as_str))
        .flat_map(tokenize_keyword_terms)
        .filter(|term| !title_terms.contains(term) && seen.insert(term.clone()))
        .collect();
    let informative: HashSet<String> = select_informative_terms(remaining.clone(), MAX_QUERY_WORDS)
        .into_iter()
        .collect();
    let terms: Vec<String> = remaining
        .into_iter()
        .filter(|term| informative.contains(term))
        .collect();
    if terms.is_empty() {
        return queries.first().cloned();
    }
    Some(terms.join(" "))
}

/// Cut the results to the `limit` hits, then append every hit's unseen section
/// neighbours after all of them as context.
///
/// The hits are the evidence, in rank order, and nothing is placed among them.
/// Context follows hit by hit in rank order, each neighbourhood in the order
/// the section reads. How much of it reaches the prompt is the evidence
/// budget's call: it walks hits before context and keeps what fits, so a
/// neighbour only ever uses room no hit could, and is never fetched in place
/// of one.
pub(in super::super) async fn append_context(
    repository: &dyn ChatRecords,
    results: &mut Vec<SearchResultDto>,
    allowed: &HashSet<String>,
    limit: usize,
) -> Result<()> {
    results.truncate(limit);
    let hits: Vec<_> = results.iter().map(|r| r.id.clone()).collect();
    // A neighbour that is itself a hit keeps its rank; one shared by two hits
    // follows the first.
    let mut present: HashSet<_> = hits.iter().cloned().collect();
    for hit in &hits {
        for passage in repository.retrieval_neighbors(hit, allowed).await? {
            if present.insert(passage.id.clone()) {
                results.push(passage_result(passage, "section_neighbor"));
            }
        }
    }
    Ok(())
}

/// Union two retrieval passes by chunk id and re-rank them by RRF over both.
///
/// Rank, not score, is what fuses: the first pass arrives already reranked on a
/// blended 0-1 scale while the second carries raw RRF weights, and those two
/// scales cannot be compared. Reciprocal rank does not care. A passage both
/// passes found rises above one that only the retry did, which is the whole
/// point of correcting rather than replacing.
pub(in super::super) fn fuse_passes(
    hybrid: &dyn HybridSearchTrait,
    first: Vec<SearchResultDto>,
    second: Vec<SearchResultDto>,
    limit: usize,
) -> Vec<SearchResultDto> {
    let pass = |results| RankedBranch {
        kind: BranchKind::Fused,
        weight: 1.0,
        results,
    };
    hybrid.fuse(
        vec![pass(first), pass(second)],
        limit,
        Spread::AcrossDocuments,
    )
}

#[cfg(test)]
#[path = "library_search_tests.rs"]
mod tests;
