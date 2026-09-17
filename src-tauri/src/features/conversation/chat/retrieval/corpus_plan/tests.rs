use super::*;
use std::sync::Arc;

fn hit(id: &str, doc: &str, score: f32) -> SearchResultDto {
    SearchResultDto {
        id: id.into(),
        document_id: Some(doc.into()),
        score,
        title: doc.into(),
        content: id.into(),
        path: None,
        position: None,
        vector_score: None,
        bm25_score: None,
        vector_rank: None,
        bm25_rank: None,
        metadata: HashMap::new(),
    }
}

#[test]
fn ranks_fuse_across_different_score_scales_and_keep_document_diversity() {
    let vector = vec![hit("v", "a", 0.99), hit("shared", "b", 0.4)];
    let lexical = vec![hit("shared", "b", 100.0), hit("k", "c", 90.0)];
    let fused = fuse_branches(vec![(0.7, vector), (0.3, lexical)], 3);
    assert_eq!(fused[0].id, "shared");
    let mut crowded: Vec<_> = (0..12).map(|i| hit(&format!("a{i}"), "a", 1.0)).collect();
    crowded.push(hit("b0", "b", 0.1));
    let diverse = fuse_branches(vec![(1.0, crowded)], 8);
    assert_eq!(diverse[4].id, "b0");
    assert_eq!(diverse.len(), 8);
}

#[test]
fn exact_references_keep_subsections_and_deduplicate_before_the_budget() {
    assert_eq!(
        section_identifiers("Compare §706.07(a),706.07(b); 706.07(a) and 1.2. Ignore mpep-100.pdf"),
        ["706.07(a)", "706.07(b)", "1.2"]
    );
}

async fn section_fixture() -> sqlx::SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::raw_sql("CREATE TABLE documents(id TEXT, file_name TEXT, file_path TEXT, source_context TEXT);
        CREATE TABLE text_chunks(id TEXT, document_id TEXT, content TEXT, chunk_index INTEGER, section TEXT, page_number INTEGER);
        INSERT INTO documents VALUES ('a','Manual','/manual',NULL),('b','Other edition','/other',NULL),('private','Private','/private',NULL);
        INSERT INTO text_chunks VALUES
        ('a-sub','a','First subsection evidence',20,'Chapter 700 > 706.07 Actions > § 706.07(a) First action',2),
        ('a-nested','a','Nested exception',21,'Chapter 700 > 706.07 Actions > § 706.07(a) First action > 706.07(a)(1) Exception',3),
        ('a-other','a','Second subsection evidence',22,'Chapter 700 > Section 706.07(b) Second action',4),
        ('a-upper','a','Different uppercase subsection',23,'Chapter 700 > 706.07(A) Other action',4),
        ('b-sub','b','Other edition evidence',100,'706.07(a)',5),
        ('private-sub','private','Private evidence',0,'706.07(a) Secret',1);")
        .execute(&pool).await.unwrap();
    for index in 0..20 {
        sqlx::query(
            "INSERT INTO text_chunks VALUES (?, 'a', 'Parent evidence', ?, '706.07 Actions', 1)",
        )
        .bind(format!("parent-{index}"))
        .bind(index)
        .execute(&pool)
        .await
        .unwrap();
    }
    pool
}

#[tokio::test]
async fn exact_section_lookup_preserves_scope_subsections_and_comparison_coverage() {
    let repo = ConversationRepository::new(section_fixture().await);
    let allowed = HashSet::from(["a".into(), "b".into()]);
    let found = repo
        .retrieval_section_passages(&["706.07(a)".into()], &allowed)
        .await
        .unwrap();
    assert_eq!(
        found.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
        ["a-sub", "b-sub", "a-nested"]
    );
    assert_eq!(found[0].page_number, Some(2));
    let found = repo
        .retrieval_section_passages(&["706.07".into(), "706.07(b)".into()], &allowed)
        .await
        .unwrap();
    assert!(found.len() <= 12);
    assert!(
        found.iter().any(|p| p.id == "a-other"),
        "long parent must not starve the second requested section"
    );
    assert!(repo
        .retrieval_section_passages(&["706%".into()], &allowed)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn direct_evidence_survives_rewrite_omission_and_unavailable_search_models() {
    use crate::application::ports::MockEmbeddingPort;
    use crate::features::search::engine::{
        text_search::SqliteTextSearch, vector_search::USearchVectorIndex,
    };
    let pool = section_fixture().await;
    let repository = ConversationRepository::new(pool.clone());
    let embedder = Arc::new(MockEmbeddingPort::new_degraded());
    let index = Arc::new(USearchVectorIndex::new(384, None).unwrap());
    let semantic = SemanticSearchUseCase::new(embedder.clone(), index.clone());
    // The fixture deliberately lacks an FTS index too; both search branches fail.
    let hybrid = HybridSearchUseCase::new(embedder, index, Arc::new(SqliteTextSearch::new(pool)));
    let scope = super::super::SpaceDocumentScope {
        space_id: super::super::DEFAULT_SPACE_ID.into(),
        document_ids: ["a".to_owned()].into(),
    };
    let plan = CorpusSearchPlan {
        queries: vec!["first action".into()],
        opening_document_ids: vec![],
        start_at_beginning: false,
    };
    let found = retrieve(
        &repository,
        &semantic,
        &hybrid,
        "Explain 706.07(a)",
        &plan,
        &scope,
        8,
    )
    .await
    .unwrap();
    assert_eq!(
        found
            .results
            .iter()
            .map(|p| p.id.as_str())
            .collect::<Vec<_>>(),
        ["a-sub", "a-nested"]
    );
    assert!(retrieve(
        &repository,
        &semantic,
        &hybrid,
        "Explain 999.99",
        &plan,
        &scope,
        8
    )
    .await
    .is_err());
}

#[test]
fn plan_cannot_invent_documents_or_escape_scope() {
    let catalog = vec![CorpusDocument {
        sections: Vec::new(),
        source_context: None,
        id: "allowed".into(),
        name: "Guide".into(),
        opening: "Introduction".into(),
        chapter_number: None,
    }];
    let p = CorpusSearchPlan {
        queries: vec![" ".into(), "subject".into(), "SUBJECT".into()],
        opening_document_ids: vec!["outside".into(), "allowed".into(), "allowed".into()],
        start_at_beginning: false,
    }
    .validate(&catalog)
    .unwrap();
    assert_eq!(p.queries, ["subject"]);
    assert_eq!(p.opening_document_ids, ["allowed"]);
    assert!(CorpusSearchPlan {
        queries: vec![],
        start_at_beginning: false,
        opening_document_ids: vec![]
    }
    .validate(&catalog)
    .is_err());
}

#[tokio::test]
async fn catalog_and_openings_are_scoped_ordered_and_bounded() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::raw_sql("CREATE TABLE documents(id TEXT, file_name TEXT, file_path TEXT, source_context TEXT);
        CREATE TABLE text_chunks(id TEXT, document_id TEXT, content TEXT, chunk_index INTEGER, section TEXT, page_number INTEGER);
        INSERT INTO documents(id,file_name,file_path) VALUES ('a','Guide','/guide'), ('b','Private','/private');
        INSERT INTO text_chunks(id,document_id,content,chunk_index) VALUES ('a2','a','Second',2),('a0','a','Introduction',0),('a1','a','First',1),('b0','b','Private',0);").execute(&pool).await.unwrap();
    let repository = ConversationRepository::new(pool);
    let scope = HashSet::from(["a".to_string()]);
    let catalog = repository.retrieval_catalog(&scope).await.unwrap();
    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog[0].opening, "Introduction");
    let openings = repository
        .retrieval_openings(&["b".into(), "a".into(), "a".into()], &scope, 2)
        .await
        .unwrap();
    assert_eq!(
        openings.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
        ["a0", "a1"]
    );
    assert!(repository
        .retrieval_catalog(&HashSet::new())
        .await
        .unwrap()
        .is_empty());
}

#[test]
fn beginning_uses_explicit_chapter_sequence_not_filename_or_upload_order() {
    let document = |id: &str, name: &str, opening: &str| CorpusDocument {
        sections: Vec::new(),
        source_context: None,
        id: id.into(),
        name: name.into(),
        opening: opening.into(),
        chapter_number: CorpusDocument::opening_chapter_number(opening),
    };
    let catalog = vec![
        document("changes", "guide-00-changes.pdf", "Summary of changes"),
        document("later", "guide-01.pdf", "Chapter 200 Later material"),
        document("intro", "guide-introduction.pdf", "Introduction"),
        document("unrelated", "another-01.pdf", "Chapter 1 Unrelated"),
        document("first", "guide-09.pdf", "Chapter 100 First material"),
    ];
    let plan = CorpusSearchPlan {
        queries: vec!["guide".into()],
        opening_document_ids: vec!["intro".into()],
        start_at_beginning: true,
    }
    .validate(&catalog)
    .unwrap();
    assert_eq!(plan.opening_document_ids, ["intro", "first"]);
    let plan = CorpusSearchPlan {
        queries: vec!["guide".into()],
        opening_document_ids: vec!["changes".into()],
        start_at_beginning: true,
    }
    .validate(&catalog)
    .unwrap();
    assert_eq!(
        plan.opening_document_ids,
        ["intro", "first"],
        "legacy imports must use the actual introduction instead of a change log"
    );
    assert_eq!(
        CorpusDocument::opening_chapter_number("See chapter 1 for details"),
        None
    );
    assert_eq!(
        CorpusDocument::opening_chapter_number("CHAPTER 12: Contents"),
        Some(12)
    );
    let single = vec![document(
        "first",
        "guide-01-introduction.pdf",
        "Chapter 1 Introduction",
    )];
    let plan = CorpusSearchPlan {
        queries: vec!["guide".into()],
        opening_document_ids: vec!["first".into()],
        start_at_beginning: true,
    }
    .validate(&single)
    .unwrap();
    assert_eq!(plan.opening_document_ids, ["first"]);
}

/// Uses a read-only snapshot of the real indexed corpus. Recomputes sampled
/// vectors and exercises the SAME planner, search, scope and prompt functions
/// as chat. Does not modify conversation history or the user's database.
#[tokio::test]
#[ignore = "requires LATTICE_RETRIEVAL_DB, LATTICE_LLAMACPP_SETTINGS and LATTICE_RETRIEVAL_REPORT"]
async fn live_corpus_retrieval_and_answer() {
    use crate::application::ports::EmbeddingPort;
    use crate::features::embedding::candle_service::CandleEmbeddingService;
    use crate::features::llm::llama_cpp::LlamaCppLlm;
    use crate::features::search::engine::text_search::SqliteTextSearch;
    use crate::features::search::engine::vector_search::USearchVectorIndex;

    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(std::env::var("LATTICE_RETRIEVAL_DB").unwrap())
                .read_only(true),
        )
        .await
        .unwrap();
    let model_path: String =
        sqlx::query_scalar("SELECT base_path FROM models WHERE is_active_for_embedding=1")
            .fetch_one(&pool)
            .await
            .unwrap();
    let embedding = Arc::new(CandleEmbeddingService::open_unregistered(model_path).unwrap());
    let rows: Vec<(String, Vec<u8>, String, String, String)> = sqlx::query_as("SELECT c.id, e.embedding, c.content, c.document_id, COALESCE(NULLIF(c.contextualized_content,''),c.content) FROM text_chunks c JOIN text_embeddings e ON e.chunk_id=c.id WHERE e.model_name=? ORDER BY c.document_id,c.chunk_index")
        .bind(EmbeddingPort::model_identity(embedding.as_ref())).fetch_all(&pool).await.unwrap();
    assert!(
        !rows.is_empty(),
        "active model fingerprint has no matching vectors"
    );
    let mut minimum_cosine = 1.0_f32;
    for row in rows.iter().step_by((rows.len() / 12).max(1)).take(12) {
        let fresh = embedding.embed_single(&row.4).await.unwrap();
        let stored = crate::features::embedding::encoding::decode_embedding(&row.1).unwrap();
        let cosine: f32 = fresh.iter().zip(&stored).map(|(a, b)| a * b).sum();
        minimum_cosine = minimum_cosine.min(cosine);
        assert!(
            cosine > 0.999,
            "stored vector does not reproduce: {} similarity={cosine}",
            row.0
        );
    }
    println!("Embedding audit: {} vectors match active model; 12 recomputed samples, minimum cosine {minimum_cosine}", rows.len());
    let index = Arc::new(USearchVectorIndex::new(embedding.dimension(), None).unwrap());
    let count = index
        .rebuild_from_embeddings(
            rows.into_iter()
                .map(|(id, bytes, content, doc, _)| {
                    (
                        crate::features::embedding::encoding::vector_key(&id),
                        crate::features::embedding::encoding::decode_embedding(&bytes).unwrap(),
                        content,
                        id,
                        doc,
                    )
                })
                .collect(),
        )
        .unwrap();
    assert!(count > 0);
    let semantic = SemanticSearchUseCase::new(embedding.clone(), index.clone());
    let hybrid = HybridSearchUseCase::new(
        embedding.clone(),
        index.clone(),
        Arc::new(SqliteTextSearch::new(pool.clone())),
    );
    let repository = ConversationRepository::new(pool.clone());
    let conversation: String = match std::env::var("LATTICE_RETRIEVAL_CONVERSATION_ID") {
        Ok(id) => id,
        Err(_) => {
            sqlx::query_scalar("SELECT id FROM conversations ORDER BY updated_at DESC LIMIT 1")
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    let (space_id, document_ids) = repository
        .retrieval_document_scope(&conversation)
        .await
        .unwrap()
        .unwrap();
    let scope = super::super::SpaceDocumentScope {
        space_id,
        document_ids,
    };
    let question: String = match std::env::var("LATTICE_RETRIEVAL_QUESTION_FILE") {
        Ok(path) => std::fs::read_to_string(path).unwrap(),
        Err(_) => sqlx::query_scalar("SELECT content FROM conversation_messages WHERE conversation_id=? AND role='user' ORDER BY created_at LIMIT 1").bind(&conversation).fetch_optional(&pool).await.unwrap()
            .expect("No user message in the selected conversation; provide LATTICE_RETRIEVAL_QUESTION_FILE"),
    };
    let catalog = repository
        .retrieval_catalog(&scope.document_ids)
        .await
        .unwrap();
    let settings: serde_json::Value = serde_json::from_slice(
        &std::fs::read(std::env::var("LATTICE_LLAMACPP_SETTINGS").unwrap()).unwrap(),
    )
    .unwrap();
    let config: crate::application::contracts::settings::LLMSettingsDto =
        serde_json::from_value(settings["settings"]["llm"].clone()).unwrap();
    let llm = LlamaCppLlm::new(&config).unwrap();
    let started = Instant::now();
    let plan = plan(&llm, &question, None, &catalog, &HashMap::new())
        .await
        .unwrap();
    println!(
        "Live plan ({:?}): {}",
        started.elapsed(),
        serde_json::to_string(&plan).unwrap()
    );
    let search_started = Instant::now();
    let response = retrieve(
        &repository,
        &semantic,
        &hybrid,
        &question,
        &plan,
        &scope,
        16,
    )
    .await
    .unwrap();
    let mut names: Vec<_> = response
        .results
        .iter()
        .filter_map(|r| {
            catalog
                .iter()
                .find(|d| Some(&d.id) == r.document_id.as_ref())
                .map(|d| d.name.clone())
        })
        .collect();
    names.sort();
    names.dedup();
    println!(
        "Live retrieval: {} passages from {:?}, search {:?}",
        response.results.len(),
        names,
        search_started.elapsed()
    );
    assert!(
        names
            .iter()
            .any(|n| n.contains("introduction") || n.contains("foreword")),
        "failed to retrieve introductory evidence"
    );
    assert!(
        names.iter().any(|n| n == "mpep-0100.pdf"),
        "failed to retrieve organization/first chapter"
    );
    assert!(response
        .results
        .iter()
        .all(|r| scope.document_ids.contains(r.document_id.as_ref().unwrap())));
    // Exact-topic recall must also work; a corpus overview alone is insufficient.
    for query in [
        "rejection contrasted with objection 706",
        "access confidentiality patent applications 101",
    ] {
        let query_plan = CorpusSearchPlan {
            queries: vec![query.into()],
            start_at_beginning: false,
            opening_document_ids: vec![],
        };
        let found = retrieve(
            &repository,
            &semantic,
            &hybrid,
            query,
            &query_plan,
            &scope,
            16,
        )
        .await
        .unwrap();
        let expected = if query.contains("706") {
            "mpep-0700.pdf"
        } else {
            "mpep-0100.pdf"
        };
        assert!(
            found.results.iter().take(5).any(|r| catalog
                .iter()
                .any(|d| Some(&d.id) == r.document_id.as_ref() && d.name == expected)),
            "specific query missed expected chapter: {query}"
        );
    }
    let mut citations = HashMap::new();
    for r in &response.results {
        let next = citations.len() as u32 + 1;
        citations.entry(r.id.clone()).or_insert(next);
    }
    let context = crate::features::conversation::chat::prompting::build_kb_context(
        &response.results.iter().collect::<Vec<_>>(),
        &citations,
    )
    .unwrap();
    let prompts =
        crate::features::conversation::chat::normalize_prompt_settings(config.prompts.clone());
    let prompt = crate::features::conversation::chat::prompting::PromptMessageBuilder::new(
        &prompts,
        &question,
        false,
        crate::features::conversation::chat::SearchFlags {
            force_kb_search: true,
            force_web_search: false,
            force_wiki_search: false,
            deep_research_mode: false,
            force_followup_mode: false,
        },
    )
    .with_kb_context(Some(context))
    .build();
    // Preserve retrieval diagnostics even when generation exceeds the same
    // five-minute limit as the production tool loop. A functional answer that
    // arrives after that deadline is not an end-to-end success in the app.
    let report_path = std::env::var("LATTICE_RETRIEVAL_REPORT").unwrap();
    std::fs::write(
        &report_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "status":"retrieval_passed_generation_pending",
            "embedding_count":count,"recomputed_samples":12,
            "minimum_cosine":minimum_cosine,"plan":plan,"sources":names,
            "passages":response.results.len(),"question":question,
        }))
        .unwrap(),
    )
    .unwrap();
    let generation_started = Instant::now();
    let progress = std::sync::Mutex::new((None::<f64>, String::new()));
    let on_text = |text: String| {
        if !text.is_empty() {
            let mut progress = progress.lock().unwrap();
            progress
                .0
                .get_or_insert_with(|| generation_started.elapsed().as_secs_f64());
            progress.1.push_str(&text);
        }
        Ok(())
    };
    let request = CompletionRequest {
        input: crate::application::services::completion_input::from_context(
            &prompts.system_prompt,
            &[],
            &prompt,
        ),
        ..Default::default()
    };
    let result = tokio::time::timeout(
        Duration::from_secs(300),
        llm.complete_with_progress(&request, &on_text),
    )
    .await;
    let progress = progress.lock().unwrap();
    let (completion, failure) = match result {
        Ok(Ok(completion)) => (Some(completion), None),
        Ok(Err(error)) => (None, Some(error.to_string())),
        Err(_) => (
            None,
            Some("generation exceeded the production five-minute deadline".into()),
        ),
    };
    let answer = completion
        .as_ref()
        .map(|c| c.text.as_str())
        .unwrap_or_default();
    let smoke_passed = failure.is_none()
        && completion
            .as_ref()
            .is_some_and(|c| c.finish_reason == "stop")
        && answer.len() > 300
        && answer.contains('[')
        && progress.1 == answer;
    // Save measurements on failure too. A bracket and enough text are smoke
    // checks, not independently reviewed citation or answer-quality judgments.
    let report = serde_json::json!({
        "status":if smoke_passed {"completed_smoke_checks"} else {"generation_failed"},
        "failure":failure,"embedding_count":count,"recomputed_samples":12,
        "minimum_cosine":minimum_cosine,"plan":plan,"sources":names,
        "passages":response.results.len(),"elapsed_seconds":started.elapsed().as_secs_f64(),
        "generation_seconds":generation_started.elapsed().as_secs_f64(),
        "first_answer_seconds":progress.0,"streamed_characters":progress.1.chars().count(),
        "finish_reason":completion.as_ref().map(|c| &c.finish_reason),
        "input_tokens":completion.as_ref().map(|c| c.input_tokens),
        "output_tokens":completion.as_ref().map(|c| c.output_tokens),
        "question":question,"answer":answer,
    });
    std::fs::write(report_path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    assert!(
        smoke_passed,
        "generation or streaming smoke checks failed; timing diagnostics saved"
    );
    println!(
        "Live sourced answer: {} characters, total {:?}",
        answer.len(),
        started.elapsed()
    );
}

#[test]
fn explicit_book_order_keeps_editions_separate_and_reaches_the_first_chapter() {
    use crate::domain::value_objects::source_context::{SourceContext, SourceGroup, StructureMode};
    let group = SourceGroup {
        id: "book-a".into(),
        title: "Manual".into(),
        edition: Some("2026".into()),
        description: None,
        ordered: true,
        structure: StructureMode::Sections,
    };
    let mut catalog: Vec<_> = [
        "Table of contents",
        "Change summary",
        "Foreword",
        "Introduction",
        "Chapter 100",
        "Chapter 200",
    ]
    .iter()
    .enumerate()
    .map(|(position, name)| CorpusDocument {
        id: format!("a{position}"),
        name: name.to_string(),
        opening: name.to_string(),
        chapter_number: CorpusDocument::opening_chapter_number(name),
        sections: vec![],
        source_context: Some(SourceContext {
            group: group.clone(),
            position: position as u32,
        }),
    })
    .collect();
    let request = "Help me train for the Patent Bar exam, Manual of Patent Examining Procedure is entirely vectorized";
    let local = fallback_with_catalog(request, &catalog);
    assert!(local.start_at_beginning);
    assert!(local.opening_document_ids.contains(&"a3".to_owned()));
    assert!(local.opening_document_ids.contains(&"a4".to_owned()));
    assert!(!local.opening_document_ids.contains(&"a1".to_owned()));
    assert!(ordered_learning_plan("Help me study section 706.07(a)", &catalog).is_none());
    assert!(ordered_learning_plan("Help me learn about appeals", &catalog).is_none());
    let mut other = catalog[4].clone();
    other.id = "other-edition".into();
    other.source_context.as_mut().unwrap().group.id = "book-b".into();
    other.source_context.as_mut().unwrap().position = 0;
    catalog.push(other);
    assert!(
        ordered_learning_plan(request, &catalog).is_none(),
        "Do not guess between editions"
    );
    let plan = CorpusSearchPlan {
        queries: vec!["start reading".into()],
        opening_document_ids: vec!["a0".into()],
        start_at_beginning: true,
    }
    .validate(&catalog)
    .unwrap();
    assert!(plan.opening_document_ids.contains(&"a3".to_owned()));
    assert!(plan.opening_document_ids.contains(&"a4".to_owned()));
    assert!(!plan.opening_document_ids.contains(&"a1".to_owned()));
    assert!(!plan
        .opening_document_ids
        .contains(&"other-edition".to_owned()));
    assert!(plan
        .opening_document_ids
        .windows(2)
        .all(|ids| ids[0] < ids[1]));
}

/// Records what the planner was asked and replays canned JSON plans.
struct PlannerStub {
    responses: std::sync::Mutex<std::collections::VecDeque<String>>,
    prompts: std::sync::Mutex<Vec<String>>,
    systems: std::sync::Mutex<Vec<Vec<String>>>,
}

impl PlannerStub {
    fn new(responses: &[&str]) -> Self {
        Self {
            responses: std::sync::Mutex::new(responses.iter().map(|r| (*r).to_string()).collect()),
            prompts: std::sync::Mutex::new(Vec::new()),
            systems: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> usize {
        self.prompts.lock().unwrap().len()
    }

    fn last_prompt(&self) -> String {
        self.prompts.lock().unwrap().last().cloned().unwrap()
    }
}

#[async_trait::async_trait]
impl LLMPort for PlannerStub {
    async fn generate(
        &self,
        prompt: &str,
        context: &[String],
        _images: Option<Vec<String>>,
    ) -> Result<String> {
        self.prompts.lock().unwrap().push(prompt.to_string());
        self.systems.lock().unwrap().push(context.to_vec());
        Ok(self
            .responses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| "{}".to_string()))
    }

    async fn generate_streaming(
        &self,
        _prompt: &str,
        _context: &[String],
        _images: Option<Vec<String>>,
    ) -> Result<Box<dyn futures::Stream<Item = Result<String>> + Send + Unpin + '_>> {
        Ok(Box::new(Box::pin(futures::stream::empty())))
    }

    fn model_name(&self) -> &str {
        "planner-stub"
    }

    fn max_context_tokens(&self) -> usize {
        32_000
    }

    fn count_tokens(&self, text: &str) -> usize {
        text.split_whitespace().count()
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

fn correction_catalog() -> Vec<CorpusDocument> {
    vec![CorpusDocument {
        sections: Vec::new(),
        source_context: None,
        id: "allowed".into(),
        name: "Employment handbook".into(),
        opening: "Introduction".into(),
        chapter_number: None,
    }]
}

#[tokio::test]
async fn a_correction_reuses_the_planner_prompt_and_appends_what_already_failed() {
    let stub = PlannerStub::new(&[
        r#"{"queries":["garden leave notice period"],"opening_document_ids":["allowed"],"start_at_beginning":true}"#,
    ]);
    let correction = CorrectionRequest {
        queries_already_tried: vec!["noncompete clauses".into()],
        top_result_titles: vec!["Quarterly revenue".into()],
        why_insufficient: vec!["low_term_coverage"],
    };

    let plan = plan_correction(
        &stub,
        "What about garden leave?",
        None,
        &correction_catalog(),
        &correction,
    )
    .await
    .unwrap();

    assert_eq!(plan.queries, ["garden leave notice period"]);
    // One system prompt for both passes: a correction is evidence in the user
    // turn, not a second prompt to keep in sync.
    let systems = stub.systems.lock().unwrap();
    assert_eq!(systems[0], [format!("System: {PLANNER_SYSTEM}")]);
    let prompt = stub.last_prompt();
    assert!(prompt.contains("noncompete clauses"), "{prompt}");
    assert!(prompt.contains("Quarterly revenue"), "{prompt}");
    assert!(prompt.contains("low_term_coverage"), "{prompt}");
    // A correction is a focused search. It must not re-enter ordered reading,
    // which is how one retry could otherwise cascade into a different pipeline.
    assert!(plan.opening_document_ids.is_empty());
    assert!(!plan.start_at_beginning);
}

#[tokio::test]
async fn a_correction_that_repeats_a_failed_query_is_rejected_after_exactly_one_call() {
    let stub = PlannerStub::new(&[
        r#"{"queries":["Noncompete Clauses"],"opening_document_ids":[],"start_at_beginning":false}"#,
        r#"{"queries":["something new"],"opening_document_ids":[],"start_at_beginning":false}"#,
    ]);
    let correction = CorrectionRequest {
        queries_already_tried: vec!["noncompete clauses".into()],
        top_result_titles: Vec::new(),
        why_insufficient: vec!["low_top_score"],
    };

    let result = plan_correction(
        &stub,
        "What about noncompete clauses?",
        None,
        &correction_catalog(),
        &correction,
    )
    .await;

    assert!(result.is_err());
    // The cap is the point: a rejected correction does not try again.
    assert_eq!(stub.calls(), 1);
}
