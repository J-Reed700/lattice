//! # Ask Question Use Case
//!
//! Implements Retrieval-Augmented Generation (RAG) for question answering.
//!
//! This use case orchestrates:
//! 1. Query interpretation via HyDE (classify + hypothetical doc generation)
//! 2. Query embedding generation
//! 3. Relevant context retrieval from vector store
//! 4. Context window budget management (token-aware truncation)
//! 5. LLM prompt construction with inline citations
//! 6. Answer generation with source metadata

use async_stream::stream;
use futures::stream::{Stream, StreamExt};
use std::sync::Arc;

use crate::application::dtos::qa_dto::{QARequestDto, QAResponseDto, SourceDto, StreamChunkDto};
use crate::application::mappers::search_mapper::infer_category;
use crate::application::mappers::SearchMapper;
use crate::application::ports::{
    ChunkRepositoryPort, DocumentRepositoryPort, EmbeddingPort, LLMPort, VectorSearchPort,
};
use crate::domain::entities::search_result::SearchResult;
use crate::domain::qa::hyde::{HyDEInterpretation, QueryType};
use crate::infrastructure::services::hyde::HyDEService;
use crate::shared::error::Result;
use crate::shared::text_utils::{build_excerpt, extract_highlight_terms};
use tracing::{debug, error, info, warn};

// ============================================================================
// Constants
// ============================================================================

/// Fraction of the context window reserved for the LLM response.
/// Prevents the prompt from consuming the entire context budget.
const RESPONSE_TOKEN_BUDGET_RATIO: f64 = 0.25;

/// Overhead tokens for the prompt template itself (instructions, formatting).
const PROMPT_OVERHEAD_TOKENS: usize = 200;

/// Maximum number of chunks a document can have before we switch to
/// snapshot mode (window around matched chunks) instead of full-document mode.
const FULL_DOCUMENT_CHUNK_THRESHOLD: usize = 20;

/// Number of sibling chunks to include before and after each matched chunk
/// when a document exceeds the full-document threshold.
const SNAPSHOT_WINDOW_RADIUS: usize = 5;

// ============================================================================
// Source metadata helpers (pure functions)
// ============================================================================

fn validate_file_name(name: &str, document_id: &str) -> String {
    let trimmed = name.trim();

    if trimmed.is_empty() {
        warn!(
            "Document {} has empty file_name, using fallback",
            document_id
        );
        return format!("document_{}", document_id);
    }

    const MAX_FILE_NAME_LENGTH: usize = 255;
    if trimmed.len() > MAX_FILE_NAME_LENGTH {
        warn!(
            "Document {} has oversized file_name: {} chars",
            document_id,
            trimmed.len()
        );
        return format!("{}...", &trimmed[..MAX_FILE_NAME_LENGTH]);
    }

    trimmed.to_string()
}

fn validate_file_path(path: &std::path::Path, document_id: &str) -> String {
    let path_str = path.to_string_lossy();

    if path_str.is_empty() {
        warn!("Document {} has empty file_path", document_id);
        return String::from("[unknown path]");
    }

    if path_str.contains('\u{FFFD}') {
        warn!("Document {} file_path contains invalid UTF-8", document_id);
    }

    path_str.to_string()
}

fn validate_file_size(size: i64, document_id: &str) -> i64 {
    if size < 0 {
        warn!(
            "Document {} has negative file_size_bytes: {}, using 0",
            document_id, size
        );
        return 0;
    }

    const MAX_REASONABLE_SIZE: i64 = 1_099_511_627_776; // 1TB
    if size > MAX_REASONABLE_SIZE {
        warn!(
            "Document {} has suspiciously large size: {} bytes",
            document_id, size
        );
    }

    size
}

// ============================================================================
// Use Case
// ============================================================================

/// Ask question use case implementing RAG with HyDE.
///
/// Provides question-answering capabilities by:
/// - Classifying queries (greeting, question, command) using HyDE
/// - Generating hypothetical answers for better retrieval
/// - Retrieving relevant context from indexed documents
/// - Applying token-budget-aware context truncation
/// - Generating answers using LLM with retrieved context
/// - Providing source citations for transparency
pub struct AskQuestionUseCase {
    hyde_service: Arc<HyDEService>,
    embedding_service: Arc<dyn EmbeddingPort>,
    vector_search: Arc<dyn VectorSearchPort>,
    llm: Arc<dyn LLMPort>,
    document_repo: Arc<dyn DocumentRepositoryPort>,
    chunk_repo: Arc<dyn ChunkRepositoryPort>,
}

impl AskQuestionUseCase {
    pub fn new(
        hyde_service: Arc<HyDEService>,
        embedding_service: Arc<dyn EmbeddingPort>,
        vector_search: Arc<dyn VectorSearchPort>,
        llm: Arc<dyn LLMPort>,
        document_repo: Arc<dyn DocumentRepositoryPort>,
        chunk_repo: Arc<dyn ChunkRepositoryPort>,
    ) -> Self {
        Self {
            hyde_service,
            embedding_service,
            vector_search,
            llm,
            document_repo,
            chunk_repo,
        }
    }

    // ====================================================================
    // Public entry points
    // ====================================================================

    /// Execute question answering (non-streaming).
    pub async fn execute(&self, request: QARequestDto) -> Result<QAResponseDto> {
        let start = std::time::Instant::now();

        // Step 1: Interpret query with HyDE
        info!("Interpreting query with HyDE: {}", request.question);
        let interpretation = self.hyde_service.interpret_query(&request.question).await?;

        info!(
            "Query classified as {:?} with strategy {:?}",
            interpretation.query_type, interpretation.search_strategy
        );

        // Step 2: Branch based on query type
        let (answer, sources) = match interpretation.query_type {
            QueryType::Greeting => {
                info!("Processing greeting without context retrieval");
                (
                    self.generate_greeting_response(&request.question).await?,
                    Vec::new(),
                )
            }
            QueryType::Question | QueryType::Command | QueryType::Followup => {
                info!("Processing {} with RAG pipeline", interpretation.query_type);
                self.execute_rag(&request, &interpretation).await?
            }
        };

        let generation_time_ms = start.elapsed().as_millis() as u64;

        Ok(QAResponseDto {
            answer,
            sources,
            confidence: None,
            metadata: Some(crate::application::dtos::qa_dto::QAMetadataDto {
                model: self.llm.model_name().to_string(),
                prompt_tokens: None,
                completion_tokens: None,
                total_tokens: None,
                generation_time_ms: Some(generation_time_ms),
            }),
        })
    }

    /// Execute question answering with streaming response.
    pub async fn execute_stream(
        &self,
        request: QARequestDto,
    ) -> Result<Box<dyn Stream<Item = Result<StreamChunkDto>> + Send + Unpin + '_>> {
        // Step 1: Interpret query with HyDE
        info!(
            "Interpreting query with HyDE (streaming): {}",
            request.question
        );
        let interpretation = self.hyde_service.interpret_query(&request.question).await?;

        // Step 2: Branch based on query type
        match interpretation.query_type {
            QueryType::Greeting => {
                info!("Processing greeting without context retrieval (streaming)");
                let greeting = self.generate_greeting_response(&request.question).await?;

                let s = stream! {
                    yield Ok(StreamChunkDto::Token { content: greeting });
                    yield Ok(StreamChunkDto::Done);
                };

                let boxed: Box<dyn Stream<Item = Result<StreamChunkDto>> + Send + Unpin> =
                    Box::new(Box::pin(s));
                Ok(boxed)
            }
            QueryType::Question | QueryType::Command | QueryType::Followup => {
                info!(
                    "Processing {} with RAG pipeline (streaming)",
                    interpretation.query_type
                );
                self.execute_rag_stream(&request, &interpretation).await
            }
        }
    }

    // ====================================================================
    // RAG pipeline
    // ====================================================================

    /// Execute RAG pipeline with streaming.
    async fn execute_rag_stream(
        &self,
        request: &QARequestDto,
        interpretation: &HyDEInterpretation,
    ) -> Result<Box<dyn Stream<Item = Result<StreamChunkDto>> + Send + Unpin + '_>> {
        // 1. Determine search text
        let search_texts = interpretation.search_text();
        info!(
            "RAG stream: search strategy {:?}, {} search text(s)",
            interpretation.search_strategy,
            search_texts.len()
        );

        // 2. Generate embeddings for the primary search text
        let search_text = search_texts.first().ok_or_else(|| {
            error!("RAG stream: No search text available from HyDE interpretation");
            crate::shared::error::AppError::Other("No search text available".to_string())
        })?;
        info!(
            "RAG stream: Embedding search text ({} chars)",
            search_text.len()
        );
        let query_embedding = self
            .embedding_service
            .embed_single(search_text)
            .await
            .map_err(|e| {
                error!(error = %e, "RAG stream: Embedding generation failed");
                e
            })?;

        // 3. Retrieve relevant context (top-K chunks)
        let limit = request.context_limit.unwrap_or(5);
        let port_dtos = self
            .vector_search
            .search(&query_embedding, limit, 0.5)
            .map_err(|e| {
                error!(error = %e, "RAG stream: Vector search failed");
                e
            })?;

        // 4. Map to domain entities
        let search_results = SearchMapper::port_dtos_to_domain(port_dtos);
        info!(
            "RAG stream: Retrieved {} search results (limit={})",
            search_results.len(),
            limit
        );

        // 5. Expand to full document context with relevance annotations
        let (context, matched_chunk_ids) = self.expand_to_full_documents(&search_results).await;
        info!(
            "RAG stream: Expanded to {} document context blocks from {} matched chunks",
            context.len(),
            matched_chunk_ids.len()
        );

        let context = self.apply_token_budget(&context, &request.question);

        // 6. Build sources (parallel document lookups)
        let sources = self
            .build_sources(&search_results, &request.question)
            .await?;

        // 7. Generate stream
        if context.is_empty() {
            let no_context_prompt = self.build_no_context_prompt(&request.question);

            info!("RAG stream: No context found, generating 'no documents' response");
            let llm_stream = self
                .llm
                .generate_streaming(&no_context_prompt, &[], request.images.clone())
                .await
                .map_err(|e| {
                    error!(error = %e, "RAG stream: LLM streaming failed (no-context path)");
                    e
                })?;

            let mapped = stream! {
                yield Ok(StreamChunkDto::Sources { sources: vec![] });
                for await chunk_res in llm_stream {
                    match chunk_res {
                        Ok(content) => yield Ok(StreamChunkDto::Token { content }),
                        Err(e) => {
                            error!(error = %e, "RAG stream: Error during LLM token streaming (no-context)");
                            yield Err(e);
                        }
                    }
                }
                yield Ok(StreamChunkDto::Done);
            };

            let boxed: Box<dyn Stream<Item = Result<StreamChunkDto>> + Send + Unpin> =
                Box::new(Box::pin(mapped));
            return Ok(boxed);
        }

        // Build prompt (context is embedded in prompt text only — not duplicated via system message)
        let prompt = self.build_prompt(&request.question, &context);
        info!(
            "RAG stream: Sending prompt ({} chars) with {} context blocks to LLM",
            prompt.len(),
            context.len()
        );

        // Pass empty context slice: the prompt already contains all context.
        // This avoids the double-context bug where context appeared in both
        // the prompt text AND the LLM's system message.
        let llm_stream = self
            .llm
            .generate_streaming(&prompt, &[], request.images.clone())
            .await
            .map_err(|e| {
                error!(error = %e, "RAG stream: LLM streaming failed (with-context path)");
                e
            })?;

        let mapped = stream! {
            yield Ok(StreamChunkDto::Sources { sources });
            for await chunk_res in llm_stream {
                match chunk_res {
                    Ok(content) => yield Ok(StreamChunkDto::Token { content }),
                    Err(e) => {
                        error!(error = %e, "RAG stream: Error during LLM token streaming");
                        yield Err(e);
                    }
                }
            }
            info!("RAG stream: Streaming complete");
            yield Ok(StreamChunkDto::Done);
        };

        let boxed: Box<dyn Stream<Item = Result<StreamChunkDto>> + Send + Unpin> =
            Box::new(Box::pin(mapped));
        Ok(boxed)
    }

    /// Execute RAG pipeline (non-streaming).
    async fn execute_rag(
        &self,
        request: &QARequestDto,
        interpretation: &HyDEInterpretation,
    ) -> Result<(String, Vec<SourceDto>)> {
        let search_texts = interpretation.search_text();
        info!(
            "RAG: search strategy {:?} yielded {} text(s)",
            interpretation.search_strategy,
            search_texts.len()
        );

        let search_text = search_texts.first().ok_or_else(|| {
            error!("RAG: No search text available from HyDE interpretation");
            crate::shared::error::AppError::Other("No search text available".to_string())
        })?;

        info!("RAG: Embedding search text ({} chars)", search_text.len());
        let query_embedding = self
            .embedding_service
            .embed_single(search_text)
            .await
            .map_err(|e| {
                error!(error = %e, "RAG: Embedding generation failed");
                e
            })?;

        let limit = request.context_limit.unwrap_or(5);
        info!("RAG: Searching for top {} results", limit);
        let port_dtos = self
            .vector_search
            .search(&query_embedding, limit, 0.5)
            .map_err(|e| {
                error!(error = %e, "RAG: Vector search failed");
                e
            })?;

        let search_results = SearchMapper::port_dtos_to_domain(port_dtos);
        info!("RAG: Retrieved {} search results", search_results.len());

        // Expand to full document context with relevance annotations
        let (context, matched_chunk_ids) = self.expand_to_full_documents(&search_results).await;
        info!(
            "RAG: Expanded to {} document context blocks from {} matched chunks",
            context.len(),
            matched_chunk_ids.len()
        );

        let context = self.apply_token_budget(&context, &request.question);
        let sources = self
            .build_sources(&search_results, &request.question)
            .await?;

        if context.is_empty() {
            warn!("RAG: No relevant context found for question");
            let no_context_prompt = self.build_no_context_prompt(&request.question);
            // Pass empty context — prompt is self-contained
            let answer = self
                .llm
                .generate(&no_context_prompt, &[], request.images.clone())
                .await
                .map_err(|e| {
                    error!(error = %e, "RAG: LLM generation failed (no-context path)");
                    e
                })?;
            return Ok((answer, Vec::new()));
        }

        info!(
            "RAG: Generating answer with {} context blocks",
            context.len()
        );
        let prompt = self.build_prompt(&request.question, &context);

        // Pass empty context — prompt already embeds all context.
        let answer = self
            .llm
            .generate(&prompt, &[], request.images.clone())
            .await
            .map_err(|e| {
                error!(error = %e, "RAG: LLM generation failed (with-context path)");
                e
            })?;

        info!("RAG: Answer generated ({} chars)", answer.len());
        Ok((answer, sources))
    }

    // ====================================================================
    // Prompt construction
    // ====================================================================

    /// Build LLM prompt with document context and relevance markers.
    ///
    /// Each context block is either:
    /// - A full small document with `>>> ... <<<` around matched sections
    /// - A snapshot excerpt from a large document (prefixed with metadata),
    ///   with `[...]` indicating omitted sections and `>>> ... <<<` on matches
    ///
    /// The LLM is instructed to use the full available context, focus on the
    /// highlighted sections, and note when only a partial excerpt was provided.
    fn build_prompt(&self, question: &str, context: &[String]) -> String {
        let context_text = context
            .iter()
            .enumerate()
            .map(|(i, c)| format!("[{}] {}", i + 1, c))
            .collect::<Vec<_>>()
            .join("\n\n");

        format!(
            "Answer the following question based on the provided document context. \
             Each numbered source is a document or excerpt. Sections marked with >>> are the \
             most relevant passages to the question. Use the full surrounding context for a \
             complete answer. Some sources may be partial excerpts from large documents — \
             if so, this is noted at the top of the source along with the document name; \
             let the user know the full document is available for review. \
             If the context doesn't contain enough information, say so honestly. \
             Reference source numbers in brackets like [1].\n\n\
             Context:\n{}\n\n\
             Question: {}\n\n\
             Answer:",
            context_text, question
        )
    }

    /// Build the prompt used when no context was found.
    fn build_no_context_prompt(&self, question: &str) -> String {
        format!(
            "The user asked: \"{}\"\n\n\
             No relevant documents were found in the knowledge base.\n\
             Politely let them know you don't have specific information about this topic in their documents.\n\n\
             Response:",
            question
        )
    }

    /// Generate a friendly greeting response without context.
    async fn generate_greeting_response(&self, query: &str) -> Result<String> {
        let greeting_prompt = format!(
            "You are a helpful AI assistant. The user greeted you with: \"{}\"\n\n\
             Respond in a friendly, conversational way. Keep it brief (1-2 sentences).\n\n\
             Response:",
            query
        );

        debug!("Generating greeting with prompt: {}", greeting_prompt);
        self.llm.generate(&greeting_prompt, &[], None).await
    }

    // ====================================================================
    // Full-document context expansion
    // ====================================================================

    /// Expand search results to document context with relevance markers.
    ///
    /// Uses a two-tier strategy based on document size:
    ///
    /// **Small documents** (≤ `FULL_DOCUMENT_CHUNK_THRESHOLD` chunks):
    /// Include ALL chunks in document order. Matched chunks are wrapped with
    /// `>>> ... <<<` markers so the LLM sees the full document with highlights.
    ///
    /// **Large documents** (> threshold, e.g. a 30-page PDF):
    /// Creates a **snapshot window** around each matched chunk: the matched chunk
    /// plus `SNAPSHOT_WINDOW_RADIUS` chunks before and after it. Multiple matched
    /// chunks in the same document have their windows merged if they overlap.
    /// The context block is prefixed with a note that this is a partial excerpt
    /// and includes the document file path so the LLM can reference the full source.
    ///
    /// Returns a tuple of:
    /// - `Vec<String>`: One context block per document (full or snapshot)
    /// - `Vec<String>`: The chunk IDs that were direct vector search hits
    async fn expand_to_full_documents(
        &self,
        search_results: &[SearchResult],
    ) -> (Vec<String>, Vec<String>) {
        use std::collections::{BTreeSet, HashMap, HashSet};

        if search_results.is_empty() {
            return (Vec::new(), Vec::new());
        }

        // Collect matched chunk IDs and group by document_id
        let mut matched_chunk_ids: HashSet<String> = HashSet::new();
        let mut doc_best_score: HashMap<String, f32> = HashMap::new();

        for result in search_results {
            matched_chunk_ids.insert(result.id().to_string());

            let doc_id = result.document_id().unwrap_or(result.id()).to_string();

            let entry = doc_best_score.entry(doc_id).or_insert(0.0);
            if result.score() > *entry {
                *entry = result.score();
            }
        }

        // Sort document IDs by best match score (highest first)
        let mut doc_ids: Vec<String> = doc_best_score.keys().cloned().collect();
        doc_ids.sort_by(|a, b| {
            let score_a = doc_best_score.get(a).copied().unwrap_or(0.0);
            let score_b = doc_best_score.get(b).copied().unwrap_or(0.0);
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Fetch all chunks for each document in parallel
        let chunk_futures: Vec<_> = doc_ids
            .iter()
            .map(|doc_id| self.chunk_repo.find_by_document(doc_id))
            .collect();
        let chunk_results = futures::future::join_all(chunk_futures).await;

        // Fetch document metadata in parallel (for file paths in snapshot headers)
        let doc_futures: Vec<_> = doc_ids
            .iter()
            .map(|doc_id| self.document_repo.find_by_id(doc_id))
            .collect();
        let doc_metadata_results = futures::future::join_all(doc_futures).await;

        // Build doc_id → file_name lookup
        let mut doc_file_names: HashMap<String, String> = HashMap::new();
        for (doc_id, result) in doc_ids.iter().zip(doc_metadata_results.into_iter()) {
            if let Ok(Some(doc)) = result {
                doc_file_names.insert(doc_id.clone(), doc.file_name().to_string());
            }
        }

        let mut context_blocks = Vec::new();

        for (doc_id, chunk_result) in doc_ids.iter().zip(chunk_results.into_iter()) {
            match chunk_result {
                Ok(mut chunks) => {
                    if chunks.is_empty() {
                        Self::push_fallback_snippets(&mut context_blocks, search_results, doc_id);
                        continue;
                    }

                    // Sort chunks by index (document order)
                    chunks.sort_by_key(|c| c.index());
                    let total_chunks = chunks.len();

                    if total_chunks <= FULL_DOCUMENT_CHUNK_THRESHOLD {
                        // ── Small document: include everything ──
                        let doc_text = Self::build_full_document_text(&chunks, &matched_chunk_ids);
                        if !doc_text.trim().is_empty() {
                            context_blocks.push(doc_text);
                        }
                    } else {
                        // ── Large document: snapshot window ──
                        let file_name = doc_file_names
                            .get(doc_id)
                            .cloned()
                            .unwrap_or_else(|| format!("document {}", doc_id));

                        let snapshot = Self::build_snapshot_text(
                            &chunks,
                            &matched_chunk_ids,
                            &file_name,
                            total_chunks,
                        );
                        if !snapshot.trim().is_empty() {
                            context_blocks.push(snapshot);
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        doc_id = %doc_id,
                        error = %e,
                        "Failed to fetch sibling chunks, falling back to matched snippets"
                    );
                    Self::push_fallback_snippets(&mut context_blocks, search_results, doc_id);
                }
            }
        }

        let matched_ids: Vec<String> = matched_chunk_ids.into_iter().collect();
        (context_blocks, matched_ids)
    }

    /// Build full document text with `>>>` / `<<<` markers on matched chunks.
    fn build_full_document_text(
        chunks: &[crate::domain::entities::chunk::Chunk],
        matched_chunk_ids: &std::collections::HashSet<String>,
    ) -> String {
        let mut doc_text = String::new();
        for chunk in chunks {
            let chunk_id = chunk.id().to_string();
            let content = chunk.content();

            if !doc_text.is_empty() {
                doc_text.push(' ');
            }

            if matched_chunk_ids.contains(&chunk_id) {
                doc_text.push_str(">>> ");
                doc_text.push_str(content);
                doc_text.push_str(" <<<");
            } else {
                doc_text.push_str(content);
            }
        }
        doc_text
    }

    /// Build a snapshot of a large document by windowing around matched chunks.
    ///
    /// For each matched chunk, includes `SNAPSHOT_WINDOW_RADIUS` chunks on each
    /// side. Overlapping windows are merged into contiguous regions separated by
    /// `[...]` to indicate omitted content.
    fn build_snapshot_text(
        chunks: &[crate::domain::entities::chunk::Chunk],
        matched_chunk_ids: &std::collections::HashSet<String>,
        file_name: &str,
        total_chunks: usize,
    ) -> String {
        use std::collections::BTreeSet;

        // Find the array indices of matched chunks
        let matched_indices: Vec<usize> = chunks
            .iter()
            .enumerate()
            .filter(|(_, c)| matched_chunk_ids.contains(&c.id().to_string()))
            .map(|(i, _)| i)
            .collect();

        if matched_indices.is_empty() {
            return String::new();
        }

        // Build the set of indices to include (windows merged via BTreeSet)
        let mut include_indices: BTreeSet<usize> = BTreeSet::new();
        for &idx in &matched_indices {
            let start = idx.saturating_sub(SNAPSHOT_WINDOW_RADIUS);
            let end = std::cmp::min(idx + SNAPSHOT_WINDOW_RADIUS, chunks.len().saturating_sub(1));
            for i in start..=end {
                include_indices.insert(i);
            }
        }

        // Build snapshot text, inserting [...] gaps between non-contiguous regions
        let mut snapshot = format!(
            "(Excerpt from \"{}\", showing {} of {} sections)\n",
            file_name,
            include_indices.len(),
            total_chunks,
        );

        let mut prev_idx: Option<usize> = None;
        for &idx in &include_indices {
            // Insert gap marker if there's a discontinuity
            if let Some(prev) = prev_idx {
                if idx > prev + 1 {
                    snapshot.push_str("\n[...]\n");
                }
            }

            let chunk = &chunks[idx];
            let content = chunk.content();

            if !snapshot.ends_with('\n') {
                snapshot.push(' ');
            }

            if matched_chunk_ids.contains(&chunk.id().to_string()) {
                snapshot.push_str(">>> ");
                snapshot.push_str(content);
                snapshot.push_str(" <<<");
            } else {
                snapshot.push_str(content);
            }

            prev_idx = Some(idx);
        }

        snapshot
    }

    /// Fallback: push original search result snippets when chunk fetch fails.
    fn push_fallback_snippets(
        context_blocks: &mut Vec<String>,
        search_results: &[SearchResult],
        doc_id: &str,
    ) {
        for result in search_results {
            let result_doc_id = result.document_id().unwrap_or(result.id());
            if result_doc_id == doc_id {
                if let Some(snippet) = result.snippet() {
                    context_blocks.push(format!(">>> {} <<<", snippet));
                }
            }
        }
    }

    // ====================================================================
    // Context window budget
    // ====================================================================

    /// Apply token-budget-aware truncation to context chunks.
    ///
    /// Keeps chunks (highest-score first, which is the order from vector search)
    /// until the token budget is exhausted. This prevents prompt overflow and
    /// ensures the LLM has room for its response.
    fn apply_token_budget(&self, context: &[String], question: &str) -> Vec<String> {
        let max_tokens = self.llm.max_context_tokens();
        let response_budget = (max_tokens as f64 * RESPONSE_TOKEN_BUDGET_RATIO) as usize;
        let question_tokens = self.llm.count_tokens(question);
        let available = max_tokens
            .saturating_sub(response_budget)
            .saturating_sub(PROMPT_OVERHEAD_TOKENS)
            .saturating_sub(question_tokens);

        let mut used = 0usize;
        let mut kept = Vec::new();

        for chunk in context {
            let chunk_tokens = self.llm.count_tokens(chunk);
            if used + chunk_tokens > available {
                debug!(
                    "Token budget exhausted: {}/{} tokens used, dropping remaining {} chunks",
                    used,
                    available,
                    context.len() - kept.len()
                );
                break;
            }
            used += chunk_tokens;
            kept.push(chunk.clone());
        }

        if kept.len() < context.len() {
            info!(
                "Context truncated: kept {}/{} chunks ({}/{} tokens)",
                kept.len(),
                context.len(),
                used,
                available
            );
        }

        kept
    }

    // ====================================================================
    // Source building (parallel document lookups)
    // ====================================================================

    /// Build source citations from search results.
    ///
    /// Uses parallel document lookups to avoid N+1 sequential queries.
    async fn build_sources(
        &self,
        search_results: &[SearchResult],
        query: &str,
    ) -> Result<Vec<SourceDto>> {
        let highlight_terms = extract_highlight_terms(query, 8);
        // Collect unique document IDs and fetch in parallel
        let doc_ids: Vec<&str> = search_results
            .iter()
            .map(|r| r.document_id().unwrap_or(r.id()))
            .filter(|id| !id.is_empty())
            .collect();

        // Parallel document lookups
        let doc_futures: Vec<_> = doc_ids
            .iter()
            .map(|id| self.document_repo.find_by_id(id))
            .collect();

        let doc_results = futures::future::join_all(doc_futures).await;

        // Build lookup map: doc_id → Document
        let mut doc_map = std::collections::HashMap::new();
        for (id, result) in doc_ids.iter().zip(doc_results.into_iter()) {
            match result {
                Ok(Some(doc)) => {
                    doc_map.insert(*id, doc);
                }
                Ok(None) => {
                    debug!("Document not found for id: {}", id);
                }
                Err(e) => {
                    warn!(error = %e, doc_id = %id, "Failed to fetch document metadata");
                }
            }
        }

        // Batch chunk metadata lookups
        let chunk_ids: Vec<String> = search_results.iter().map(|r| r.id().to_string()).collect();
        let mut chunk_map = std::collections::HashMap::new();
        match self.chunk_repo.find_by_ids(&chunk_ids).await {
            Ok(chunks) => {
                for chunk in chunks {
                    chunk_map.insert(chunk.id().as_str().to_string(), chunk);
                }
            }
            Err(e) => {
                warn!(error = %e, "Failed to fetch chunk metadata batch");
            }
        }

        // Build SourceDto for each result
        let mut sources = Vec::with_capacity(search_results.len());
        for result in search_results {
            let doc_id = result.document_id().unwrap_or(result.id());
            let document = doc_map.get(doc_id);
            let fallback_path = result.file_path().unwrap_or("");
            let fallback_file_name = std::path::Path::new(fallback_path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("");

            let (file_path, file_name, mime_type, file_size_bytes, modified_at) =
                if let Some(doc) = document {
                    (
                        validate_file_path(doc.file_path(), doc_id),
                        validate_file_name(doc.file_name(), doc_id),
                        doc.mime_type().to_string(),
                        validate_file_size(doc.size_bytes(), doc_id),
                        doc.modified_at().to_rfc3339(),
                    )
                } else {
                    (
                        validate_file_path(std::path::Path::new(fallback_path), doc_id),
                        validate_file_name(fallback_file_name, doc_id),
                        String::new(),
                        0,
                        String::new(),
                    )
                };

            let path = if file_path.is_empty() {
                None
            } else {
                Some(file_path.clone())
            };

            let chunk_meta = chunk_map.get(result.id());
            let section = chunk_meta.and_then(|chunk| chunk.section().map(|s| s.to_string()));
            let chunk_index = chunk_meta.map(|chunk| chunk.index());

            let content = result.snippet().unwrap_or("").to_string();
            let excerpt = if content.trim().is_empty() {
                None
            } else {
                Some(build_excerpt(&content, &highlight_terms, 480))
            };

            let highlights = if highlight_terms.is_empty() {
                None
            } else {
                Some(highlight_terms.clone())
            };

            sources.push(SourceDto {
                document_id: doc_id.to_string(),
                chunk_id: result.id().to_string(),
                content,
                score: result.score(),
                path,
                position: None,
                file_name,
                file_path: file_path.clone(),
                mime_type,
                category: infer_category(&file_path),
                file_size_bytes,
                modified_at,
                excerpt,
                highlights,
                section,
                chunk_index,
                chunk_excerpts: None,
            });
        }
        Ok(sources)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_file_name_empty() {
        let result = validate_file_name("", "test-doc");
        assert!(result.starts_with("document_"));
    }

    #[test]
    fn test_validate_file_name_normal() {
        let result = validate_file_name("report.pdf", "test-doc");
        assert_eq!(result, "report.pdf");
    }

    #[test]
    fn test_validate_file_size_negative() {
        let result = validate_file_size(-1, "test-doc");
        assert_eq!(result, 0);
    }

    #[test]
    fn test_validate_file_size_normal() {
        let result = validate_file_size(1024, "test-doc");
        assert_eq!(result, 1024);
    }

    #[test]
    fn test_validate_file_path_empty() {
        let result = validate_file_path(std::path::Path::new(""), "test-doc");
        assert_eq!(result, "[unknown path]");
    }
}
