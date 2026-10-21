//! Searching the library with a plan: the fused first-pass search, evidence
//! expansion around its hits, and fusing a corrective pass into the first.
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::features::conversation::chat::ports::ChatRecords;
use lattice_core::application::contracts::search::Spread;
use lattice_core::shared::error::{AppError, Result};

use super::{
    BranchKind, CorpusSearchPlan, HybridSearchTrait, RankedBranch, SearchRequestDto,
    SearchResponseDto, SearchResultDto,
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
    let mut branches = Vec::new();
    let mut successful_branches = 0;
    let reading_ids: HashSet<_> = plan
        .opening_document_ids
        .iter()
        .filter(|id| scope.document_ids.contains(*id))
        .cloned()
        .collect();
    let search_ids = if plan.start_at_beginning && !reading_ids.is_empty() {
        &reading_ids
    } else {
        &scope.document_ids
    };
    // Independent lexical and vector branches: a query/model failure cannot
    // silently discard successful retrieval from the other branch.
    for query in &plan.queries {
        // The allow-list is the space plus this chat's attachments; see
        // `fused_search`.
        let query_branches = hybrid
            .search_branches(
                query,
                None,
                Some(search_ids),
                limit,
                lattice_core::shared::constants::DEFAULT_VECTOR_FUSION_WEIGHT,
                lattice_core::shared::constants::DEFAULT_KEYWORD_FUSION_WEIGHT,
            )
            .await;
        successful_branches += query_branches.answered;
        branches.extend(query_branches.branches);
    }
    // As many opening passages of each document as the search reads in all:
    // `limit` is sized to the turn's evidence budget, so a large window reads
    // far into the opening documents and a small one only their first pages.
    let openings = repository
        .retrieval_openings(&plan.opening_document_ids, &scope.document_ids, limit)
        .await?;
    let mut results = Vec::new();
    // Round-robin opening chunks, in reading order, prevents one long
    // front-matter document from exhausting the context before the first
    // substantive chapter is read. The prompt's best-first fill takes them in
    // this order until the evidence budget is spent.
    let mut groups: HashMap<String, Vec<_>> = HashMap::new();
    for passage in openings {
        groups
            .entry(passage.document_id.clone())
            .or_default()
            .push(passage);
    }
    let deepest = groups.values().map(Vec::len).max().unwrap_or(0);
    for offset in 0..deepest {
        for id in &plan.opening_document_ids {
            if let Some(p) = groups.get(id).and_then(|passages| passages.get(offset)) {
                results.push(passage_result(p.clone(), "document_opening"));
            }
        }
    }
    // Keep exact references from the user's request even if a planner rewrite
    // omits them. Rewrites can add follow-up references but cannot replace these.
    let identifiers = section_identifiers(&format!("{} {}", question, plan.queries.join(" ")));
    results.extend(
        repository
            .retrieval_section_passages(&identifiers, &scope.document_ids)
            .await?
            .into_iter()
            .map(|p| passage_result(p, "section_lookup")),
    );
    if successful_branches == 0 && results.is_empty() {
        return Err(AppError::ServiceNotAvailable(
            "Document search failed in both vector and keyword retrieval".into(),
        ));
    }
    let mut seen = HashSet::new();
    results.retain(|result| seen.insert(result.id.clone()));
    let opening_ids: HashSet<_> = results.iter().map(|r| r.id.clone()).collect();
    results.extend(
        hybrid
            .fuse(branches, limit, Spread::AcrossDocuments)
            .into_iter()
            .filter(|r| !opening_ids.contains(&r.id)),
    );
    // Opening evidence is bounded separately; it must not be thrown away by
    // lexical overlap gates testing the original conversational instructions.
    results.retain(|r| {
        r.document_id
            .as_ref()
            .is_some_and(|id| scope.document_ids.contains(id))
    });
    let catalog = repository.retrieval_catalog(&scope.document_ids).await?;
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

pub(super) fn passage_result(
    p: lattice_core::application::contracts::search::CorpusPassage,
    method: &str,
) -> SearchResultDto {
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

pub(in super::super) async fn expand_evidence(
    repository: &dyn ChatRecords,
    results: &mut Vec<SearchResultDto>,
    allowed: &HashSet<String>,
    limit: usize,
) -> Result<()> {
    let anchors: Vec<_> = results.iter().take(8).map(|r| r.id.clone()).collect();
    let mut seen: HashSet<_> = results.iter().map(|r| r.id.clone()).collect();
    for anchor in anchors {
        for p in repository.retrieval_neighbors(&anchor, allowed).await? {
            if results.len() >= limit {
                return Ok(());
            }
            if seen.insert(p.id.clone()) {
                results.push(passage_result(p, "section_neighbor"));
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
