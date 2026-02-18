# Vault Backend Test Suite

Comprehensive test suite for the Vault backend MVP, focusing on document upload/download functionality, content extraction, embedding generation, and semantic search.

## Test Structure

```
tests/
├── conftest.py                    # Shared fixtures and configuration
├── fixtures/                      # Sample test data files
│   ├── sample_document.txt
│   ├── sample_code.md
│   └── sample_data.json
├── unit/                          # Unit tests (60%)
│   ├── api/
│   │   └── test_documents.py     # Document API endpoint tests
│   ├── services/
│   │   └── test_indexing.py      # Indexing service tests
│   └── modules/
│       └── test_content_extractor.py  # Content extraction tests
└── integration/                   # Integration tests (30%)
    ├── test_upload_pipeline.py    # Upload flow tests
    └── test_search_pipeline.py    # Search flow tests
```

## Test Coverage Goals

- **Target Coverage**: >70% of new code
- **Unit Tests**: 60% - Fast, isolated component tests
- **Integration Tests**: 30% - End-to-end workflow tests
- **E2E Tests**: 10% - Critical user paths (future)

## Prerequisites

### 1. Install Dependencies

```bash
cd vault/backend
poetry install
```

### 2. Setup Test Database

Create a test database in PostgreSQL:

```sql
CREATE DATABASE vault_test;
CREATE USER vault WITH PASSWORD 'vault';
GRANT ALL PRIVILEGES ON DATABASE vault_test TO vault;
```

Enable pgvector extension:

```sql
\c vault_test
CREATE EXTENSION IF NOT EXISTS vector;
```

### 3. Configure Environment

Create `.env.test` file:

```env
TEST_DATABASE_URL=postgresql+asyncpg://vault:vault@localhost:5432/vault_test
EMBEDDING_MODEL=sentence-transformers/all-MiniLM-L6-v2
LOG_LEVEL=WARNING
```

### 4. Optional: Install PDF Support

For PDF tests (optional, will be skipped if not installed):

```bash
poetry add --group dev reportlab
```

## Running Tests

### Run All Tests

```bash
pytest tests/ -v
```

### Run Specific Test Categories

```bash
# Unit tests only
pytest tests/unit/ -v

# Integration tests only
pytest tests/integration/ -v

# Specific test file
pytest tests/unit/api/test_documents.py -v

# Specific test class
pytest tests/unit/api/test_documents.py::TestListFiles -v

# Specific test method
pytest tests/unit/api/test_documents.py::TestListFiles::test_list_files_empty -v
```

### Run with Coverage

```bash
# Generate coverage report
pytest tests/ --cov=src --cov-report=html --cov-report=term

# View HTML report
open htmlcov/index.html  # macOS
start htmlcov/index.html  # Windows
xdg-open htmlcov/index.html  # Linux
```

### Run by Markers

```bash
# Run only unit tests
pytest -m unit

# Run only integration tests
pytest -m integration

# Skip slow tests
pytest -m "not slow"

# Run asyncio tests
pytest -m asyncio
```

## Test Fixtures

### Database Fixtures

- `test_engine`: Session-scoped test database engine
- `db_session`: Function-scoped database session with auto-rollback
- `watch_folder`: Pre-created watch folder for file tests

### File Fixtures

- `temp_dir`: Temporary directory for test files
- `sample_text_file`: Plain text test file
- `sample_markdown_file`: Markdown test file
- `sample_json_file`: JSON test file
- `sample_pdf_file`: Generated PDF test file (requires reportlab)
- `large_text_file`: Large text file for performance tests
- `invalid_pdf_file`: Corrupted PDF for error testing

### Model Fixtures

- `indexed_file`: Pre-indexed file in database
- `indexed_file_with_content`: File with extracted text content
- `multiple_indexed_files`: List of 5 indexed files
- `file_factory`: Factory function to create test files

### Mock Fixtures

- `mock_embedding_service`: Mocked embedding service
- `mock_content_extractor`: Mocked content extractor

## Writing New Tests

### Test Naming Convention

- Test files: `test_*.py`
- Test classes: `Test*`
- Test methods: `test_*`

### Example Unit Test

```python
import pytest

@pytest.mark.unit
class TestDocumentUpload:

    async def test_upload_text_file(
        self,
        client: AsyncClient,
        watch_folder,
        sample_text_file
    ):
        payload = {
            "watch_folder_id": str(watch_folder.id),
            "path": str(sample_text_file),
            "force_reindex": False
        }

        response = await client.post("/api/v1/files", json=payload)
        assert response.status_code == 200
        assert response.json()["success"] is True
```

### Example Integration Test

```python
import pytest

@pytest.mark.integration
class TestUploadSearchFlow:

    async def test_upload_then_search(
        self,
        client: AsyncClient,
        db_session: AsyncSession,
        watch_folder,
        sample_text_file
    ):
        # Upload document
        upload_payload = {
            "watch_folder_id": str(watch_folder.id),
            "path": str(sample_text_file),
            "force_reindex": False
        }
        upload_response = await client.post("/api/v1/files", json=upload_payload)
        assert upload_response.status_code == 200

        # Search for document
        search_payload = {
            "query": "test document",
            "mode": "text",
            "limit": 10
        }
        search_response = await client.post("/api/v1/search", json=search_payload)
        assert search_response.status_code == 200
        assert search_response.json()["total"] >= 1
```

## Test Categories

### Unit Tests

#### Document API (`tests/unit/api/test_documents.py`)
- List files with pagination and filters
- Get file by ID
- Create/index file
- Update file metadata
- Delete file (soft and permanent)
- Get download URL
- Get file statistics

#### Indexing Service (`tests/unit/services/test_indexing.py`)
- File hashing and deduplication
- MIME type detection
- Modified file detection
- Error handling
- Task management
- Batch processing

#### Content Extraction (`tests/unit/modules/test_content_extractor.py`)
- Text file extraction
- PDF extraction
- JSON extraction
- Markdown extraction
- Language detection
- Encoding handling
- Error cases (corrupted files, missing files)

### Integration Tests

#### Upload Pipeline (`tests/integration/test_upload_pipeline.py`)
- End-to-end upload flow
- Content extraction during upload
- Embedding generation
- Storage integration
- Duplicate handling
- Batch upload
- Upload-download roundtrip

#### Search Pipeline (`tests/integration/test_search_pipeline.py`)
- Upload and search flow
- Vector search
- Text search
- Hybrid search
- Search with filters
- Search pagination
- Search ranking
- Multi-file search

## Continuous Integration

### GitHub Actions (future)

```yaml
- name: Run tests
  run: |
    poetry run pytest tests/ -v --cov=src --cov-report=xml

- name: Upload coverage
  uses: codecov/codecov-action@v3
  with:
    files: ./coverage.xml
```

## Troubleshooting

### Database Connection Issues

```bash
# Check PostgreSQL is running
pg_isready -h localhost -p 5432

# Test connection
psql -h localhost -U vault -d vault_test
```

### Import Errors

```bash
# Ensure you're in the backend directory
cd vault/backend

# Reinstall dependencies
poetry install

# Verify Python path
poetry run python -c "import src; print(src.__file__)"
```

### Test Failures

```bash
# Run with verbose output
pytest tests/ -vv

# Run with print statements visible
pytest tests/ -s

# Run with warnings
pytest tests/ -W default

# Stop on first failure
pytest tests/ -x
```

### Slow Tests

```bash
# Skip slow tests
pytest tests/ -m "not slow"

# Show slowest 10 tests
pytest tests/ --durations=10
```

## Performance Benchmarks

Expected test performance:

- **Unit tests**: <1s per test
- **Integration tests**: <5s per test
- **Full suite**: <60s total

## Coverage Reports

Coverage goals by module:

| Module | Target | Current |
|--------|--------|---------|
| API Routes | 90% | - |
| Services | 85% | - |
| Content Extractors | 80% | - |
| Models | 70% | - |
| Overall | 75% | - |

## Best Practices

1. **Isolation**: Each test should be independent
2. **Speed**: Unit tests should run quickly (<100ms)
3. **Clarity**: Test names should describe what they test
4. **Coverage**: Test happy paths, edge cases, and error cases
5. **Fixtures**: Reuse fixtures to reduce duplication
6. **Mocking**: Mock external dependencies (storage, APIs)
7. **Cleanup**: Tests should clean up after themselves

## Future Enhancements

- [ ] E2E tests with real embedding models
- [ ] Performance/load testing suite
- [ ] Property-based testing with Hypothesis
- [ ] Contract testing for API endpoints
- [ ] Visual regression testing for UI components
- [ ] Chaos engineering tests

## Resources

- [pytest Documentation](https://docs.pytest.org/)
- [pytest-asyncio Documentation](https://pytest-asyncio.readthedocs.io/)
- [Coverage.py Documentation](https://coverage.readthedocs.io/)
- [FastAPI Testing](https://fastapi.tiangolo.com/tutorial/testing/)
- [SQLAlchemy Testing](https://docs.sqlalchemy.org/en/20/orm/session_transaction.html#joining-a-session-into-an-external-transaction-such-as-for-test-suites)
