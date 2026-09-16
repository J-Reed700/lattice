# Function Calling Implementation Guide

> **Note (2026-09-16):** The Python/FastAPI backend (`src/api`) has been removed. The Python sections below are kept for historical context only; the Rust desktop implementation is the one that ships.

This guide shows how to implement the 5 function calling tools in both **Python (Backend)** and **Rust (Desktop)**.

**Architecture**: Local-first with Ollama for privacy-preserving AI. All processing happens on-device except optional web search.

## Table of Contents

1. [Backend Implementation (Python/FastAPI)](#backend-implementation-pythonfastapi)
2. [Desktop Implementation (Rust/Tauri)](#desktop-implementation-rusttauri)
3. [Frontend Integration (TypeScript/React)](#frontend-integration-typescriptreact)
4. [Testing Examples](#testing-examples)

---

## Backend Implementation (Python/FastAPI)

### 1. Service Layer

Create service in `/src/api/src/services/function_calling_service.py`:

```python
"""Service for LLM function calling tools.

Local-first implementation - all processing happens on-device.
Uses local embeddings for semantic search, local storage for documents.
No cloud API calls except optional web search.
"""

from __future__ import annotations

from datetime import datetime
from typing import TYPE_CHECKING

from src.schemas.function_calling import (
    DocumentResult,
    GetDocumentInput,
    GetDocumentOutput,
    ListDocumentsInput,
    ListDocumentsOutput,
    SemanticSearchInput,
    SemanticSearchOutput,
)
from src.services.search.service import SearchService

if TYPE_CHECKING:
    from sqlalchemy.ext.asyncio import AsyncSession


class FunctionCallingService:
    """Service for LLM function calling tools.

    All operations are local-first:
    - Semantic search uses local embeddings
    - Documents retrieved from local storage
    - No data sent to cloud except optional web search
    """

    def __init__(self, db: AsyncSession):
        self.db = db
        self.search_service = SearchService(db)  # Local search service

    async def semantic_search(
        self, input: SemanticSearchInput
    ) -> SemanticSearchOutput:
        """Execute semantic search on vault documents (100% local).

        All processing happens locally:
        - Embeddings generated on-device
        - Vector search performed locally (FAISS)
        - No data sent to cloud

        Args:
            input: Search parameters

        Returns:
            Search results with metadata

        Raises:
            HTTPException: If search fails
        """
        # Call existing search service (local only)
        start_time = datetime.utcnow()
        
        results = await self.search_service.search(
            query=input.query,
            search_type=input.search_mode,
            top_k=input.limit,
            min_score=input.threshold,
            file_types=input.file_types,
            date_from=input.date_from,
            date_to=input.date_to,
        )
        
        end_time = datetime.utcnow()
        execution_time = (end_time - start_time).total_seconds() * 1000
        
        # Map to function calling schema
        doc_results = [
            DocumentResult(
                document_id=str(r.file_id),
                filename=r.filename,
                file_path=r.file_metadata.get("file_path", ""),
                mime_type=r.mime_type,
                score=r.score,
                snippet=r.snippet,
                chunk_index=r.chunk_index,
                modified_at=r.matched_at,
                size_bytes=r.file_metadata.get("size_bytes", 0),
            )
            for r in results.results
        ]
        
        return SemanticSearchOutput(
            results=doc_results,
            total_found=results.total_results,
            search_time_ms=execution_time,
            query=input.query,
        )

    async def get_document(
        self, input: GetDocumentInput
    ) -> GetDocumentOutput:
        """Retrieve full document content by ID.

        Args:
            input: Document retrieval parameters

        Returns:
            Document content and metadata

        Raises:
            HTTPException: If document not found
        """
        # Fetch document from database
        doc = await self._fetch_document(input.document_id)
        
        # Read file content
        content = await self._read_file_content(
            doc.file_path,
            max_length=input.max_content_length
        )
        
        # Build metadata if requested
        metadata = None
        if input.include_metadata:
            metadata = await self._build_metadata(doc)
        
        return GetDocumentOutput(
            document_id=input.document_id,
            content=content.text,
            content_truncated=content.truncated,
            metadata=metadata,
        )

    async def list_documents(
        self, input: ListDocumentsInput
    ) -> ListDocumentsOutput:
        """List and filter documents in vault.

        Args:
            input: Filtering and pagination parameters

        Returns:
            List of documents with metadata
        """
        # Build query based on filter_mode
        query = self._build_list_query(input)
        
        # Execute query
        result = await self.db.execute(query)
        documents = result.scalars().all()
        
        # Count total
        total = await self._count_documents(input)
        
        # Map to response
        doc_items = [self._map_to_list_item(doc) for doc in documents]
        
        return ListDocumentsOutput(
            documents=doc_items,
            total=total,
            limit=input.limit,
            offset=input.offset,
            has_more=(input.offset + len(doc_items)) < total,
        )
```

### 2. API Routes

Create routes in `/src/api/src/api/v1/function_calling.py`:

```python
"""API routes for LLM function calling."""

from __future__ import annotations

from fastapi import APIRouter, Depends, HTTPException, status
from sqlalchemy.ext.asyncio import AsyncSession

from src.api.deps import get_db, rate_limit
from src.schemas.function_calling import (
    GetDocumentInput,
    GetDocumentOutput,
    ListDocumentsInput,
    ListDocumentsOutput,
    SemanticSearchInput,
    SemanticSearchOutput,
)
from src.services.function_calling_service import FunctionCallingService

router = APIRouter(prefix="/function-calling", tags=["function-calling"])


@router.post("/semantic-search", response_model=SemanticSearchOutput)
@rate_limit(limit=100, window=60)  # 100 requests/minute
async def semantic_search(
    input: SemanticSearchInput,
    db: AsyncSession = Depends(get_db),
) -> SemanticSearchOutput:
    """Search vault documents using semantic similarity (100% local).

    This endpoint is designed for LLM function calling with Ollama.
    All processing is local - embeddings generated on-device.
    Rate limit: 100 requests/minute.
    """
    service = FunctionCallingService(db)
    return await service.semantic_search(input)  # Local processing only


@router.post("/get-document", response_model=GetDocumentOutput)
@rate_limit(limit=200, window=60)  # 200 requests/minute
async def get_document(
    input: GetDocumentInput,
    db: AsyncSession = Depends(get_db),
) -> GetDocumentOutput:
    """Retrieve full document content by ID.

    This endpoint is designed for LLM function calling.
    Rate limit: 200 requests/minute.
    """
    service = FunctionCallingService(db)
    
    try:
        return await service.get_document(input)
    except FileNotFoundError:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail={
                "error_code": "DOCUMENT_NOT_FOUND",
                "message": f"Document {input.document_id} not found",
            },
        )


@router.post("/list-documents", response_model=ListDocumentsOutput)
@rate_limit(limit=200, window=60)  # 200 requests/minute
async def list_documents(
    input: ListDocumentsInput,
    db: AsyncSession = Depends(get_db),
) -> ListDocumentsOutput:
    """List and filter documents in vault.

    This endpoint is designed for LLM function calling.
    Rate limit: 200 requests/minute.
    """
    service = FunctionCallingService(db)
    return await service.list_documents(input)
```

### 3. Register Routes

Add to `/src/api/src/api/v1/__init__.py`:

```python
from src.api.v1 import function_calling

# In create_api_router()
api_router.include_router(function_calling.router)
```

---

## Desktop Implementation (Rust/Tauri)

### 1. Service Layer

Create service in `/src-tauri/src/services/function_calling_service.rs`:

```rust
//! Function calling service for LLM tools.
//!
//! Local-first implementation for privacy-preserving AI:
//! - All search operations use local embeddings (ONNX Runtime)
//! - Documents stored and retrieved locally (SQLite)
//! - No cloud API calls except optional web search

use crate::application::dtos::function_calling_dto::*;
use crate::domain::document::DocumentAggregate;
use crate::shared::error::{AppError, Result};
use crate::services::traits::{SearchServiceTrait, DocumentServiceTrait};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Instant;

/// Trait for function calling service.
///
/// All operations are local-first - no data leaves the device.
#[async_trait]
pub trait FunctionCallingServiceTrait: Send + Sync {
    async fn semantic_search(&self, input: SemanticSearchInput) -> Result<SemanticSearchOutput>;
    async fn get_document(&self, input: GetDocumentInput) -> Result<GetDocumentOutput>;
    async fn list_documents(&self, input: ListDocumentsInput) -> Result<ListDocumentsOutput>;
}

/// Function calling service implementation.
///
/// Privacy-first architecture:
/// - Local embeddings via ONNX Runtime
/// - Local vector search
/// - Local document storage
pub struct FunctionCallingService {
    search_service: Arc<dyn SearchServiceTrait>,  // Local search
    document_service: Arc<dyn DocumentServiceTrait>,  // Local storage
}

impl FunctionCallingService {
    pub fn new(
        search_service: Arc<dyn SearchServiceTrait>,
        document_service: Arc<dyn DocumentServiceTrait>,
    ) -> Self {
        Self {
            search_service,
            document_service,
        }
    }
}

#[async_trait]
impl FunctionCallingServiceTrait for FunctionCallingService {
    async fn semantic_search(&self, input: SemanticSearchInput) -> Result<SemanticSearchOutput> {
        let start = Instant::now();

        // Execute search (100% local - embeddings generated on-device)
        let results = self.search_service
            .search(
                &input.query,  // Embedded locally via ONNX Runtime
                input.limit,
                input.threshold,
                input.search_mode,
            )
            .await?;
        
        let search_time_ms = start.elapsed().as_secs_f64() * 1000.0;
        
        // Map to function calling schema
        let doc_results: Vec<DocumentResult> = results
            .into_iter()
            .map(|r| DocumentResult {
                document_id: r.document_id,
                filename: r.filename,
                file_path: r.file_path,
                mime_type: r.mime_type,
                score: r.score,
                snippet: r.snippet,
                chunk_index: r.chunk_index,
                modified_at: r.modified_at,
                size_bytes: r.size_bytes,
            })
            .collect();
        
        Ok(SemanticSearchOutput {
            results: doc_results.clone(),
            total_found: doc_results.len(),
            search_time_ms,
            query: input.query,
        })
    }
    
    async fn get_document(&self, input: GetDocumentInput) -> Result<GetDocumentOutput> {
        // Fetch document aggregate
        let aggregate = self.document_service
            .get_by_id(&input.document_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!(
                "Document {} not found",
                input.document_id
            )))?;
        
        // Read file content
        let content = self.document_service
            .read_content(&input.document_id)
            .await?;
        
        // Truncate if needed
        let (final_content, truncated) = if content.len() > input.max_content_length {
            (content[..input.max_content_length].to_string(), true)
        } else {
            (content, false)
        };
        
        // Build metadata if requested
        let metadata = if input.include_metadata {
            Some(self.build_metadata(&aggregate))
        } else {
            None
        };
        
        Ok(GetDocumentOutput {
            document_id: input.document_id,
            content: final_content,
            content_truncated: truncated,
            metadata,
        })
    }
    
    async fn list_documents(&self, input: ListDocumentsInput) -> Result<ListDocumentsOutput> {
        // Build filter based on input
        let documents = self.document_service
            .list_filtered(
                input.filter_mode,
                input.file_types.as_deref(),
                input.tags.as_deref(),
                input.date_from,
                input.date_to,
                input.limit,
                input.offset,
                input.sort_by,
                input.sort_order,
            )
            .await?;
        
        // Count total
        let total = self.document_service
            .count_filtered(
                input.filter_mode,
                input.file_types.as_deref(),
                input.tags.as_deref(),
                input.date_from,
                input.date_to,
            )
            .await?;
        
        let has_more = (input.offset + documents.len()) < total;
        
        Ok(ListDocumentsOutput {
            documents,
            total,
            limit: input.limit,
            offset: input.offset,
            has_more,
        })
    }
}

impl FunctionCallingService {
    fn build_metadata(&self, aggregate: &DocumentAggregate) -> DocumentMetadata {
        let doc = aggregate.document();
        let metadata = doc.metadata();
        
        DocumentMetadata {
            filename: metadata.file_name().to_string(),
            file_path: doc.file_path().as_path().display().to_string(),
            mime_type: metadata.mime_type().to_string(),
            extension: metadata.extension().to_string(),
            size_bytes: metadata.size_bytes(),
            created_at: None, // TODO: Add if available
            modified_at: metadata.modified_at(),
            indexed_at: doc.indexed_at(),
            tags: aggregate.tags().iter().map(|t| t.to_string()).collect(),
            chunk_count: aggregate.chunks().len(),
        }
    }
}
```

### 2. Tauri Commands

Create commands in `/src-tauri/src/commands/function_calling.rs`:

```rust
//! Tauri commands for function calling.
//!
//! All commands process data locally via Ollama and local services.
//! No cloud API calls except optional web search.

use crate::application::dtos::function_calling_dto::*;
use crate::di::service_container::ServiceContainer;
use crate::shared::error::{AppError, Result};
use crate::audit::{audit_success, audit_failure, AuditAction};
use tauri::State;

/// Semantic search command (100% local processing).
///
/// Privacy-first: Embeddings generated locally, search performed on-device.
#[tauri::command]
pub async fn semantic_search(
    container: State<'_, ServiceContainer>,
    input: SemanticSearchInput,
) -> Result<SemanticSearchOutput> {
    // Rate limiting
    container.security_context()
        .rate_limiters
        .search
        .check()
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    // Input validation
    let validated_query = container.security_context()
        .input_validator
        .validate_search_query(&input.query)?;

    // Execute search (all local - no cloud API calls)
    let service = container.function_calling_service();
    let result = service.semantic_search(input).await?;
    
    // Audit success
    audit_success!(
        action = AuditAction::SemanticSearch,
        resource_id = validated_query.as_str(),
        metadata = ("result_count", result.total_found.to_string())
    );
    
    Ok(result)
}

/// Get document command.
#[tauri::command]
pub async fn get_document(
    container: State<'_, ServiceContainer>,
    input: GetDocumentInput,
) -> Result<GetDocumentOutput> {
    // Rate limiting
    container.security_context()
        .rate_limiters
        .document_access
        .check()
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;
    
    // Execute
    let service = container.function_calling_service();
    let result = service.get_document(input.clone()).await?;
    
    // Audit success
    audit_success!(
        action = AuditAction::DocumentAccess,
        resource_id = input.document_id.as_str()
    );
    
    Ok(result)
}

/// List documents command.
#[tauri::command]
pub async fn list_documents(
    container: State<'_, ServiceContainer>,
    input: ListDocumentsInput,
) -> Result<ListDocumentsOutput> {
    // Rate limiting
    container.security_context()
        .rate_limiters
        .list
        .check()
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;
    
    // Execute
    let service = container.function_calling_service();
    let result = service.list_documents(input).await?;
    
    // Audit success
    audit_success!(
        action = AuditAction::ListDocuments,
        metadata = ("result_count", result.total.to_string())
    );
    
    Ok(result)
}
```

### 3. Register Commands

Add to `/src-tauri/src/main.rs`:

```rust
mod commands {
    pub mod function_calling;
    // ... other commands
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::function_calling::semantic_search,
            commands::function_calling::get_document,
            commands::function_calling::list_documents,
            // ... other commands
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

---

## Frontend Integration (TypeScript/React)

### Custom Hook

Create `/src/hooks/useFunctionCalling.ts`:

```typescript
/**
 * React hook for function calling with local LLM (Ollama).
 *
 * Privacy-first: All processing happens locally via Tauri IPC.
 * No cloud API calls except optional web search.
 */
import { invoke } from '@tauri-apps/api/core';
import { useState } from 'react';

interface SemanticSearchInput {
  query: string;
  limit?: number;
  threshold?: number;
  search_mode?: 'semantic' | 'keyword' | 'hybrid';  // All modes are local
  file_types?: string[];
}

interface DocumentResult {
  document_id: string;
  filename: string;
  file_path: string;
  score: number;
  snippet: string;
  modified_at: string;
}

interface SemanticSearchOutput {
  results: DocumentResult[];
  total_found: number;
  search_time_ms: number;
  query: string;
}

export function useFunctionCalling() {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const semanticSearch = async (
    input: SemanticSearchInput
  ): Promise<SemanticSearchOutput | null> => {
    setLoading(true);
    setError(null);

    try {
      // Invoke local Tauri command - all processing is local
      const result = await invoke<SemanticSearchOutput>('semantic_search', {
        input,
      });
      // Result generated entirely on-device (no cloud API calls)
      return result;
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Search failed');
      return null;
    } finally {
      setLoading(false);
    }
  };

  const getDocument = async (documentId: string) => {
    setLoading(true);
    setError(null);

    try {
      const result = await invoke('get_document', {
        input: {
          document_id: documentId,
          include_metadata: true,
          max_content_length: 50000,
        },
      });
      return result;
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to get document');
      return null;
    } finally {
      setLoading(false);
    }
  };

  return {
    semanticSearch,
    getDocument,
    loading,
    error,
  };
}
```

### Usage in Component

```typescript
import { useFunctionCalling } from '@/hooks/useFunctionCalling';

export function SearchComponent() {
  const { semanticSearch, loading, error } = useFunctionCalling();
  const [results, setResults] = useState<DocumentResult[]>([]);

  const handleSearch = async (query: string) => {
    // All search happens locally - no cloud API calls
    const searchResult = await semanticSearch({
      query,
      limit: 10,
      search_mode: 'hybrid',  // Local hybrid: embeddings + BM25
    });

    if (searchResult) {
      setResults(searchResult.results);  // Results from local processing
    }
  };

  return (
    <div>
      <input
        type="text"
        onChange={(e) => handleSearch(e.target.value)}
        placeholder="Search documents..."
      />

      {loading && <p>Searching...</p>}
      {error && <p className="error">{error}</p>}

      <div className="results">
        {results.map((doc) => (
          <div key={doc.document_id} className="result-item">
            <h3>{doc.filename}</h3>
            <p>{doc.snippet}</p>
            <span>Score: {doc.score.toFixed(2)}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
```

---

## Testing Examples

### Python Unit Tests

```python
import pytest
from src.schemas.function_calling import SemanticSearchInput
from src.services.function_calling_service import FunctionCallingService


@pytest.mark.asyncio
async def test_semantic_search(db_session):
    """Test semantic search function."""
    service = FunctionCallingService(db_session)
    
    input = SemanticSearchInput(
        query="machine learning",
        limit=10,
        threshold=0.5,
        search_mode="hybrid"
    )
    
    result = await service.semantic_search(input)
    
    assert result.query == "machine learning"
    assert len(result.results) <= 10
    assert result.search_time_ms > 0


@pytest.mark.asyncio
async def test_get_document_not_found(db_session):
    """Test get_document with invalid ID."""
    service = FunctionCallingService(db_session)
    
    input = GetDocumentInput(document_id="nonexistent")
    
    with pytest.raises(FileNotFoundError):
        await service.get_document(input)
```

### Rust Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_semantic_search() {
        let mock_search = Arc::new(MockSearchService::new());
        let mock_doc = Arc::new(MockDocumentService::new());
        
        let service = FunctionCallingService::new(mock_search, mock_doc);
        
        let input = SemanticSearchInput {
            query: "test query".to_string(),
            limit: 10,
            threshold: 0.5,
            search_mode: SearchMode::Hybrid,
            file_types: None,
            date_from: None,
            date_to: None,
        };
        
        let result = service.semantic_search(input).await.unwrap();
        
        assert_eq!(result.query, "test query");
        assert!(result.search_time_ms > 0.0);
    }
    
    #[tokio::test]
    async fn test_rate_limiting() {
        let container = create_test_container().await;
        
        // First 100 should succeed
        for _ in 0..100 {
            let input = SemanticSearchInput {
                query: "test".to_string(),
                ..Default::default()
            };
            
            assert!(semantic_search(State::from(&container), input).await.is_ok());
        }
        
        // 101st should fail
        let input = SemanticSearchInput {
            query: "test".to_string(),
            ..Default::default()
        };
        
        assert!(semantic_search(State::from(&container), input).await.is_err());
    }
}
```

### Integration Tests

```python
import pytest
from httpx import AsyncClient


@pytest.mark.asyncio
async def test_semantic_search_endpoint(client: AsyncClient):
    """Test semantic search API endpoint."""
    response = await client.post(
        "/api/v1/function-calling/semantic-search",
        json={
            "query": "machine learning",
            "limit": 5,
            "search_mode": "hybrid"
        }
    )
    
    assert response.status_code == 200
    data = response.json()
    
    assert "results" in data
    assert "total_found" in data
    assert data["query"] == "machine learning"


@pytest.mark.asyncio
async def test_rate_limiting(client: AsyncClient):
    """Test rate limiting enforcement."""
    # Make 101 requests (limit is 100/min)
    for i in range(101):
        response = await client.post(
            "/api/v1/function-calling/semantic-search",
            json={"query": f"test {i}"}
        )
        
        if i < 100:
            assert response.status_code == 200
        else:
            assert response.status_code == 429  # Rate limit exceeded
```

---

## Next Steps

1. **Implement Backend**:
   - Create `FunctionCallingService`
   - Add API routes
   - Register in router

2. **Implement Desktop**:
   - Create Rust service
   - Add Tauri commands
   - Register commands

3. **Add Tests**:
   - Unit tests for services
   - Integration tests for endpoints
   - Security tests for rate limiting

4. **Documentation**:
   - Generate OpenAPI specs
   - Add usage examples
   - Document rate limits

5. **Security Review**:
   - Verify input validation
   - Test rate limiting
   - Audit logging working
   - Path validation correct

