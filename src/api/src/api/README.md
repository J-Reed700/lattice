# Vault API Documentation

FastAPI-based REST API for the Vault backend - Universal Memory Search & File Indexing.

## Overview

The Vault API provides endpoints for:
- **Search**: Semantic and full-text search across indexed files
- **Files**: File management and metadata operations
- **Indexing**: Trigger and monitor file indexing operations
- **Health**: Service health and readiness checks

## Quick Start

### Running the API

```bash
# Development mode with auto-reload
python -m src.main

# Or using uvicorn directly
uvicorn src.api.app:app --reload --host 0.0.0.0 --port 8000
```

### Configuration

Configure via environment variables or `.env` file:

```env
# API Configuration
API_HOST=0.0.0.0
API_PORT=8000
API_PREFIX=/api/v1
API_WORKERS=4
API_RELOAD=false

# Database
DATABASE_URL=postgresql+asyncpg://vault:vault@localhost:5432/vault

# CORS
CORS_ORIGINS=["http://localhost:3000", "http://localhost:5173"]

# Search
SEARCH_DEFAULT_LIMIT=20
SEARCH_MAX_LIMIT=100
```

## API Endpoints

### Health Endpoints

#### `GET /api/v1/health`
Basic health check - returns service status.

**Response:**
```json
{
  "status": "healthy",
  "timestamp": "2024-01-15T10:00:00Z",
  "service": "recall-api"
}
```

#### `GET /api/v1/health/db`
Database health check - verifies database connectivity.

#### `GET /api/v1/health/ready`
Readiness check for Kubernetes/orchestration.

#### `GET /api/v1/health/live`
Liveness check for Kubernetes/orchestration.

---

### Search Endpoints

#### `POST /api/v1/search`
Search for files using natural language queries.

**Request Body:**
```json
{
  "query": "quarterly sales report 2024",
  "mode": "hybrid",
  "limit": 20,
  "offset": 0,
  "filters": {
    "mime_types": ["application/pdf"],
    "extensions": ["pdf"],
    "date_from": "2024-01-01T00:00:00Z",
    "date_to": "2024-12-31T23:59:59Z",
    "min_size": 1024,
    "max_size": 10485760
  }
}
```

**Search Modes:**
- `vector`: Semantic similarity search using embeddings
- `text`: Traditional keyword/full-text search
- `hybrid`: Combination of vector + text (recommended)

**Response:**
```json
{
  "results": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "file_path": "/Users/john/Documents/report.pdf",
      "filename": "report.pdf",
      "extension": "pdf",
      "mime_type": "application/pdf",
      "size_bytes": 1024000,
      "score": 0.85,
      "snippet": "This quarterly sales report shows...",
      "thumbnail_url": "/api/v1/files/550e8400.../thumbnail",
      "created_at": "2024-01-15T10:30:00Z",
      "modified_at": "2024-01-15T10:30:00Z",
      "indexed_at": "2024-01-15T11:00:00Z",
      "watch_folder_id": "660e8400-e29b-41d4-a716-446655440001"
    }
  ],
  "total": 42,
  "limit": 20,
  "offset": 0,
  "has_more": true,
  "query": "quarterly sales report 2024",
  "mode": "hybrid",
  "took_ms": 245
}
```

#### `GET /api/v1/search/history`
Get recent search history.

---

### File Endpoints

#### `GET /api/v1/files`
List all indexed files with pagination and filtering.

**Query Parameters:**
- `limit` (int): Maximum number of files to return (1-100, default: 20)
- `offset` (int): Number of files to skip (default: 0)
- `mime_type` (str): Filter by MIME type
- `extension` (str): Filter by file extension
- `sort_by` (str): Field to sort by (default: indexed_at)
- `sort_order` (str): asc or desc (default: desc)

**Response:**
```json
{
  "files": [...],
  "total": 150,
  "limit": 20,
  "offset": 0,
  "has_more": true
}
```

#### `GET /api/v1/files/stats`
Get aggregated file statistics.

**Response:**
```json
{
  "total_files": 1543,
  "total_size_bytes": 5368709120,
  "by_mime_type": {
    "application/pdf": 450,
    "image/jpeg": 620
  },
  "by_extension": {
    "pdf": 450,
    "jpg": 620
  },
  "indexed_today": 15,
  "indexed_this_week": 87,
  "indexed_this_month": 324
}
```

#### `GET /api/v1/files/{file_id}`
Get details of a specific file.

#### `POST /api/v1/files`
Index a new file or re-index an existing file.

**Request Body:**
```json
{
  "watch_folder_id": "660e8400-e29b-41d4-a716-446655440001",
  "path": "/Users/john/Documents/report.pdf",
  "force_reindex": false
}
```

#### `PATCH /api/v1/files/{file_id}`
Update file metadata (e.g., last_accessed_at).

#### `DELETE /api/v1/files/{file_id}`
Delete a file from the index.

**Query Parameters:**
- `permanent` (bool): Permanently delete vs soft delete (default: false)

#### `GET /api/v1/files/{file_id}/thumbnail`
Get file thumbnail (images and documents with previews).

#### `GET /api/v1/files/{file_id}/download`
Get download URL or file path.

---

### Indexing Endpoints

#### `POST /api/v1/index`
Trigger indexing of a watch folder.

**Request Body:**
```json
{
  "watch_folder_id": "660e8400-e29b-41d4-a716-446655440001",
  "force": false,
  "recursive": true
}
```

**Response:**
```json
{
  "success": true,
  "message": "Indexing started",
  "data": {
    "watch_folder_id": "660e8400-e29b-41d4-a716-446655440001",
    "force": false,
    "recursive": true
  }
}
```

#### `GET /api/v1/index/status`
Get current indexing status.

**Response:**
```json
{
  "status": "running",
  "total_files": 150,
  "processed_files": 45,
  "failed_files": 2,
  "start_time": "2024-01-15T10:00:00Z",
  "end_time": null,
  "current_file": "/Users/john/Documents/report.pdf"
}
```

#### `POST /api/v1/index/cancel`
Cancel the current indexing operation.

---

## Project Structure

```
src/api/
├── __init__.py           # API package initialization
├── app.py                # FastAPI application factory
├── dependencies.py       # Dependency injection functions
└── v1/                   # API v1 route handlers
    ├── __init__.py       # Router registration
    ├── health.py         # Health check endpoints
    ├── search.py         # Search endpoints
    ├── files.py          # File management endpoints
    ├── index.py          # Indexing endpoints
    ├── documents.py      # Document management
    ├── agent.py          # AI agent endpoints
    ├── agentic_rag.py    # Agentic RAG endpoints
    ├── auth.py           # Authentication endpoints
    ├── sync.py           # Sync endpoints
    └── ...               # Additional endpoints
```

## Architecture

### Application Factory

The `create_app()` function in `app.py` creates and configures the FastAPI application:

1. **Lifespan Management**: Database initialization on startup, cleanup on shutdown
2. **Middleware**: CORS, request logging, timing
3. **Exception Handlers**: Custom error handling and validation
4. **Route Registration**: All API routers with prefix

### Dependency Injection

Dependencies are defined in `dependencies.py`:

- `get_db()`: Database session for each request
- `get_settings_dep()`: Application settings singleton
- `get_search_service()`: Search service instance
- `get_indexing_service()`: Indexing service instance
- `get_storage_service()`: Storage service instance

### Error Handling

All endpoints return consistent error responses:

```json
{
  "error": "Error message",
  "detail": "Detailed error information",
  "code": "ERROR_CODE"
}
```

## Development

### Interactive API Documentation

FastAPI automatically generates interactive API documentation:

- **Swagger UI**: http://localhost:8000/docs
- **ReDoc**: http://localhost:8000/redoc
- **OpenAPI JSON**: http://localhost:8000/openapi.json

### Adding New Endpoints

1. **Define schemas** in `src/schemas/`
2. **Create route handler** in `src/api/v1/`
3. **Register router** in `src/api/v1/__init__.py`

Example:

```python
# src/api/v1/tags.py
from fastapi import APIRouter

router = APIRouter(prefix="/tags", tags=["tags"])

@router.get("")
async def list_tags():
    return {"tags": []}
```

```python
# src/api/v1/__init__.py
from src.api.v1 import tags

router.include_router(tags.router)
```

## Testing

```bash
# Run with pytest
pytest tests/api/

# Test specific endpoint
pytest tests/api/test_search.py -v

# Test with coverage
pytest tests/api/ --cov=src.api --cov-report=html
```

## Production Deployment

### Using Gunicorn + Uvicorn Workers

```bash
gunicorn src.api.app:app \
  --workers 4 \
  --worker-class uvicorn.workers.UvicornWorker \
  --bind 0.0.0.0:8000 \
  --access-logfile - \
  --error-logfile -
```

### Using Docker

```dockerfile
FROM python:3.11-slim

WORKDIR /app
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

COPY src/ src/
CMD ["uvicorn", "src.api.app:app", "--host", "0.0.0.0", "--port", "8000"]
```

### Environment Variables

Required for production:
- `DATABASE_URL`
- `JWT_SECRET_KEY` (change from default!)
- `API_WORKERS`
- `CORS_ORIGINS`

## Security

- **CORS**: Configured origins only
- **Validation**: Pydantic validates all inputs
- **SQL Injection**: SQLAlchemy ORM prevents SQL injection
- **Rate Limiting**: (TODO) Add rate limiting middleware
- **Authentication**: (TODO) JWT token authentication

## Performance

- **Async/Await**: All endpoints are async for high concurrency
- **Connection Pooling**: Database connection pool configured
- **Response Caching**: (TODO) Add caching for expensive queries
- **Pagination**: All list endpoints support pagination

## Monitoring

The API includes:
- **Request Logging**: All requests logged with timing
- **Health Checks**: Multiple health check endpoints
- **Process Time Header**: `X-Process-Time` header on all responses
- **Error Tracking**: All errors logged with stack traces

## License

MIT License - See LICENSE file for details
