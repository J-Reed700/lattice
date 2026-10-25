//! Loading documents: by id, path, pattern, checksum or filter, as metadata
//! or as whole aggregates with their chunks and tags.

use super::hydrate::hydrate_many;
use super::parts::{fetch_chunks_for_document, fetch_tags_for_document};
use crate::infrastructure::persistence::mappers::{DocumentMapper, DocumentModel};
use lattice_core::domain::entities::Document as DocumentEntity;
use lattice_core::shared::error::{AppError, Result};
use sqlx::SqliteConnection;

pub async fn find_by_id(conn: &mut SqliteConnection, id: &str) -> Result<Option<DocumentEntity>> {
    let db_model = sqlx::query_as::<_, DocumentModel>(
        r#"
        SELECT
            id,
            file_path,
            file_name,
            file_type,
            mime_type,
            size_bytes,
            modified_at,
            indexed_at,
            checksum,
            status,
            error_message,
            language,
            category,
            quality_score,
            access_count,
            last_accessed_at,
            word_count, source_context
        FROM documents
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| {
        AppError::Database(format!(
            "Failed to find document with id '{}'. Document may not exist or database may be corrupted. Error: {}",
            id, e
        ))
    })?;

    match db_model {
        Some(model) => Ok(Some(
            hydrate(conn, DocumentMapper::to_entity(&model)?).await?,
        )),
        None => Ok(None),
    }
}

pub async fn find_by_path(
    conn: &mut SqliteConnection,
    file_path: &str,
) -> Result<Option<DocumentEntity>> {
    let db_model = sqlx::query_as::<_, DocumentModel>(
        r#"
        SELECT
            id,
            file_path,
            file_name,
            file_type,
            mime_type,
            size_bytes,
            modified_at,
            indexed_at,
            checksum,
            status,
            error_message,
            language,
            category,
            quality_score,
            access_count,
            last_accessed_at,
            word_count, source_context
        FROM documents
        WHERE file_path = ?
        "#,
    )
    .bind(file_path)
    .fetch_optional(conn)
    .await
    .map_err(|e| {
        AppError::Database(format!(
            "Failed to find document at path '{}'. File may not be indexed or database may be corrupted. Error: {}",
            file_path, e
        ))
    })?;

    match db_model {
        Some(model) => Ok(Some(DocumentMapper::to_entity(&model)?)),
        None => Ok(None),
    }
}

pub async fn find_by_path_pattern(
    conn: &mut SqliteConnection,
    pattern: &str,
) -> Result<Vec<DocumentEntity>> {
    let db_models: Vec<DocumentModel> = sqlx::query_as::<_, DocumentModel>(
        r#"
        SELECT
            id,
            file_path,
            file_name,
            file_type,
            mime_type,
            size_bytes,
            modified_at,
            indexed_at,
            checksum,
            status,
            error_message,
            language,
            category,
            quality_score,
            access_count,
            last_accessed_at,
            word_count, source_context
        FROM documents
        WHERE file_path LIKE ?
        ORDER BY file_path DESC
        "#,
    )
    .bind(pattern)
    .fetch_all(conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to find documents by pattern: {}", e)))?;

    DocumentMapper::to_entities(&db_models)
}

pub async fn find_by_checksum(
    conn: &mut SqliteConnection,
    checksum: &lattice_core::domain::value_objects::Checksum,
) -> Result<Option<DocumentEntity>> {
    let db_model = sqlx::query_as::<_, DocumentModel>(
        r#"
        SELECT
            id,
            file_path,
            file_name,
            file_type,
            mime_type,
            size_bytes,
            modified_at,
            indexed_at,
            checksum,
            status,
            error_message,
            language,
            category,
            quality_score,
            access_count,
            last_accessed_at,
            word_count, source_context
        FROM documents
        WHERE checksum = ?
        "#,
    )
    .bind(checksum.as_str())
    .fetch_optional(conn)
    .await
    .map_err(|e| {
        AppError::Database(format!("Failed to find document by checksum. Error: {}", e))
    })?;

    match db_model {
        Some(model) => Ok(Some(DocumentMapper::to_entity(&model)?)),
        None => Ok(None),
    }
}

pub async fn list_metadata(conn: &mut SqliteConnection) -> Result<Vec<DocumentEntity>> {
    let db_models: Vec<DocumentModel> = sqlx::query_as::<_, DocumentModel>(
        r#"
        SELECT
            id,
            file_path,
            file_name,
            file_type,
            mime_type,
            size_bytes,
            modified_at,
            indexed_at,
            checksum,
            status,
            error_message,
            language,
            category,
            quality_score,
            access_count,
            last_accessed_at,
            word_count, source_context
        FROM documents
        ORDER BY indexed_at DESC
        "#,
    )
    .fetch_all(conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to list documents: {}", e)))?;

    DocumentMapper::to_entities(&db_models)
}

pub async fn find_all_paginated(
    conn: &mut SqliteConnection,
    limit: usize,
) -> Result<Vec<DocumentEntity>> {
    let db_models: Vec<DocumentModel> = sqlx::query_as::<_, DocumentModel>(
        r#"
        SELECT
            id,
            file_path,
            file_name,
            file_type,
            mime_type,
            size_bytes,
            modified_at,
            indexed_at,
            checksum,
            status,
            error_message,
            language,
            category,
            quality_score,
            access_count,
            last_accessed_at,
            word_count, source_context
        FROM documents
        -- See DocumentRepository::find_all_paginated: a chat's attachments are
        -- not part of the library listing.
        WHERE owner_conversation_id IS NULL
        ORDER BY indexed_at DESC
        LIMIT ?
        "#,
    )
    .bind(limit as i64)
    .fetch_all(conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to list documents (paginated): {}", e)))?;

    DocumentMapper::to_entities(&db_models)
}

pub async fn find_aggregate_by_checksum_tx(
    conn: &mut SqliteConnection,
    checksum: &str,
) -> Result<Option<DocumentEntity>> {
    let doc_model: Option<DocumentModel> = sqlx::query_as::<_, DocumentModel>(
        r#"
        SELECT id, file_path, file_name, file_type, mime_type, size_bytes, modified_at,
               indexed_at, checksum, status, error_message, language, category, quality_score,
               access_count, last_accessed_at, word_count, source_context
        FROM documents
        WHERE checksum = ?
        LIMIT 1
        "#,
    )
    .bind(checksum)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to find document by checksum: {}", e)))?;

    match doc_model {
        Some(model) => {
            let doc_entity = DocumentMapper::to_entity(&model)?;
            let chunks = fetch_chunks_for_document(conn, doc_entity.id().as_str()).await?;
            let tags = fetch_tags_for_document(conn, doc_entity.id().as_str()).await?;
            let aggregate = doc_entity.from_parts(chunks, tags)?;
            Ok(Some(aggregate))
        }
        None => Ok(None),
    }
}

async fn hydrate(conn: &mut SqliteConnection, document: DocumentEntity) -> Result<DocumentEntity> {
    let chunks = fetch_chunks_for_document(conn, document.id().as_str()).await?;
    let tags = fetch_tags_for_document(conn, document.id().as_str()).await?;
    document.from_parts(chunks, tags)
}

pub async fn find_all(conn: &mut SqliteConnection) -> Result<Vec<DocumentEntity>> {
    let metadata = list_metadata(conn).await?;
    hydrate_many(conn, metadata).await
}

pub async fn find_by_filter(
    conn: &mut SqliteConnection,
    filter: &lattice_core::application::ports::DocumentFilter,
) -> Result<Vec<DocumentEntity>> {
    use lattice_core::application::ports::Filter;
    filter.validate()?;
    let mut query = sqlx::QueryBuilder::new("SELECT id, file_path, file_name, file_type, mime_type, size_bytes, modified_at, indexed_at, checksum, status, error_message, language, category, quality_score, access_count, last_accessed_at, word_count, source_context FROM documents WHERE 1=1");
    if let Some(pattern) = &filter.path_pattern {
        query.push(" AND file_path LIKE ").push_bind(pattern);
    }
    if let Some(status) = &filter.status {
        query.push(" AND status = ").push_bind(status);
    }
    query
        .push(" ORDER BY indexed_at DESC LIMIT ")
        .push_bind(filter.limit.unwrap_or(1000) as i64);
    let models = query
        .build_query_as::<DocumentModel>()
        .fetch_all(&mut *conn)
        .await?;
    let mut documents = Vec::with_capacity(models.len());
    for model in models {
        documents.push(DocumentMapper::to_entity(&model)?);
    }
    hydrate_many(conn, documents).await
}

pub async fn find_metadata_by_ids(
    conn: &mut SqliteConnection,
    ids: &[String],
) -> Result<Vec<DocumentEntity>> {
    let mut documents = Vec::with_capacity(ids.len());
    for batch in ids.chunks(500) {
        let mut query = sqlx::QueryBuilder::new("SELECT id, file_path, file_name, file_type, mime_type, size_bytes, modified_at, indexed_at, checksum, status, error_message, language, category, quality_score, access_count, last_accessed_at, word_count, source_context FROM documents WHERE id IN (");
        let mut values = query.separated(", ");
        for id in batch {
            values.push_bind(id);
        }
        values.push_unseparated(")");
        let models = query
            .build_query_as::<DocumentModel>()
            .fetch_all(&mut *conn)
            .await?;
        for model in models {
            documents.push(DocumentMapper::to_entity(&model)?);
        }
    }
    Ok(documents)
}
