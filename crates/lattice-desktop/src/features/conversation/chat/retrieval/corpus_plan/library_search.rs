//! Searching the library with a plan: the fused first-pass search, section
//! context appended after its hits, and fusing a corrective pass into the first.
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::features::conversation::chat::ports::ChatRecords;
use lattice_core::application::contracts::search::CorpusPassage;
use lattice_core::shared::error::{AppError, Result};

use super::{
    select_informative_terms, tokenize_keyword_terms, BranchKind, CorpusSearchPlan,
    HybridSearchTrait, RankedBranch, SearchRequestDto, SearchResponseDto, SearchResultDto,
    MAX_QUERY_WORDS,
};

/// One query searched exactly as first-pass retrieval searches it: the
/// library search orchestrator's branches, weights and RRF constant.
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
            scoped = hybrid.fuse(query_branches.branches, limit);
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
            .fuse(branches, limit)
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
    hybrid.fuse(vec![pass(first), pass(second)], limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::conversation::repository::ConversationRepository;
    use lattice_core::application::services::context_assembler::EvidenceBudget;

    /// One chunk of a fixture library: id, document, chunk index, section, text.
    type Chunk = (
        &'static str,
        &'static str,
        i64,
        Option<&'static str>,
        &'static str,
    );

    /// A cell-biology handbook: eight consecutive chunks of one section,
    /// `m1`..`m8`, and three glossary chunks with no section, `t1`..`t3`,
    /// which have no neighbours.
    const MITOSE: &[Chunk] = &[
        (
            "m1",
            "leitfaden",
            1,
            Some("Zellteilung > Mitose"),
            "Interphase",
        ),
        (
            "m2",
            "leitfaden",
            2,
            Some("Zellteilung > Mitose"),
            "Prophase",
        ),
        (
            "m3",
            "leitfaden",
            3,
            Some("Zellteilung > Mitose"),
            "Prometaphase",
        ),
        (
            "m4",
            "leitfaden",
            4,
            Some("Zellteilung > Mitose"),
            "Metaphase",
        ),
        (
            "m5",
            "leitfaden",
            5,
            Some("Zellteilung > Mitose"),
            "Anaphase",
        ),
        (
            "m6",
            "leitfaden",
            6,
            Some("Zellteilung > Mitose"),
            "Telophase",
        ),
        (
            "m7",
            "leitfaden",
            7,
            Some("Zellteilung > Mitose"),
            "Zytokinese",
        ),
        (
            "m8",
            "leitfaden",
            8,
            Some("Zellteilung > Mitose"),
            "Tochterzellen",
        ),
        ("t1", "leitfaden", 101, None, "Glossar A"),
        ("t2", "leitfaden", 102, None, "Glossar B"),
        ("t3", "leitfaden", 103, None, "Glossar C"),
    ];

    /// A parish chronicle: 1911 in six chunks, its harvest subsection in two
    /// more, then the first chunk of 1912.
    const CRONICA: &[Chunk] = &[
        (
            "c1",
            "cronica",
            1,
            Some("Crónica parroquial > 1911"),
            "Enero: se bendice la campana nueva.",
        ),
        (
            "c2",
            "cronica",
            2,
            Some("Crónica parroquial > 1911"),
            "Febrero: el párroco visita las aldeas.",
        ),
        (
            "c3",
            "cronica",
            3,
            Some("Crónica parroquial > 1911"),
            "Marzo: misiones de Cuaresma.",
        ),
        (
            "c4",
            "cronica",
            4,
            Some("Crónica parroquial > 1911"),
            "Abril: procesión bajo la lluvia.",
        ),
        (
            "c5",
            "cronica",
            5,
            Some("Crónica parroquial > 1911"),
            "Mayo: se reparan las tejas del pórtico.",
        ),
        (
            "c6",
            "cronica",
            6,
            Some("Crónica parroquial > 1911"),
            "Junio: las cuentas de la cosecha, como se detalla a continuación.",
        ),
        (
            "c7",
            "cronica",
            7,
            Some("Crónica parroquial > 1911 > Cosecha"),
            "La vendimia empieza tarde por las lluvias.",
        ),
        (
            "c8",
            "cronica",
            8,
            Some("Crónica parroquial > 1911 > Cosecha"),
            "Se recogen doscientas cargas de uva.",
        ),
        (
            "c9",
            "cronica",
            9,
            Some("Crónica parroquial > 1912"),
            "Enero de 1912: gran nevada.",
        ),
    ];

    async fn library(chunks: &[Chunk]) -> ConversationRepository {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .unwrap();
        sqlx::raw_sql(
            "CREATE TABLE documents(id TEXT, file_name TEXT, file_path TEXT);
            CREATE TABLE text_chunks(id TEXT, document_id TEXT, content TEXT, chunk_index INTEGER, section TEXT, page_number INTEGER, from_ocr BOOLEAN NOT NULL DEFAULT 0);",
        )
        .execute(&pool)
        .await
        .unwrap();
        let documents: HashSet<_> = chunks.iter().map(|chunk| chunk.1).collect();
        for document in documents {
            sqlx::query("INSERT INTO documents VALUES (?, ?, ?)")
                .bind(document)
                .bind(document)
                .bind(format!("{document}.md"))
                .execute(&pool)
                .await
                .unwrap();
        }
        for &(id, document, chunk_index, section, content) in chunks {
            sqlx::query(
                "INSERT INTO text_chunks(id, document_id, content, chunk_index, section) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(document)
            .bind(content)
            .bind(chunk_index)
            .bind(section)
            .execute(&pool)
            .await
            .unwrap();
        }
        ConversationRepository::new(pool)
    }

    /// `hits` in the order search ranked them, as `append_context` leaves them
    /// for `limit` hits.
    async fn appended(
        repository: &ConversationRepository,
        chunks: &[Chunk],
        hits: &[&str],
        limit: usize,
    ) -> Vec<SearchResultDto> {
        let mut results: Vec<_> = hits
            .iter()
            .map(|hit| {
                let &(id, document, chunk_index, section, content) = chunks
                    .iter()
                    .find(|chunk| chunk.0 == *hit)
                    .expect("fixture chunk");
                let passage = CorpusPassage {
                    id: id.into(),
                    document_id: document.into(),
                    name: document.into(),
                    path: format!("{document}.md"),
                    content: content.into(),
                    chunk_index: chunk_index as usize,
                    section: section.map(Into::into),
                    page_number: None,
                    from_ocr: false,
                };
                passage_result(passage, "hybrid")
            })
            .collect();
        let allowed = chunks.iter().map(|chunk| chunk.1.to_string()).collect();
        append_context(repository, &mut results, &allowed, limit)
            .await
            .expect("context");
        results
    }

    fn ids(results: &[SearchResultDto]) -> Vec<&str> {
        results.iter().map(|r| r.id.as_str()).collect()
    }

    #[tokio::test]
    async fn context_follows_every_hit_in_rank_order() {
        let repository = library(MITOSE).await;
        let results = appended(&repository, MITOSE, &["m2", "m6", "t1"], 10).await;
        // The hits as ranked, then m2's neighbourhood, then m6's.
        assert_eq!(ids(&results), ["m2", "m6", "t1", "m1", "m3", "m5", "m7"]);
    }

    #[tokio::test]
    async fn a_hit_that_is_also_a_neighbour_keeps_its_rank_once() {
        let repository = library(MITOSE).await;
        let results = appended(&repository, MITOSE, &["m3", "m2"], 10).await;
        assert_eq!(ids(&results), ["m3", "m2", "m4", "m1"]);
    }

    #[tokio::test]
    async fn hits_past_the_limit_are_cut_before_context_is_read() {
        let repository = library(MITOSE).await;
        // m8 is not evidence, so its neighbourhood is not read either.
        let results = appended(&repository, MITOSE, &["m1", "m5", "m8"], 2).await;
        assert_eq!(ids(&results), ["m1", "m5", "m2", "m4", "m6"]);
    }

    #[tokio::test]
    async fn a_subsection_is_its_sections_context() {
        let repository = library(CRONICA).await;
        let results = appended(&repository, CRONICA, &["c6"], 1).await;
        assert_eq!(ids(&results), ["c6", "c5", "c7"]);
        // The neighbour keeps its own heading, so the prompt names the
        // subsection it comes from.
        assert_eq!(
            results[2].metadata.get("section"),
            Some(&serde_json::json!("Crónica parroquial > 1911 > Cosecha"))
        );
        assert_eq!(
            results[2].metadata.get("retrievalMethod"),
            Some(&serde_json::json!("section_neighbor"))
        );
        // The next year is another section, not the harvest's continuation.
        let results = appended(&repository, CRONICA, &["c8"], 1).await;
        assert_eq!(ids(&results), ["c8", "c7"]);
    }

    #[tokio::test]
    async fn context_never_takes_a_hits_room() {
        let repository = library(MITOSE).await;
        let results = appended(&repository, MITOSE, &["m2", "m6", "t1"], 10).await;
        let chars = |r: &SearchResultDto| r.content.chars().count();
        let carried = |budget: usize| -> Vec<String> {
            EvidenceBudget::new(budget)
                .select(&results, chars)
                .into_iter()
                .map(|r| r.id.clone())
                .collect()
        };
        let hits: usize = results[..3].iter().map(chars).sum();

        assert_eq!(carried(hits), ["m2", "m6", "t1"]);
        assert_eq!(carried(hits + chars(&results[3])), ["m2", "m6", "t1", "m1"]);
        // t1 does not fit what m2 and m6 leave. That room goes to context
        // short enough for it, behind the hits, as it would to a shorter hit.
        assert_eq!(carried(hits - 1), ["m2", "m6", "m5"]);
        // At every budget the hits carried are the ones the hits alone carry.
        for budget in 0..=results.iter().map(chars).sum::<usize>() {
            let alone: Vec<_> = EvidenceBudget::new(budget)
                .select(&results[..3], chars)
                .into_iter()
                .map(|r| r.id.clone())
                .collect();
            assert_eq!(carried(budget)[..alone.len()], alone[..], "{budget}");
        }
    }

    #[tokio::test]
    async fn a_chunk_without_a_section_has_no_context() {
        let repository = library(MITOSE).await;
        let results = appended(&repository, MITOSE, &["t2"], 10).await;
        assert_eq!(ids(&results), ["t2"]);
    }

    /// A scoped query whose planner query only repeats the title, so what
    /// remains is the question's own.
    fn query(question: &str, title: &str) -> String {
        scoped_query(question, &[title.to_string()], &[title]).expect("query")
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
            scoped_query(
                "Mozart Requiem programme notes?",
                &["Mozart Requiem".to_string()],
                &["Mozart Requiem programme notes"]
            ),
            Some("Mozart Requiem".to_string())
        );
        assert_eq!(
            scoped_query("Leitfaden Zellbiologie", &[], &["Leitfaden Zellbiologie"]),
            None
        );
    }

    /// The terms of the scoped query for `question`, a planner `rewrite` and
    /// the chosen document's `title`.
    fn scoped_terms(question: &str, rewrite: &str, title: &str) -> Vec<String> {
        query_terms(&scoped_query(question, &[rewrite.to_string()], &[title]).expect("query"))
    }

    fn query_terms(query: &str) -> Vec<String> {
        query.split(' ').map(str::to_owned).collect()
    }

    #[test]
    fn a_scoped_query_adds_the_planners_rewrite_without_the_title() {
        let transcript = scoped_terms(
            "In the release call transcript, what do we roll back to if the deploy fails?",
            "release rollback strategy previous build",
            "Release call transcript",
        );
        assert!(
            transcript.contains(&"rollback".to_string()),
            "{transcript:?}"
        );
        for title_word in ["release", "call", "transcript"] {
            assert!(
                !transcript.contains(&title_word.to_string()),
                "{transcript:?}"
            );
        }

        let minutes = scoped_terms(
            "Im Protokoll der Quartalsbesprechung: worauf setzen wir zurück?",
            "Rollback-Ziel Version",
            "Protokoll der Quartalsbesprechung",
        );
        assert!(minutes.contains(&"rollback".to_string()), "{minutes:?}");
        for title_word in ["protokoll", "quartalsbesprechung"] {
            assert!(!minutes.contains(&title_word.to_string()), "{minutes:?}");
        }

        let music = scoped_terms(
            "In the Mozart Requiem programme notes, what key is the Lacrimosa in?",
            "Lacrimosa key signature D minor",
            "Mozart Requiem programme notes",
        );
        for term in ["lacrimosa", "signature", "minor"] {
            assert!(music.contains(&term.to_string()), "{music:?}");
        }
        for title_word in ["requiem", "notes"] {
            assert!(!music.contains(&title_word.to_string()), "{music:?}");
        }
    }

    /// Chunks `0..count` of each document, in reading order.
    fn chunks_of(documents: &[&str], text: &str, count: usize) -> Vec<CorpusPassage> {
        documents
            .iter()
            .flat_map(|document| {
                (0..count).map(move |index| CorpusPassage {
                    id: format!("{document}c{index}"),
                    document_id: (*document).into(),
                    name: format!("{document}.md"),
                    path: format!("/vault/{document}.md"),
                    content: format!("{text} {}", index + 1),
                    chunk_index: index,
                    section: None,
                    page_number: None,
                    from_ocr: false,
                })
            })
            .collect()
    }

    #[test]
    fn openings_stop_at_their_share_and_take_turns_between_documents() {
        for (documents, text) in [
            (["vertrag", "anhang", "nachtrag"], "Kapitel"),
            (
                ["mitosis", "meiosis", "apoptosis"],
                "Capítulo: el ciclo celular",
            ),
        ] {
            let ids = documents.map(String::from).to_vec();
            let mut results = Vec::new();
            append_openings(&mut results, chunks_of(&documents, text, 10), &ids, 5);

            let [first, second, third] = documents;
            assert_eq!(
                results.into_iter().map(|r| r.id).collect::<Vec<_>>(),
                [
                    format!("{first}c0"),
                    format!("{second}c0"),
                    format!("{third}c0"),
                    format!("{first}c1"),
                    format!("{second}c1"),
                ]
            );
        }
    }

    /// Finds `hits` for every query and records the documents each search
    /// was confined to.
    struct RecordingLibrary {
        hits: Vec<SearchResultDto>,
        searched: std::sync::Mutex<Vec<HashSet<String>>>,
    }

    #[async_trait::async_trait]
    impl HybridSearchTrait for RecordingLibrary {
        async fn execute_scoped(
            &self,
            _request: SearchRequestDto,
            _space_id: Option<&str>,
            _allowed_document_ids: Option<&HashSet<String>>,
        ) -> Result<SearchResponseDto> {
            Ok(SearchResponseDto {
                total: self.hits.len(),
                results: self.hits.clone(),
                query_time_ms: 0,
            })
        }
        async fn search_branches(
            &self,
            _query: &str,
            _space_id: Option<&str>,
            allowed_document_ids: Option<&HashSet<String>>,
            _limit: usize,
            _vector_weight: f32,
            _bm25_weight: f32,
        ) -> lattice_core::application::contracts::search::QueryBranches {
            self.searched
                .lock()
                .expect("searches")
                .push(allowed_document_ids.cloned().unwrap_or_default());
            lattice_core::application::contracts::search::QueryBranches {
                branches: Vec::new(),
                answered: 2,
            }
        }
        fn fuse(&self, _branches: Vec<RankedBranch>, limit: usize) -> Vec<SearchResultDto> {
            self.hits.iter().take(limit).cloned().collect()
        }
        async fn rerank(
            &self,
            response: SearchResponseDto,
            _options: &lattice_core::application::contracts::search::RerankOptions,
        ) -> lattice_core::application::contracts::search::Reranked {
            lattice_core::application::contracts::search::Reranked {
                response,
                applied: false,
            }
        }
    }

    /// Twenty chunks of each document, numbered `{text} 1` to `{text} 20`.
    async fn records(
        documents: &[&str],
        text: &str,
    ) -> crate::features::conversation::repository::ConversationRepository {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .expect("pool");
        sqlx::raw_sql(
            "CREATE TABLE documents (id TEXT PRIMARY KEY, file_name TEXT, file_path TEXT, source_context TEXT,
                 owner_conversation_id TEXT);
             CREATE TABLE text_chunks (id TEXT, document_id TEXT, content TEXT, chunk_index INTEGER,
                 section TEXT, page_number INTEGER, from_ocr BOOLEAN NOT NULL DEFAULT 0);",
        )
        .execute(&pool)
        .await
        .expect("schema");
        for passage in chunks_of(documents, text, 20) {
            sqlx::query("INSERT OR IGNORE INTO documents VALUES (?, ?, ?, NULL, NULL)")
                .bind(&passage.document_id)
                .bind(&passage.name)
                .bind(&passage.path)
                .execute(&pool)
                .await
                .expect("document");
            sqlx::query(
                "INSERT INTO text_chunks (id, document_id, content, chunk_index) VALUES (?, ?, ?, ?)",
            )
            .bind(&passage.id)
            .bind(&passage.document_id)
            .bind(&passage.content)
            .bind(i64::try_from(passage.chunk_index).expect("index"))
            .execute(&pool)
            .await
            .expect("chunk");
        }
        crate::features::conversation::repository::ConversationRepository::new(pool)
    }

    #[tokio::test]
    async fn reading_from_the_start_is_one_source_not_the_whole_pool() {
        for (question, rewrite, [chosen, other], text) in [
            (
                "Welche Frist gilt für die Kündigung?",
                "Kündigungsfrist Rahmenvertrag",
                ["vertrag", "anhang"],
                "Kapitel",
            ),
            (
                "¿Cuánto dura la fase S del ciclo celular?",
                "duración fase S interfase",
                ["biologia", "genetica"],
                "Capítulo: el ciclo celular",
            ),
        ] {
            let records = records(&[chosen, other], text).await;
            let hit = chunks_of(&[chosen], text, 20)
                .into_iter()
                .nth(17)
                .map(|p| passage_result(p, "hybrid"))
                .expect("chunk 17");
            let library = RecordingLibrary {
                hits: vec![hit],
                searched: Default::default(),
            };
            let scope = super::super::super::SpaceDocumentScope {
                space_id: "space_general".into(),
                document_ids: HashSet::from([chosen.to_string(), other.to_string()]),
            };
            let plan = CorpusSearchPlan {
                queries: vec![rewrite.into()],
                opening_document_ids: vec![chosen.into()],
                start_at_beginning: true,
            };

            let results = retrieve(&records, &library, question, &plan, &scope, 10)
                .await
                .expect("retrieve")
                .results;

            let openings: Vec<_> = results
                .iter()
                .filter(|r| {
                    r.metadata.get("retrievalMethod")
                        == Some(&serde_json::json!("document_opening"))
                })
                .map(|r| r.id.clone())
                .collect();
            assert_eq!(
                openings,
                (0..5).map(|i| format!("{chosen}c{i}")).collect::<Vec<_>>(),
                "{question}"
            );
            assert!(
                results.iter().any(|r| r.id == format!("{chosen}c17")),
                "{question}"
            );
            let searched = library.searched.lock().expect("searches").clone();
            assert_eq!(searched.first(), Some(&scope.document_ids), "{question}");
            // The search inside the chosen document still ran.
            assert!(
                searched.contains(&HashSet::from([chosen.to_string()])),
                "{question}"
            );
        }
    }
}
