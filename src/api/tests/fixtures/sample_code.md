# Code Documentation Sample

## Overview
This markdown file tests the system's ability to handle technical documentation with code samples.

## Installation

```bash
pip install vault-backend
poetry install
```

## Configuration

Create a `.env` file with the following settings:

```env
DATABASE_URL=postgresql://user:password@localhost:5432/vault
EMBEDDING_MODEL=sentence-transformers/all-MiniLM-L6-v2
MAX_FILE_SIZE=500MB
```

## Usage Example

### Basic Search

```python
from vault import VaultClient

client = VaultClient(api_key="your-api-key")

results = client.search(
    query="machine learning algorithms",
    mode="hybrid",
    limit=10
)

for result in results:
    print(f"File: {result.filename}")
    print(f"Score: {result.score}")
    print(f"Snippet: {result.snippet}")
```

### Advanced Search with Filters

```python
results = client.search(
    query="neural networks",
    mode="vector",
    filters={
        "mime_types": ["application/pdf"],
        "date_from": "2024-01-01",
        "extensions": ["pdf", "docx"]
    }
)
```

## API Endpoints

### Upload Document
- **POST** `/api/v1/files`
- **Request Body**:
  ```json
  {
    "watch_folder_id": "uuid",
    "path": "/path/to/file.pdf",
    "force_reindex": false
  }
  ```

### Search Documents
- **POST** `/api/v1/search`
- **Request Body**:
  ```json
  {
    "query": "search query",
    "mode": "hybrid",
    "limit": 20,
    "offset": 0
  }
  ```

## Architecture

The system consists of:
1. **Content Extractor**: Extracts text from PDF, DOCX, TXT, MD files
2. **Embedding Generator**: Creates vector embeddings using transformer models
3. **Vector Store**: Stores embeddings in PostgreSQL with pgvector
4. **Search Engine**: Combines vector similarity and full-text search

## Testing

Run the test suite:

```bash
pytest tests/ -v
pytest tests/integration/ --cov=src
```

## Performance Benchmarks

| Operation | Time (ms) | Throughput |
|-----------|-----------|------------|
| Text Extraction | 50 | 20 files/sec |
| Embedding Generation | 200 | 5 files/sec |
| Vector Search | 100 | 10 queries/sec |
| Hybrid Search | 150 | 7 queries/sec |

## Keywords
indexing, search, embeddings, semantic-search, document-retrieval, knowledge-management, nlp, machine-learning, postgresql, vector-database
