//! Preparing a file for import: validation, extraction, chunking and embedding, before any transaction.

use std::path::{Path, PathBuf};

use tracing::instrument;

use crate::application::factories::{ChecksumFactory, FileMetadataFactory};
use crate::application::ports::ImportedBlob;
use crate::domain::entities::Document;
use crate::domain::value_objects::SparseEmbedding;
use crate::features::indexing::dto::IndexFileRequestDto;
use crate::features::indexing::mapper::IndexingMapper;
use crate::infrastructure::services::metadata_extraction::MetadataExtractor;
use crate::shared::error::{AppError, Result};
use crate::shared::types::ValidatedFilePath;

use super::{
    Embedding, EmbeddingWithVector, IndexFileUseCase, PrepareForIndexingOutcome, PreparedDocument,
};

impl IndexFileUseCase {
    /// Prepare file for indexing (extraction + embeddings) WITHOUT opening transaction.
    /// Returns prepared aggregate and embeddings ready to be saved.
    ///
    /// This is the preparation step of the batch operation.
    /// Call this to prepare files, then save all at once in a single fast transaction.
    #[instrument(skip(self, request), fields(file_path = %request.path))]
    pub async fn prepare_for_indexing(
        &self,
        request: IndexFileRequestDto,
    ) -> Result<PrepareForIndexingOutcome> {
        let source_path = ValidatedFilePath::new(PathBuf::from(&request.path))?;
        self.validate_supported_file(source_path.as_path())?;
        let content_hash = self
            .file_storage
            .compute_hash(source_path.as_path())
            .await?;
        let content_checksum = crate::domain::value_objects::Checksum::new(content_hash.clone())?;

        let requested_context: Option<crate::domain::value_objects::source_context::SourceContext> =
            request
                .metadata
                .as_ref()
                .and_then(|m| m.get("source_context"))
                .map(|s| serde_json::from_str(s))
                .transpose()?;
        if let Some(context) = &requested_context {
            context.group.validate()?;
        }
        let existing = self
            .document_repo
            .find_by_checksum(&content_checksum)
            .await?;
        if let Some(existing) = &existing {
            if requested_context
                .as_ref()
                .is_none_or(|context| Some(context) == existing.source_context())
            {
                // The previous attempt may have committed SQLite and then failed
                // while publishing to runtime search. Finish that step on retry.
                self.publish_stored_document(existing.id().as_str()).await?;
                return Ok(PrepareForIndexingOutcome::Duplicate {
                    document_id: existing.id().to_string(),
                });
            }
        }

        let already_in_library = self.content_storage.exists_by_hash(&content_hash).await?;
        let ImportedBlob {
            path: library_path,
            hash: library_hash,
            lease,
        } = self
            .content_storage
            .import_file(source_path.as_path())
            .await?;
        let imported_new = !already_in_library;

        // Everything from here on can fail. When it does and this import is
        // what put the blob on disk, the blob has to go back: nothing will
        // ever reference it.
        let prepared = self
            .build_prepared_document(
                source_path.as_path(),
                library_path.as_path(),
                request.chunking_strategy,
                existing,
                requested_context,
            )
            .await;

        let (document, embeddings, sparse_terms) = match prepared {
            Ok(parts) => parts,
            Err(error) => {
                // Release the lease first, or the collector would refuse its
                // own caller's blob as still being imported.
                drop(lease);
                if imported_new {
                    self.release_failed_import(&library_hash).await;
                }
                return Err(error);
            }
        };

        Ok(PrepareForIndexingOutcome::Prepared(Box::new(
            PreparedDocument {
                document,
                embeddings,
                library_path: library_path.to_str().unwrap_or("").to_string(),
                imported_new,
                sparse_terms,
                lease,
            },
        )))
    }

    /// Extract, chunk, annotate and embed the blob that was just imported.
    ///
    /// Split out from `prepare_for_indexing` so the import's blob lease has a
    /// single, obvious failure boundary: everything in here runs after the
    /// copy, and anything it returns as an error means nothing will ever
    /// reference that blob.
    async fn build_prepared_document(
        &self,
        source_path: &Path,
        library_path: &Path,
        chunking_strategy: crate::features::indexing::dto::ChunkingStrategyDto,
        existing: Option<Document>,
        requested_context: Option<crate::domain::value_objects::source_context::SourceContext>,
    ) -> Result<(Document, Vec<EmbeddingWithVector>, Vec<SparseEmbedding>)> {
        // Heavy work (extraction + embeddings) - NO TRANSACTION
        let metadata = FileMetadataFactory::from_path(library_path)?;
        let checksum = ChecksumFactory::from_path(library_path)?;
        let extracted = self.content_extractor.extract_content(library_path).await?;
        let page_ranges = extracted.page_ranges;
        let content = extracted.text;
        self.validate_text_content(source_path, &content)?;

        let chunking_strategy = IndexingMapper::chunking_strategy_to_domain(chunking_strategy)
            .adapt_for_content(&content);
        let library_path_validated = ValidatedFilePath::new(library_path.to_path_buf())?;
        let mut document = Document::from_file(
            library_path_validated,
            metadata.clone(),
            checksum,
            content.clone(),
            chunking_strategy,
        )?;

        if let Some(mut saved) = existing {
            saved.reindex(content.clone(), chunking_strategy)?;
            document = saved;
        }
        if let Some(context) = requested_context {
            document.set_source_context(Some(context));
        }
        let metadata_extractor = MetadataExtractor::new();
        let file_ext = document
            .file_path()
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        document.set_language(metadata_extractor.detect_language(&content, file_ext));
        document.set_category(metadata_extractor.categorize(
            &content,
            metadata.file_name(),
            metadata.mime_type(),
        ));
        document.set_quality_score(metadata_extractor.calculate_quality_score(
            &content,
            metadata.size_bytes(),
            metadata.modified_at(),
        ));
        document.set_word_count(metadata_extractor.count_words(&content));

        for index in 0..document.chunks().len() {
            let chunk = document.chunks().get(index).ok_or_else(|| {
                AppError::InvalidInput(format!(
                    "Chunk index {} out of bounds in index_file_and_persist",
                    index
                ))
            })?;
            let chunk_content = chunk.content().to_string();
            document.set_chunk_token_count(index, metadata_extractor.count_tokens(chunk.content()));
            document.set_chunk_word_count(
                index,
                metadata_extractor.count_words(&chunk_content) as usize,
            );
            document.set_chunk_has_code(index, metadata_extractor.contains_code(&chunk_content));
            document.set_chunk_section(index, metadata_extractor.extract_section(&chunk_content));
        }

        let (document, spans) = super::super::embedding_input::prepare_structured_with_spans(
            document,
            self.embedding_service.as_ref(),
            &page_ranges,
        )?;
        let model_identity = self.embedding_service.model_identity();
        // With late chunking each structure span is embedded once and pooled
        // per chunk; otherwise this is the per-chunk batch as before. Either
        // way it yields one vector per chunk, in chunk order.
        //
        // When the model has a sparse head and is *not* late chunking, both
        // representations come from one forward pass over exactly the texts
        // `embed_prepared_chunks` would have used, so the dense vectors are
        // unchanged. Late chunking pools dense vectors over a whole span and
        // has no equivalent pooling for term weights, so there the sparse head
        // is read from the per-chunk texts in a second pass instead.
        let (embeddings, sparse_terms) = if self.sparse_indexing_enabled() {
            let chunk_texts: Vec<String> = document
                .chunks()
                .iter()
                .map(|chunk| chunk.embedding_text())
                .collect();
            if self.embedding_service.uses_late_chunking() {
                let dense = super::super::embedding_input::embed_prepared_chunks(
                    &document,
                    self.embedding_service.as_ref(),
                    &spans,
                )
                .await?;
                (
                    dense,
                    self.embedding_service
                        .embed_sparse_batch(&chunk_texts)
                        .await?,
                )
            } else {
                self.embedding_service
                    .embed_batch_with_sparse(&chunk_texts)
                    .await?
            }
        } else {
            (
                super::super::embedding_input::embed_prepared_chunks(
                    &document,
                    self.embedding_service.as_ref(),
                    &spans,
                )
                .await?,
                Vec::new(),
            )
        };
        if self.embedding_service.model_identity() != model_identity
            || embeddings.len() != document.chunks().len()
        {
            return Err(AppError::InvalidState(
                "Embedding model changed or returned an incomplete batch; retry indexing".into(),
            ));
        }
        // A short sparse batch would silently key one chunk's terms to another
        // chunk's id, which is worse than having no sparse rows at all.
        let sparse_terms = if sparse_terms.len() == document.chunks().len() {
            sparse_terms
        } else {
            if !sparse_terms.is_empty() {
                tracing::warn!(
                    expected = document.chunks().len(),
                    received = sparse_terms.len(),
                    "Sparse head returned an incomplete batch; indexing this document without sparse terms"
                );
            }
            Vec::new()
        };

        // Prepare embedding entries
        let embedding_entries: Vec<(Embedding, Vec<f32>)> = document
            .chunks()
            .iter()
            .zip(embeddings.iter())
            .map(|(chunk, vec)| {
                let embedding =
                    Embedding::new(chunk.id().clone(), model_identity.clone(), vec.len());
                (embedding, vec.clone())
            })
            .collect();

        Ok((document, embedding_entries, sparse_terms))
    }

    fn validate_supported_file(&self, path: &Path) -> Result<()> {
        if self.content_extractor.is_supported(path) {
            return Ok(());
        }

        let detected_type = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("unknown")
            .to_string();

        Err(AppError::UnsupportedFileType {
            path: path.display().to_string(),
            detected_type,
        })
    }

    fn validate_text_content(&self, path: &Path, content: &str) -> Result<()> {
        if !content.trim().is_empty() {
            return Ok(());
        }

        Err(AppError::InvalidInput(format!(
            "File contains no extractable text: {}",
            path.display()
        )))
    }
}
