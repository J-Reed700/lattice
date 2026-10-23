//! Searching the library with a plan: the fused first-pass search, evidence
//! expansion around its hits, and fusing a corrective pass into the first.
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

    // A focused question about documents the planner named: one more search
    // inside just those documents, for what the question asks of them. Their
    // titles already put them on the shortlist, so title words would only
    // rank the passages that repeat the name; the rest of the question is what
    // finds the passage that answers it.
    let mut scoped = Vec::new();
    if !plan.start_at_beginning && !reading_ids.is_empty() {
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
        if !opening_ids.is_empty() && results.len() < limit {
            // Each chosen document's opening, and only that: reading further
            // into it is ordered reading, which is the other branch.
            let openings = repository
                .retrieval_openings(opening_ids, &scope.document_ids, 1)
                .await?;
            let remaining = limit.saturating_sub(results.len());
            append_openings(&mut results, openings, opening_ids, remaining);
        }
        // The named documents and the whole space split the pool evenly:
        // neither has a prior claim on what answers the question.
        let share = limit.div_ceil(2);
        let mut seen_ids: HashSet<String> = results.iter().map(|r| r.id.clone()).collect();
        results.extend(
            scoped
                .into_iter()
                .filter(|r| seen_ids.insert(r.id.clone()))
                .take(share),
        );
    } else {
        if !opening_ids.is_empty() && results.len() < limit {
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
    let existing_ids: HashSet<_> = results.iter().map(|r| r.id.clone()).collect();
    results.extend(
        hybrid
            .fuse(branches, limit, Spread::AcrossDocuments)
            .into_iter()
            .filter(|r| !existing_ids.contains(&r.id)),
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

/// What a focused question asks of the documents the planner chose: its
/// informative terms, in its own order, without the words of those documents'
/// titles. `None` only when there is nothing to search with at all; a question
/// made of nothing but the titles searches with the planner's first query.
fn scoped_query(question: &str, queries: &[String], chosen_titles: &[&str]) -> Option<String> {
    let title_terms: HashSet<String> = chosen_titles
        .iter()
        .flat_map(|title| tokenize_keyword_terms(title))
        .collect();
    let remaining: Vec<String> = tokenize_keyword_terms(question)
        .into_iter()
        .filter(|term| !title_terms.contains(term))
        .collect();
    let informative: HashSet<String> = select_informative_terms(remaining.clone(), MAX_QUERY_WORDS)
        .into_iter()
        .collect();
    let mut seen = HashSet::new();
    let terms: Vec<String> = remaining
        .into_iter()
        .filter(|term| informative.contains(term) && seen.insert(term.clone()))
        .collect();
    if terms.is_empty() {
        return queries.first().cloned();
    }
    Some(terms.join(" "))
}

/// Read the best anchors' neighbours in with them, inside `limit` passages.
///
/// `limit` is the evidence budget counted in passages, so it alone decides
/// how many anchors get their context: walking in rank order, each anchor's
/// neighbours displace the weakest hits at the tail, and the walk ends at the
/// first anchor with no room left after it.
pub(in super::super) async fn expand_evidence(
    repository: &dyn ChatRecords,
    results: &mut Vec<SearchResultDto>,
    allowed: &HashSet<String>,
    limit: usize,
) -> Result<()> {
    results.truncate(limit);
    let anchors: Vec<_> = results.iter().map(|r| r.id.clone()).collect();
    for anchor in anchors {
        // Anchors keep their order, so none below this one has room either.
        let room = results
            .iter()
            .position(|r| r.id == anchor)
            .is_some_and(|at| at + 1 < limit);
        if !room {
            break;
        }
        let neighbors = repository.retrieval_neighbors(&anchor, allowed).await?;
        place_after_anchor(results, &anchor, neighbors, limit);
    }
    Ok(())
}

/// Insert `anchor`'s neighbours that are not already evidence directly after
/// it, in the order given, cutting the list back to `limit` as they go in.
///
/// A neighbour belongs with its anchor: the prompt's budget fills in this
/// order, so the anchor and its context are kept or cut together.
fn place_after_anchor(
    results: &mut Vec<SearchResultDto>,
    anchor: &str,
    neighbors: Vec<CorpusPassage>,
    limit: usize,
) {
    let Some(mut at) = results.iter().position(|r| r.id == anchor) else {
        return;
    };
    for passage in neighbors {
        if at + 1 >= limit {
            return;
        }
        if results.iter().all(|r| r.id != passage.id) {
            at += 1;
            results.insert(at, passage_result(passage, "section_neighbor"));
            results.truncate(limit);
        }
    }
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
mod tests {
    use super::*;
    use crate::features::conversation::repository::ConversationRepository;

    fn passage(id: &str, section: &str, chunk_index: usize) -> CorpusPassage {
        CorpusPassage {
            id: id.into(),
            document_id: "leitfaden".into(),
            name: "Leitfaden Zellbiologie".into(),
            path: "leitfaden.md".into(),
            content: format!("{id}: Während der Mitose teilt sich der Zellkern."),
            chunk_index,
            section: Some(section.into()),
            page_number: None,
            from_ocr: false,
        }
    }

    fn anchors() -> Vec<SearchResultDto> {
        ["a1", "a2", "a3"]
            .iter()
            .enumerate()
            .map(|(index, id)| {
                passage_result(passage(id, "Leitfaden > Zellteilung", index * 10), "hybrid")
            })
            .collect()
    }

    fn neighbors(anchor: &str) -> Vec<CorpusPassage> {
        let ids: &[&str] = match anchor {
            "a1" => &["n1a", "n1b"],
            "a2" => &["n2a", "a1"],
            "a3" => &["n3a"],
            _ => &[],
        };
        ids.iter()
            .map(|id| passage(id, "Leitfaden > Zellteilung > Mitose", 1))
            .collect()
    }

    /// The anchors in rank order, each placing its neighbours.
    fn expanded(limit: usize) -> Vec<String> {
        let mut results = anchors();
        for anchor in ["a1", "a2", "a3"] {
            place_after_anchor(&mut results, anchor, neighbors(anchor), limit);
        }
        results.into_iter().map(|r| r.id).collect()
    }

    #[test]
    fn neighbours_follow_their_own_anchor_and_push_out_the_tail() {
        assert_eq!(
            expanded(usize::MAX),
            ["a1", "n1a", "n1b", "a2", "n2a", "a3", "n3a"]
        );
        // Exactly `limit`, and the weakest anchors make the room.
        assert_eq!(expanded(5), ["a1", "n1a", "n1b", "a2", "n2a"]);
        assert_eq!(expanded(3), ["a1", "n1a", "n1b"]);
    }

    #[test]
    fn a_neighbour_keeps_its_own_section() {
        let mut results = anchors();
        place_after_anchor(&mut results, "a3", neighbors("a3"), usize::MAX);

        let neighbour = results.last().expect("neighbour");
        assert_eq!(
            neighbour.metadata.get("section"),
            Some(&serde_json::json!("Leitfaden > Zellteilung > Mitose"))
        );
        assert_eq!(
            neighbour.metadata.get("retrievalMethod"),
            Some(&serde_json::json!("section_neighbor"))
        );
    }

    /// Eight consecutive chunks of one section, `m1`..`m8`, and three
    /// chunks with no section, `t1`..`t3`, which have no neighbours.
    async fn mitose() -> ConversationRepository {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .unwrap();
        sqlx::raw_sql(
            "CREATE TABLE documents(id TEXT, file_name TEXT, file_path TEXT);
            CREATE TABLE text_chunks(id TEXT, document_id TEXT, content TEXT, chunk_index INTEGER, section TEXT, page_number INTEGER, from_ocr BOOLEAN NOT NULL DEFAULT 0);
            INSERT INTO documents VALUES ('leitfaden','Leitfaden Zellbiologie','leitfaden.md');
            INSERT INTO text_chunks(id, document_id, content, chunk_index, section) VALUES
            ('m1','leitfaden','Interphase',1,'Zellteilung > Mitose'),
            ('m2','leitfaden','Prophase',2,'Zellteilung > Mitose'),
            ('m3','leitfaden','Prometaphase',3,'Zellteilung > Mitose'),
            ('m4','leitfaden','Metaphase',4,'Zellteilung > Mitose'),
            ('m5','leitfaden','Anaphase',5,'Zellteilung > Mitose'),
            ('m6','leitfaden','Telophase',6,'Zellteilung > Mitose'),
            ('m7','leitfaden','Zytokinese',7,'Zellteilung > Mitose'),
            ('m8','leitfaden','Tochterzellen',8,'Zellteilung > Mitose'),
            ('t1','leitfaden','Glossar A',101,NULL),
            ('t2','leitfaden','Glossar B',102,NULL),
            ('t3','leitfaden','Glossar C',103,NULL);",
        )
        .execute(&pool)
        .await
        .unwrap();
        ConversationRepository::new(pool)
    }

    /// `ranked` as `expand_evidence` leaves it for `limit` passages.
    async fn expand(
        repository: &ConversationRepository,
        ranked: &[&str],
        limit: usize,
    ) -> Vec<String> {
        let mut results: Vec<_> = ranked
            .iter()
            .map(|id| passage_result(passage(id, "Zellteilung > Mitose", 0), "hybrid"))
            .collect();
        let allowed = HashSet::from(["leitfaden".to_string()]);
        expand_evidence(repository, &mut results, &allowed, limit)
            .await
            .expect("neighbours");
        results.into_iter().map(|r| r.id).collect()
    }

    #[tokio::test]
    async fn neighbours_of_the_best_anchor_displace_the_weakest_hits() {
        let repository = mitose().await;
        // m2's context takes two slots and m6's the last one; the tail makes room.
        assert_eq!(
            expand(&repository, &["m2", "m6", "t1", "t2", "t3"], 5).await,
            ["m2", "m1", "m3", "m6", "m5"]
        );
    }

    #[tokio::test]
    async fn no_anchor_without_room_after_it_reads_its_neighbours() {
        let repository = mitose().await;
        // m2's context pushes m6 past the limit, so m6 brings none.
        assert_eq!(
            expand(&repository, &["m2", "m6", "t1"], 3).await,
            ["m2", "m1", "m3"]
        );
        // m6 holds the last slot, with nowhere to put its context.
        assert_eq!(
            expand(&repository, &["t1", "t2", "m6"], 3).await,
            ["t1", "t2", "m6"]
        );
    }

    #[tokio::test]
    async fn a_neighbour_already_in_the_evidence_is_not_repeated() {
        let repository = mitose().await;
        // m3 is m2's neighbour and a hit of its own: it keeps its place, once.
        assert_eq!(
            expand(&repository, &["m2", "m3", "t1"], 10).await,
            ["m2", "m1", "m3", "m4", "t1"]
        );
        // A hit pushed past the limit is no longer evidence, so it can come
        // back as a better anchor's context.
        assert_eq!(
            expand(&repository, &["m2", "m5", "t1", "m6"], 6).await,
            ["m2", "m1", "m3", "m5", "m4", "m6"]
        );
    }

    #[tokio::test]
    async fn the_evidence_never_exceeds_the_limit() {
        let repository = mitose().await;
        let ranked = ["m2", "m6", "t1", "m4", "t2", "m8", "t3"];
        // From nothing to more room than the fixture has passages.
        for limit in 0..=12 {
            let evidence = expand(&repository, &ranked, limit).await;
            assert!(evidence.len() <= limit, "{limit}: {evidence:?}");
            let distinct: HashSet<_> = evidence.iter().collect();
            assert_eq!(distinct.len(), evidence.len(), "{limit}: {evidence:?}");
        }
    }

    fn query(question: &str, title: &str) -> String {
        scoped_query(question, &["planner query".to_string()], &[title]).expect("query")
    }

    #[test]
    fn a_scoped_query_drops_the_chosen_documents_title_words_in_any_language() {
        let contract = query(
            "Under the Tessivar Cloud framework agreement, what notice period applies to termination?",
            "Tessivar Cloud framework agreement",
        );
        for title_word in ["tessivar", "cloud", "framework", "agreement"] {
            assert!(!contract.contains(title_word), "{contract}");
        }
        assert!(contract.contains("termination"), "{contract}");

        let handbook = query(
            "Im Handbuch Plattformbetrieb: welche Referenz gehört auf ein Ticket?",
            "Handbuch Plattformbetrieb",
        );
        assert!(!handbook.contains("handbuch"), "{handbook}");
        assert!(!handbook.contains("plattformbetrieb"), "{handbook}");
        assert!(handbook.contains("referenz"), "{handbook}");
        assert!(handbook.contains("ticket"), "{handbook}");

        let music = query(
            "In the Mozart Requiem programme notes, what key is the Lacrimosa in?",
            "Mozart Requiem programme notes",
        );
        assert!(music.contains("lacrimosa"), "{music}");
        assert!(
            music
                .split(' ')
                .all(|term| ["lacrimosa", "key"].contains(&term)),
            "{music}"
        );
    }

    #[test]
    fn a_question_made_only_of_the_title_searches_with_the_planners_query() {
        assert_eq!(
            query(
                "Mozart Requiem programme notes?",
                "Mozart Requiem programme notes"
            ),
            "planner query"
        );
        assert_eq!(
            scoped_query("Leitfaden Zellbiologie", &[], &["Leitfaden Zellbiologie"]),
            None
        );
    }
}
