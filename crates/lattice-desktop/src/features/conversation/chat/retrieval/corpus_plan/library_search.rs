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
    let catalog = repository.retrieval_catalog(&scope.document_ids).await?;
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

    let mut scoped_branches = Vec::new();
    if !plan.start_at_beginning && !reading_ids.is_empty() {
        let question_tokens: HashSet<String> = std::iter::once(question)
            .chain(plan.queries.iter().map(String::as_str))
            .flat_map(super::super::tokenize_keyword_terms)
            .collect();
        let mut doc_overlaps: Vec<(String, HashSet<String>, usize)> = Vec::new();
        for doc in &catalog {
            if reading_ids.contains(&doc.id) {
                let tokens: HashSet<String> = super::super::tokenize_keyword_terms(&doc.name)
                    .into_iter()
                    .collect();
                let overlap = tokens
                    .iter()
                    .filter(|t| question_tokens.contains(*t))
                    .count();
                doc_overlaps.push((doc.id.clone(), tokens, overlap));
            }
        }
        let max_overlap = doc_overlaps.iter().map(|(_, _, ov)| *ov).max().unwrap_or(0);
        let mut scoped_reading_ids: HashSet<String> = HashSet::new();
        let mut title_tokens: HashSet<String> = HashSet::new();
        if max_overlap >= 2 {
            for (id, tokens, ov) in doc_overlaps {
                if ov == max_overlap {
                    scoped_reading_ids.insert(id);
                    title_tokens.extend(tokens);
                }
            }
        }

        if !scoped_reading_ids.is_empty() {
            let mut seen_queries = HashSet::new();
            let mut scoped_queries: Vec<String> = Vec::new();
            for raw in std::iter::once(question).chain(plan.queries.iter().map(String::as_str)) {
                let filtered: Vec<&str> = raw
                    .split_whitespace()
                    .filter(|word| {
                        let norm = word
                            .trim_matches(|c: char| !c.is_alphanumeric())
                            .to_lowercase();
                        norm.is_empty() || !title_tokens.contains(&norm)
                    })
                    .collect();
                let cleaned = filtered.join(" ").trim().to_string();
                if !cleaned.is_empty() && seen_queries.insert(cleaned.to_lowercase()) {
                    scoped_queries.push(cleaned);
                }
            }
            if scoped_queries.is_empty() {
                scoped_queries = plan.queries.clone();
            }

            for query in &scoped_queries {
                let query_branches = hybrid
                    .search_branches(
                        query,
                        None,
                        Some(&scoped_reading_ids),
                        limit,
                        lattice_core::shared::constants::DEFAULT_VECTOR_FUSION_WEIGHT,
                        lattice_core::shared::constants::DEFAULT_KEYWORD_FUSION_WEIGHT,
                    )
                    .await;
                successful_branches += query_branches.answered;
                scoped_branches.extend(query_branches.branches);
            }
        }
    }

    let mut results = Vec::new();
    let use_openings = plan.start_at_beginning || super::wants_document_openings(question);
    let opening_ids = &plan.opening_document_ids;
    // Keep exact references from the user's request even if a planner rewrite
    // omits them. Rewrites can add follow-up references but cannot replace these.
    let identifiers = section_identifiers(&format!("{} {}", question, plan.queries.join(" ")));

    if !plan.start_at_beginning {
        if !identifiers.is_empty() {
            results.extend(
                repository
                    .retrieval_section_passages(&identifiers, &scope.document_ids)
                    .await?
                    .into_iter()
                    .map(|p| passage_result(p, "section_lookup")),
            );
        }
        if use_openings && !opening_ids.is_empty() && results.len() < limit {
            let per_doc = 2;
            let openings = repository
                .retrieval_openings(opening_ids, &scope.document_ids, per_doc)
                .await?;
            let remaining = limit.saturating_sub(results.len());
            append_openings(&mut results, openings, opening_ids, remaining);
        }
        if !scoped_branches.is_empty() {
            let mut seen_ids: HashSet<String> = results.iter().map(|r| r.id.clone()).collect();
            for r in hybrid.fuse(scoped_branches, 2, Spread::AcrossDocuments) {
                if seen_ids.insert(r.id.clone()) {
                    results.push(r);
                }
            }
        }
    } else {
        if use_openings && !opening_ids.is_empty() && results.len() < limit {
            let openings = repository
                .retrieval_openings(opening_ids, &scope.document_ids, limit)
                .await?;
            // `limit` is the reading depth for each selected document here,
            // not a cap shared across all of them. Round-robin order lets the
            // later evidence budget cut the tail fairly between documents.
            append_openings(&mut results, openings, opening_ids, usize::MAX);
        }
        if !identifiers.is_empty() {
            results.extend(
                repository
                    .retrieval_section_passages(&identifiers, &scope.document_ids)
                    .await?
                    .into_iter()
                    .map(|p| passage_result(p, "section_lookup")),
            );
        }
    }
    if successful_branches == 0 && results.is_empty() {
        return Err(AppError::ServiceNotAvailable(
            "Document search failed in both vector and keyword retrieval".into(),
        ));
    }
    let mut seen = HashSet::new();
    results.retain(|result| seen.insert(result.id.clone()));
    let fused_main = hybrid.fuse(branches, limit, Spread::AcrossDocuments);
    let existing_ids: HashSet<_> = results.iter().map(|r| r.id.clone()).collect();
    results.extend(
        fused_main
            .into_iter()
            .filter(|r| !existing_ids.contains(&r.id)),
    );
    if let Some(bridge) =
        super::shortlist::find_entity_bridge(&results, &catalog, &scope.document_ids, question)
    {
        let target_ids: Vec<String> = catalog
            .iter()
            .filter(|document| bridge.target_document_ids.contains(&document.id))
            .map(|document| document.id.clone())
            .collect();
        // A source opening that unambiguously names another document is a
        // structural link, so carry the target's opening in reading order.
        // Predicate search below still finds relevant later clauses. When the
        // name maps to several documents, leave selection to ranked search
        // rather than opening every same-entity file.
        if target_ids.len() == 1 {
            for passage in repository
                .retrieval_openings(&target_ids, &scope.document_ids, 2)
                .await?
            {
                let mut opening = passage_result(passage, "entity_bridge");
                tag_entity_bridge(&mut opening, &bridge);
                if let Some(index) = results.iter().position(|result| result.id == opening.id) {
                    // Fusion drops chunk position and enrichment metadata. The
                    // repository copy restores both for deterministic reading
                    // order as well as the bridge relationship.
                    results[index] = opening;
                } else {
                    results.push(opening);
                }
            }
        }
        for result in &mut results {
            if result
                .document_id
                .as_ref()
                .is_some_and(|id| bridge.target_document_ids.contains(id))
            {
                tag_entity_bridge(result, &bridge);
            }
        }
        let predicate_terms: Vec<String> = super::super::tokenize_keyword_terms(question)
            .into_iter()
            .filter(|w| !bridge.source_heading_terms.contains(w))
            .collect();
        let entity_query = if !predicate_terms.is_empty() {
            predicate_terms.join(" ")
        } else {
            question.to_string()
        };
        let entity_branches = hybrid
            .search_branches(
                &entity_query,
                None,
                Some(&bridge.target_document_ids),
                8,
                lattice_core::shared::constants::DEFAULT_VECTOR_FUSION_WEIGHT,
                lattice_core::shared::constants::DEFAULT_KEYWORD_FUSION_WEIGHT,
            )
            .await;
        let mut seen_now: HashSet<_> = results.iter().map(|r| r.id.clone()).collect();
        for mut r in hybrid.fuse(entity_branches.branches, 6, Spread::AcrossDocuments) {
            if seen_now.insert(r.id.clone()) {
                tag_entity_bridge(&mut r, &bridge);
                results.push(r);
            }
        }
    }

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

fn tag_entity_bridge(result: &mut SearchResultDto, bridge: &super::shortlist::EntityBridge) {
    result
        .metadata
        .insert("retrievalMethod".into(), serde_json::json!("entity_bridge"));
    result.metadata.insert(
        "bridgeSourceResultId".into(),
        serde_json::json!(&bridge.source_result_id),
    );
    result.metadata.insert(
        "bridgeSourceDocumentId".into(),
        serde_json::json!(&bridge.source_document_id),
    );
}

fn append_openings(
    results: &mut Vec<SearchResultDto>,
    openings: Vec<lattice_core::application::contracts::search::CorpusPassage>,
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
    let mut top_neighbors = Vec::new();
    for anchor in anchors.iter().take(4) {
        for p in repository.retrieval_neighbors(anchor, allowed).await? {
            if top_neighbors.len() >= 4 || results.len() + top_neighbors.len() >= limit {
                break;
            }
            if seen.insert(p.id.clone()) {
                top_neighbors.push(passage_result(p, "section_neighbor"));
            }
        }
        if top_neighbors.len() >= 4 || results.len() + top_neighbors.len() >= limit {
            break;
        }
    }
    if !top_neighbors.is_empty() {
        let insert_idx = 6.min(results.len());
        results.splice(insert_idx..insert_idx, top_neighbors);
    }
    for anchor in anchors.iter().skip(4) {
        for p in repository.retrieval_neighbors(anchor, allowed).await? {
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
