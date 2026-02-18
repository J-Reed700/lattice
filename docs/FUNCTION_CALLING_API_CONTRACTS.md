# Function Calling API Contracts

**Design Philosophy**: Bricks and Studs
**Architecture**: Local-First with Ollama
**Version**: 1.0
**Last Updated**: 2025-11-20

This document defines the API contracts for LLM function calling tools in the Recall system.

**Privacy-First Design**: All core functions (search, documents) operate locally. Optional web search is the only external service.

## Table of Contents

1. [Design Principles](#design-principles)
2. [Phase 1: Core Retrieval](#phase-1-core-retrieval)
   - [semantic_search](#1-semantic_search)
   - [get_document](#2-get_document)
   - [list_documents](#3-list_documents)
3. [Phase 2: Web Integration](#phase-2-web-integration)
   - [web_search](#4-web_search)
   - [fetch_url_content](#5-fetch_url_content)
4. [Error Handling](#error-handling)
5. [Security & Rate Limiting](#security--rate-limiting)

---

## Design Principles

Following the **"Bricks and Studs"** philosophy:

- **Local-First**: All processing happens on-device via Ollama (no cloud AI)
- **Privacy-Preserving**: No data leaves your machine except optional web search
- **Minimal**: Only essential fields, no hypothetical futures
- **Clear**: Descriptive names, comprehensive documentation
- **Stable**: Schema versioning for breaking changes
- **Validated**: Strong type checking at all boundaries
- **Regeneratable**: Implementation can be rebuilt from specs

Each function is a **"stud"** - a well-defined connection point between system components.

**Key Architecture Features**:
- Local LLM inference via Ollama (no API keys required)
- Local embeddings for semantic search
- Local storage (SQLite + FAISS)
- Optional web search via DuckDuckGo (user choice)

---

## Phase 1: Core Retrieval

### 1. semantic_search

**Purpose**: Search the user's document vault using semantic similarity (100% local processing).

#### Tool Definition (Ollama-Compatible Format)

```json
{
  "name": "semantic_search",
  "description": "Search the user's personal document vault using semantic similarity. All processing is 100% local - embeddings generated on-device, no data sent to cloud. Use this when the user asks to find, search, or retrieve information from their indexed documents. Supports semantic (local embedding-based), keyword (BM25), and hybrid search modes. Returns ranked results with snippets and metadata.",
  "input_schema": {
    "type": "object",
    "properties": {
      "query": {
        "type": "string",
        "description": "Search query describing what to find (e.g., 'machine learning papers', 'meeting notes from last week')",
        "minLength": 1,
        "maxLength": 500
      },
      "limit": {
        "type": "integer",
        "description": "Maximum number of results to return",
        "default": 10,
        "minimum": 1,
        "maximum": 50
      },
      "threshold": {
        "type": "number",
        "description": "Minimum similarity score (0.0-1.0). Lower values return more results",
        "default": 0.3,
        "minimum": 0.0,
        "maximum": 1.0
      },
      "search_mode": {
        "type": "string",
        "enum": ["semantic", "keyword", "hybrid"],
        "description": "Search algorithm: 'semantic' (embeddings), 'keyword' (BM25), or 'hybrid' (both)",
        "default": "hybrid"
      },
      "file_types": {
        "type": "array",
        "items": { "type": "string" },
        "description": "Filter by file extensions (e.g., ['pdf', 'txt']). Max 10 extensions",
        "maxItems": 10
      },
      "date_from": {
        "type": "string",
        "format": "date-time",
        "description": "Only return documents modified after this date (ISO 8601)"
      },
      "date_to": {
        "type": "string",
        "format": "date-time",
        "description": "Only return documents modified before this date (ISO 8601)"
      }
    },
    "required": ["query"],
    "additionalProperties": false
  }
}
```

#### Response Schema

```typescript
{
  "results": [
    {
      "document_id": "550e8400-e29b-41d4-a716-446655440000",
      "filename": "machine_learning_paper.pdf",
      "file_path": "/vault/documents/ml/machine_learning_paper.pdf",
      "mime_type": "application/pdf",
      "score": 0.89,
      "snippet": "...gradient descent optimization in neural networks...",
      "chunk_index": 3,
      "modified_at": "2025-11-15T14:30:00Z",
      "size_bytes": 1048576
    }
  ],
  "total_found": 15,
  "search_time_ms": 42.5,
  "query": "gradient descent"
}
```

#### Example Usage

```python
# Example 1: Simple semantic search
result = await semantic_search(
    query="machine learning papers",
    limit=10
)

# Example 2: Filtered search
result = await semantic_search(
    query="project meeting notes",
    file_types=["txt", "md"],
    date_from="2025-11-01T00:00:00Z",
    search_mode="hybrid"
)

# Example 3: High precision search
result = await semantic_search(
    query="neural network architecture",
    threshold=0.7,  # Higher threshold = fewer but more relevant results
    limit=5
)
```

#### Validation Rules

- **query**: Required, 1-500 characters
- **limit**: 1-50 (prevents excessive results)
- **threshold**: 0.0-1.0 (similarity score range)
- **file_types**: Max 10 extensions (prevents filter abuse)
- **search_mode**: Must be one of: semantic, keyword, hybrid

#### Rate Limiting

- **100 requests/minute** per user
- Reason: Search is computationally expensive (embeddings, vector similarity)

#### Audit Events

```rust
AuditAction::SemanticSearch {
    query: String,
    result_count: usize,
    search_mode: SearchMode,
}
```

---

### 2. get_document

**Purpose**: Retrieve the full content of a specific document by ID (from local storage).

#### Tool Definition (Ollama-Compatible Format)

```json
{
  "name": "get_document",
  "description": "Retrieve the full text content of a specific document by its ID from local storage. All processing is local - no data sent to cloud. Use this after semantic_search to get the complete document text, or when you need to read an entire document. Returns full content (up to max_content_length) with optional metadata.",
  "input_schema": {
    "type": "object",
    "properties": {
      "document_id": {
        "type": "string",
        "description": "Document ID from search results",
        "minLength": 1,
        "maxLength": 100
      },
      "include_metadata": {
        "type": "boolean",
        "description": "Include file metadata (size, dates, tags)",
        "default": true
      },
      "max_content_length": {
        "type": "integer",
        "description": "Maximum content length in characters (prevents huge returns)",
        "default": 50000,
        "minimum": 1000,
        "maximum": 100000
      }
    },
    "required": ["document_id"],
    "additionalProperties": false
  }
}
```

#### Response Schema

```typescript
{
  "document_id": "550e8400-e29b-41d4-a716-446655440000",
  "content": "Full document text content here...",
  "content_truncated": false,
  "metadata": {
    "filename": "research_paper.pdf",
    "file_path": "/vault/documents/research_paper.pdf",
    "mime_type": "application/pdf",
    "extension": "pdf",
    "size_bytes": 1048576,
    "created_at": "2025-10-01T10:00:00Z",
    "modified_at": "2025-11-15T14:30:00Z",
    "indexed_at": "2025-11-15T15:00:00Z",
    "tags": ["research", "machine-learning"],
    "chunk_count": 12
  }
}
```

#### Example Usage

```python
# Example 1: Get document with metadata
doc = await get_document(
    document_id="550e8400-e29b-41d4-a716-446655440000",
    include_metadata=True
)

# Example 2: Get content only (no metadata)
doc = await get_document(
    document_id="abc123",
    include_metadata=False,
    max_content_length=10000  # Shorter limit
)

# Check if content was truncated
if doc.content_truncated:
    print(f"Content truncated at {len(doc.content)} characters")
```

#### Validation Rules

- **document_id**: Required, 1-100 characters
- **max_content_length**: 1,000-100,000 characters (prevents memory exhaustion)
- Document must exist and be accessible

#### Rate Limiting

- **200 requests/minute** per user
- Reason: Less expensive than search, but still I/O intensive

#### Audit Events

```rust
AuditAction::DocumentAccess {
    document_id: String,
    include_metadata: bool,
}
```

#### Security Considerations

- **Path Validation**: All file paths validated via `ValidatedFilePath` (prevents CWE-22)
- **Content Truncation**: Enforced `max_content_length` prevents DoS
- **Access Control**: Only indexed documents accessible (no arbitrary file reads)

---

### 3. list_documents

**Purpose**: Browse and filter documents in the vault (from local database).

#### Tool Definition (Ollama-Compatible Format)

```json
{
  "name": "list_documents",
  "description": "Browse and filter documents in the local vault. All processing is local - no data sent to cloud. Use this to explore what documents exist, get recent/favorite documents, or filter by type/date/tags. Supports pagination and sorting. Good for answering 'show me all PDFs' or 'recent documents'.",
  "input_schema": {
    "type": "object",
    "properties": {
      "filter_mode": {
        "type": "string",
        "enum": ["all", "recent", "favorites", "by_tag", "by_type"],
        "description": "Filtering mode for documents",
        "default": "all"
      },
      "file_types": {
        "type": "array",
        "items": { "type": "string" },
        "description": "Filter by file extensions (for 'by_type' mode)",
        "maxItems": 10
      },
      "tags": {
        "type": "array",
        "items": { "type": "string" },
        "description": "Filter by tags (for 'by_tag' mode)",
        "maxItems": 20
      },
      "date_from": {
        "type": "string",
        "format": "date-time",
        "description": "Only return documents modified after this date"
      },
      "date_to": {
        "type": "string",
        "format": "date-time",
        "description": "Only return documents modified before this date"
      },
      "limit": {
        "type": "integer",
        "description": "Maximum number of documents to return",
        "default": 50,
        "minimum": 1,
        "maximum": 500
      },
      "offset": {
        "type": "integer",
        "description": "Number of documents to skip (for pagination)",
        "default": 0,
        "minimum": 0
      },
      "sort_by": {
        "type": "string",
        "enum": ["modified", "created", "name", "size"],
        "description": "Sort field",
        "default": "modified"
      },
      "sort_order": {
        "type": "string",
        "enum": ["asc", "desc"],
        "description": "Sort direction",
        "default": "desc"
      }
    },
    "required": [],
    "additionalProperties": false
  }
}
```

#### Response Schema

```typescript
{
  "documents": [
    {
      "document_id": "doc-123",
      "filename": "report.pdf",
      "file_path": "/vault/reports/report.pdf",
      "mime_type": "application/pdf",
      "extension": "pdf",
      "size_bytes": 524288,
      "modified_at": "2025-11-15T10:00:00Z",
      "tags": ["work", "important"],
      "is_favorite": true,
      "access_count": 5
    }
  ],
  "total": 127,
  "limit": 50,
  "offset": 0,
  "has_more": true
}
```

#### Example Usage

```python
# Example 1: Get all recent documents
docs = await list_documents(
    filter_mode="recent",
    limit=20
)

# Example 2: Filter by file type
docs = await list_documents(
    filter_mode="by_type",
    file_types=["pdf", "docx"],
    sort_by="size",
    sort_order="desc"
)

# Example 3: Pagination
page1 = await list_documents(limit=50, offset=0)
page2 = await list_documents(limit=50, offset=50)

# Example 4: Tag filtering
docs = await list_documents(
    filter_mode="by_tag",
    tags=["important", "work"],
    date_from="2025-11-01T00:00:00Z"
)
```

#### Validation Rules

- **limit**: 1-500 (prevents excessive memory usage)
- **file_types**: Max 10 (prevents filter complexity)
- **tags**: Max 20 (prevents filter complexity)
- **filter_mode**: Must match expected tags/file_types usage

#### Rate Limiting

- **200 requests/minute** per user
- Reason: Database query, less expensive than search

#### Audit Events

```rust
AuditAction::ListDocuments {
    filter_mode: FilterMode,
    result_count: usize,
}
```

---

## Phase 2: Web Integration

**Note**: Web search is the ONLY external service in Recall. All other functions are 100% local.

### 4. web_search

**Purpose**: Search the web using DuckDuckGo when information isn't in the vault (optional external service).

#### Tool Definition (Ollama-Compatible Format)

```json
{
  "name": "web_search",
  "description": "Search the web using DuckDuckGo (optional external service - user choice). This is the ONLY function that sends data externally. Use this when the user asks about current events, external information, or when vault search returns no relevant results. Returns search results with titles, URLs, and snippets.",
  "input_schema": {
    "type": "object",
    "properties": {
      "query": {
        "type": "string",
        "description": "Web search query",
        "minLength": 1,
        "maxLength": 300
      },
      "max_results": {
        "type": "integer",
        "description": "Maximum number of web results to return",
        "default": 5,
        "minimum": 1,
        "maximum": 10
      },
      "region": {
        "type": "string",
        "enum": ["wt-wt", "us-en", "uk-en", "de-de", "fr-fr"],
        "description": "DuckDuckGo region code (wt-wt = worldwide)",
        "default": "wt-wt"
      },
      "safesearch": {
        "type": "string",
        "enum": ["off", "moderate", "strict"],
        "description": "Safe search level",
        "default": "moderate"
      }
    },
    "required": ["query"],
    "additionalProperties": false
  }
}
```

#### Response Schema

```typescript
{
  "results": [
    {
      "title": "Python 3.12 Release Notes",
      "url": "https://docs.python.org/3.12/whatsnew/",
      "snippet": "Python 3.12 introduces new features including improved error messages...",
      "published_date": "2025-10-02T00:00:00Z"
    }
  ],
  "query": "Python 3.12 features",
  "result_count": 5
}
```

#### Example Usage

```python
# Example 1: Simple web search
results = await web_search(
    query="Python 3.12 new features",
    max_results=5
)

# Example 2: Regional search
results = await web_search(
    query="local weather Tokyo",
    region="us-en",
    safesearch="strict"
)

# Example 3: Current events
results = await web_search(
    query="AI news today",
    max_results=10
)
```

#### Validation Rules

- **query**: Required, 1-300 characters
- **max_results**: 1-10 (prevents API abuse)
- **region**: Must be valid DuckDuckGo region code
- **safesearch**: Must be: off, moderate, or strict

#### Rate Limiting

- **30 requests/minute** per user
- Reason: External API, rate limiting prevents abuse and API quota exhaustion

#### Audit Events

```rust
AuditAction::WebSearch {
    query: String,
    result_count: usize,
}
```

#### Security Considerations

- **External API**: This is the ONLY function that sends data externally (user choice)
- **Rate Limiting**: Prevents quota exhaustion
- **Input Sanitization**: Query sanitized before passing to DuckDuckGo
- **Safe Search**: Default to 'moderate' for safety
- **User Consent**: Users should be aware this function makes external requests

---

### 5. fetch_url_content

**Purpose**: Fetch and extract readable text from a web URL (optional external service).

#### Tool Definition (Ollama-Compatible Format)

```json
{
  "name": "fetch_url_content",
  "description": "Fetch and extract readable text content from a web URL (optional external service - user choice). Use this after web_search to get full content from interesting results, or when the user provides a URL to read/analyze. Supports article extraction, raw text, and markdown modes. Once fetched, content is processed locally.",
  "input_schema": {
    "type": "object",
    "properties": {
      "url": {
        "type": "string",
        "description": "URL to fetch and extract content from",
        "minLength": 10,
        "maxLength": 2000,
        "format": "uri"
      },
      "max_content_length": {
        "type": "integer",
        "description": "Maximum content length in characters",
        "default": 50000,
        "minimum": 1000,
        "maximum": 100000
      },
      "extract_mode": {
        "type": "string",
        "enum": ["article", "raw_text", "markdown"],
        "description": "Content extraction mode: 'article' (main content), 'raw_text' (all text), 'markdown' (preserve formatting)",
        "default": "article"
      },
      "timeout_seconds": {
        "type": "integer",
        "description": "Request timeout in seconds",
        "default": 10,
        "minimum": 1,
        "maximum": 30
      }
    },
    "required": ["url"],
    "additionalProperties": false
  }
}
```

#### Response Schema

```typescript
{
  "url": "https://example.com/article",
  "title": "Understanding Machine Learning",
  "content": "Machine learning is a subset of artificial intelligence...",
  "content_truncated": false,
  "word_count": 1523,
  "fetch_time_ms": 342.5,
  "content_type": "text/html; charset=utf-8"
}
```

#### Example Usage

```python
# Example 1: Fetch article
content = await fetch_url_content(
    url="https://docs.python.org/3/tutorial/",
    extract_mode="article"
)

# Example 2: Get raw text
content = await fetch_url_content(
    url="https://example.com/page",
    extract_mode="raw_text",
    max_content_length=10000
)

# Example 3: Markdown preservation
content = await fetch_url_content(
    url="https://github.com/user/repo/README.md",
    extract_mode="markdown",
    timeout_seconds=15
)

# Check if truncated
if content.content_truncated:
    print(f"Content truncated to {content.word_count} words")
```

#### Validation Rules

- **url**: Required, 10-2,000 characters, valid URI format
- **max_content_length**: 1,000-100,000 characters
- **timeout_seconds**: 1-30 seconds
- URL must use http:// or https:// (no file://, ftp://)

#### Rate Limiting

- **20 requests/minute** per user
- Reason: Network I/O expensive, prevents abuse

#### Audit Events

```rust
AuditAction::FetchUrl {
    url: String,
    extract_mode: ExtractMode,
}
```

#### Security Considerations

- **External Request**: This function fetches external content (user choice)
- **URL Validation**: Must be http/https, no local file access
- **Timeout**: Max 30s prevents hung connections
- **Content Length**: Max 100k prevents memory exhaustion
- **SSRF Prevention**: No access to internal/private IPs
- **User Agent**: Identifies as Recall system
- **Local Processing**: Once fetched, content is processed locally

---

## Error Handling

All functions return errors in a consistent format:

```typescript
{
  "error_code": "DOCUMENT_NOT_FOUND",
  "message": "Document with ID abc123 not found",
  "details": {
    "document_id": "abc123",
    "requested_at": "2025-11-20T10:30:00Z"
  }
}
```

### Standard Error Codes

| Error Code | HTTP Status | Description |
|------------|-------------|-------------|
| `DOCUMENT_NOT_FOUND` | 404 | Document ID doesn't exist |
| `RATE_LIMIT_EXCEEDED` | 429 | Too many requests |
| `INVALID_INPUT` | 400 | Validation failed |
| `INVALID_URL` | 400 | URL format invalid |
| `FETCH_FAILED` | 502 | Web fetch failed |
| `SEARCH_ERROR` | 500 | Search service error |
| `CONTENT_TOO_LARGE` | 413 | Content exceeds max_content_length |
| `TIMEOUT` | 504 | Request timed out |
| `UNAUTHORIZED` | 401 | Authentication required |

---

## Security & Rate Limiting

### Rate Limits (per user, per minute)

| Function | Limit | Reason |
|----------|-------|--------|
| `semantic_search` | 100/min | Computationally expensive (embeddings) |
| `get_document` | 200/min | I/O intensive |
| `list_documents` | 200/min | Database query |
| `web_search` | 30/min | External API quota |
| `fetch_url_content` | 20/min | Network I/O expensive |

### Security Controls

#### Input Validation

All inputs validated via Pydantic (Python) / serde (Rust):

```rust
// Example: Path validation in Rust
let path = ValidatedFilePath::new(user_provided_path)
    .map_err(|_| AppError::InvalidInput("Path traversal detected"))?;

// Example: Query validation in Python
class SemanticSearchInput(BaseModel):
    query: str = Field(..., min_length=1, max_length=500)
    
    @validator("query")
    def validate_query(cls, v: str) -> str:
        if not v.strip():
            raise ValueError("Query cannot be empty")
        return v.strip()
```

#### Rate Limiting Implementation

```rust
// Rate limiter check (CWE-770 mitigation)
container.security_context()
    .rate_limiters
    .semantic_search
    .check()
    .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;
```

#### Audit Logging

All function calls logged:

```rust
// Success audit
audit_success!(
    action = AuditAction::SemanticSearch,
    resource_id = query.as_str(),
    metadata = ("result_count", result_count.to_string())
);

// Failure audit
audit_failure!(
    action = AuditAction::SemanticSearch,
    resource_id = query.as_str(),
    error = format!("Rate limit exceeded: {}", e)
);
```

#### Path Validation (CWE-22 Prevention)

```rust
// All file operations use ValidatedFilePath
pub struct ValidatedFilePath {
    path: PathBuf,
}

impl ValidatedFilePath {
    pub fn new(path: PathBuf) -> Result<Self> {
        // Prevents:
        // - Directory traversal (../)
        // - Access outside allowed directories
        // - Symlink attacks
        // - Null byte injection
    }
}
```

#### Content Size Limits

All content truncated to prevent DoS:

```python
class GetDocumentInput(BaseModel):
    max_content_length: int = Field(
        default=50000,
        ge=1000,
        le=100000  # Hard limit: 100k chars
    )
```

#### Web Fetch Security

```rust
// SSRF prevention
fn validate_url(url: &str) -> Result<Url> {
    let parsed = Url::parse(url)?;
    
    // Only http/https
    if !["http", "https"].contains(&parsed.scheme()) {
        return Err(AppError::InvalidUrl("Scheme must be http/https"));
    }
    
    // No private IPs
    if let Some(host) = parsed.host() {
        if is_private_ip(host) {
            return Err(AppError::InvalidUrl("Access to private IPs forbidden"));
        }
    }
    
    Ok(parsed)
}
```

---

## Implementation Checklist

When implementing these functions:

- [ ] **Input Validation**: All inputs validated via Pydantic/serde
- [ ] **Rate Limiting**: Applied per function as specified
- [ ] **Audit Logging**: Success and failure events logged
- [ ] **Path Validation**: Use `ValidatedFilePath` for all file ops
- [ ] **Content Limits**: Enforce `max_content_length` limits
- [ ] **Timeout Handling**: Respect timeout parameters
- [ ] **Error Consistency**: Use standard `FunctionCallError` format
- [ ] **Documentation**: OpenAPI/JSON schemas generated
- [ ] **Tests**: Unit tests for validation, integration tests for flows
- [ ] **Security Review**: Check for CWE-22, CWE-770, CWE-918

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2025-11-20 | Initial design for 5 functions |

---

## References

- **Pydantic Schemas**: `/vault/backend/src/schemas/function_calling.py`
- **Rust DTOs**: `/vault/desktop/src-tauri/src/application/dtos/function_calling_dto.rs`
- **Security Controls**: `CLAUDE.md` - Desktop Security Best Practices
- **Rate Limiting**: CWE-770 (Uncontrolled Resource Consumption)
- **Path Validation**: CWE-22 (Path Traversal)
- **SSRF Prevention**: CWE-918 (Server-Side Request Forgery)

