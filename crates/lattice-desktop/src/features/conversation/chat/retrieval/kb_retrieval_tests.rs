use super::*;
use crate::features::conversation::chat::test_runtime::{FakeRuntime, TestVault};
use crate::features::conversation::repository::ConversationRepository;
use lattice_core::application::contracts::search::{
    QueryBranches, RankedBranch, Reranked, SearchRequestDto,
};
use lattice_core::application::ports::hybrid_search::HybridSearchTrait;
use lattice_core::domain::qa::hyde::HyDEInterpretation;
use std::sync::Mutex;

/// Finds the same passages for every query and reranks by reversing the
/// pool, recording how many candidates each rerank judged.
struct ReversingLibrary {
    hits: Vec<SearchResultDto>,
    judged: Mutex<Vec<usize>>,
}

#[async_trait::async_trait]
impl HybridSearchTrait for ReversingLibrary {
    async fn execute_scoped(
        &self,
        _request: SearchRequestDto,
        _space_id: Option<&str>,
        _allowed_document_ids: Option<&HashSet<String>>,
    ) -> lattice_core::shared::error::Result<SearchResponseDto> {
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
        _allowed_document_ids: Option<&HashSet<String>>,
        _limit: usize,
        _vector_weight: f32,
        _bm25_weight: f32,
    ) -> QueryBranches {
        QueryBranches {
            branches: Vec::new(),
            answered: 2,
        }
    }
    fn fuse(&self, _branches: Vec<RankedBranch>, limit: usize) -> Vec<SearchResultDto> {
        self.hits.iter().take(limit).cloned().collect()
    }
    async fn rerank(&self, mut response: SearchResponseDto, _options: &RerankOptions) -> Reranked {
        self.judged.lock().unwrap().push(response.results.len());
        response.results.reverse();
        Reranked {
            response,
            applied: true,
        }
    }
}

/// A library of two documents: `chosen`, which the plan reads in order,
/// and `other`, which only the searches reach.
async fn two_documents(
    chosen: (&str, &[String]),
    other: (&str, &[String]),
) -> Arc<dyn ChatRecords> {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::raw_sql(
        "CREATE TABLE documents (id TEXT, file_name TEXT, file_path TEXT, source_context TEXT,
             owner_conversation_id TEXT);
         CREATE TABLE text_chunks (id TEXT, document_id TEXT, content TEXT, chunk_index INTEGER,
             section TEXT, page_number INTEGER, from_ocr BOOLEAN NOT NULL DEFAULT 0);",
    )
    .execute(&pool)
    .await
    .unwrap();
    for (document, chunks) in [chosen, other] {
        sqlx::query("INSERT INTO documents VALUES (?, ?, ?, NULL, NULL)")
            .bind(document)
            .bind(format!("{document}.md"))
            .bind(format!("/vault/{document}.md"))
            .execute(&pool)
            .await
            .unwrap();
        for (index, content) in chunks.iter().enumerate() {
            sqlx::query(
                "INSERT INTO text_chunks (id, document_id, content, chunk_index) \
                 VALUES (?, ?, ?, ?)",
            )
            .bind(format!("{document}-{index}"))
            .bind(document)
            .bind(content)
            .bind(index as i64)
            .execute(&pool)
            .await
            .unwrap();
        }
    }
    Arc::new(ConversationRepository::new(pool))
}

fn hit(document: &str, index: usize, content: &str) -> SearchResultDto {
    SearchResultDto {
        id: format!("{document}-{index}"),
        title: format!("{document}.md"),
        content: content.to_string(),
        score: 0.03,
        path: Some(format!("/vault/{document}.md")),
        document_id: Some(document.to_string()),
        position: Some(index),
        vector_score: None,
        bm25_score: None,
        vector_rank: None,
        bm25_rank: None,
        metadata: Default::default(),
    }
}

/// A pool laid out for the reranker's shortlist can lead with more
/// openings than half the passages the turn keeps; only that half stays
/// pinned, so the cut still keeps what the searches found.
#[test]
fn only_half_the_kept_passages_stay_pinned_as_openings() {
    for document in ["Vertragswerk Cloud", "Requiem K. 626 Programmheft"] {
        let mut results: Vec<SearchResultDto> = (0..12)
            .map(|index| {
                let mut opening = hit(document, index, "Abschnitt");
                opening.metadata.insert(
                    "retrievalMethod".into(),
                    serde_json::json!("document_opening"),
                );
                opening
            })
            .collect();
        results.push(hit("Nachweis", 0, "Eskalationstreffen binnen 31 Wochen"));

        bound_leading_structural(&mut results, 8);

        let leading = results
            .iter()
            .take_while(|result| super::super::rerank::is_structural_result(result))
            .count();
        assert_eq!(leading, 4, "{document}");
        assert_eq!(results.len(), 13, "{document}: nothing is dropped");
        assert_eq!(
            results.get(4).map(|result| result.id.as_str()),
            Some("Nachweis-0"),
            "{document}: the searched passage follows the pinned half"
        );
    }
}

/// A plan the planner flagged to read in order is gathered to the
/// reranker's shortlist and reranked like any other: the chosen
/// document's opening still leads, and what the searches found behind it
/// takes the reranker's order.
#[tokio::test]
async fn an_ordered_reading_plan_is_gathered_wide_and_reranked() {
    let limit = 10;
    for (question, query, (chosen, chapter), (other, note)) in [
        (
            "Welche Frist gilt für die Kündigung?",
            "Kündigungsfrist Vertrag",
            ("vertrag", "Kapitel"),
            (
                "protokoll",
                "Die Kündigung ist mit einer Frist von Wochen möglich.",
            ),
        ),
        (
            "¿Cuánto dura la fase S del ciclo celular?",
            "duración fase S ciclo celular",
            ("biologia", "Capítulo"),
            (
                "apuntes",
                "La fase S dura varias horas en una célula humana.",
            ),
        ),
    ] {
        let chapters: Vec<String> = (1..=2 * limit).map(|n| format!("{chapter} {n}")).collect();
        let notes: Vec<String> = (0..RerankOptions::candidate_pool(limit))
            .map(|_| note.to_string())
            .collect();
        let library = Arc::new(ReversingLibrary {
            hits: notes
                .iter()
                .enumerate()
                .map(|(index, content)| hit(other, index, content))
                .collect(),
            judged: Mutex::default(),
        });
        let records = two_documents((chosen, &chapters), (other, &notes)).await;
        let runtime = FakeRuntime {
            vault: Some(TestVault {
                records: Arc::clone(&records),
                search: library.clone(),
            }),
            ..FakeRuntime::default()
        };
        let plan = super::super::corpus_plan::CorpusSearchPlan {
            queries: vec![query.to_string()],
            opening_document_ids: vec![chosen.to_string()],
            start_at_beginning: true,
        };
        let scope = super::super::SpaceDocumentScope {
            space_id: super::super::DEFAULT_SPACE_ID.to_string(),
            document_ids: HashSet::from([chosen.to_string(), other.to_string()]),
        };

        let pool = super::super::corpus_plan::retrieve(
            records.as_ref(),
            library.as_ref(),
            question,
            &plan,
            &scope,
            candidate_limit(true, limit),
        )
        .await
        .unwrap();
        let (reranked, applied) = rerank_pass(
            &runtime,
            true,
            &plan.queries.join(" "),
            &HyDEInterpretation::raw_only(question.to_string(), QueryType::Question),
            pool,
            limit,
            &RetrievalTuningSettingsDto::default(),
            &mut super::super::RetrievalSubTimingMetrics::default(),
        )
        .await;

        let judged = library.judged.lock().unwrap().clone();
        assert!(applied, "{question}");
        assert_eq!(judged.len(), 1, "{question}: one rerank for the pass");
        assert!(
            judged.iter().all(|&candidates| candidates > limit),
            "{question}: the reranker judged {judged:?} candidates for a cut of {limit}"
        );
        let first = format!("{chosen}-0");
        assert_eq!(
            reranked.results.first().map(|r| r.id.as_str()),
            Some(first.as_str()),
            "{question}: the reading still starts at the opening"
        );
        let searched: Vec<usize> = reranked
            .results
            .iter()
            .filter(|r| r.document_id.as_deref() == Some(other))
            .filter_map(|r| r.position)
            .collect();
        assert!(
            searched.windows(2).all(|pair| pair.first() > pair.last()),
            "{question}: the searched passages keep the reranker's order: {searched:?}"
        );
    }
}

#[test]
fn the_candidate_pool_is_the_rerankers_shortlist_only_when_reranking() {
    for limit in [1, 10, 40] {
        assert_eq!(
            candidate_limit(true, limit),
            RerankOptions::candidate_pool(limit)
        );
        assert_eq!(candidate_limit(false, limit), limit);
    }
}

#[test]
fn unavailable_reason_is_clipped_to_the_trace_limit() {
    let long = "é".repeat(UNAVAILABLE_REASON_MAX_CHARS * 3);
    let reason = unavailable_reason(&long);
    assert_eq!(reason.chars().count(), UNAVAILABLE_REASON_MAX_CHARS);
    assert_eq!(unavailable_reason("index offline"), "index offline");
}
