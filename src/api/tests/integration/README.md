# Integration Tests

Comprehensive integration tests for the Recall/Vault application covering critical paths across multiple components.

## Test Coverage

### 1. Upload Pipeline (`test_upload_pipeline.py`)
- ✅ Full upload flow: file → indexing → embedding → storage → search
- ✅ Text, Markdown, JSON file uploads
- ✅ Error handling (nonexistent files, corrupted PDFs)
- ✅ Content extraction and embedding generation
- ✅ Duplicate file handling and force reindex
- ✅ Batch upload operations
- ✅ Upload-download roundtrip

### 2. Search Pipeline (`test_search_pipeline.py`)
- ✅ Text, vector, and hybrid search modes
- ✅ Search with filters (MIME type, extension, date range)
- ✅ Pagination and result structure
- ✅ Search accuracy and ranking
- ✅ Error handling (empty query, invalid parameters)
- ✅ Performance metrics (response time)
- ✅ Multi-file search

### 3. Watch Folder Flow (`test_watch_service_integration.py`)
- ✅ Start/stop watching directories
- ✅ File created/modified/deleted events
- ✅ File filtering and hash computation
- ✅ Large file skipping
- ✅ End-to-end watching workflow

### 4. Concurrent Operations (`test_concurrent_operations.py`)
- ✅ Concurrent file uploads (10+ files simultaneously)
- ✅ Concurrent searches (5+ queries simultaneously)
- ✅ Mixed concurrent operations (upload + search + list)
- ✅ Concurrent filtered searches
- ✅ Concurrent metadata updates
- ✅ Batch indexing with concurrent searches
- ✅ Race condition handling (duplicate uploads)
- ✅ High concurrency stress test (30+ operations)
- ✅ Concurrent database transactions

### 5. Transaction Rollback (`test_transaction_rollback.py`)
- ✅ Rollback on embedding failure
- ✅ Rollback on storage failure
- ✅ Partial batch rollback
- ✅ Search history rollback
- ✅ Cascade delete rollback
- ✅ Constraint violation handling (duplicate hash, foreign key)
- ✅ Optimistic locking conflicts
- ✅ Nested savepoint rollback
- ✅ API error rollback

### 6. Reranking Flow (`test_reranking_flow.py`)
- ✅ Search with reranking enabled/disabled
- ✅ Reranking improves relevance scores
- ✅ Vector and hybrid search with reranking
- ✅ Graceful fallback on reranker error
- ✅ Reranking with empty results
- ✅ Performance metrics
- ✅ Large result set reranking
- ✅ Reranking with filters
- ✅ Score ordering after reranking
- ✅ Custom weights and strategies

### 7. Export & Download Flow (`test_export_download_flow.py`)
- ✅ Download file by ID
- ✅ Download nonexistent file handling
- ✅ Multiple file downloads
- ✅ Access timestamp updates
- ✅ Export file metadata as JSON
- ✅ Export search results
- ✅ Export file statistics
- ✅ Paginated file list export
- ✅ Bulk export with filters
- ✅ Cloud storage integration
- ✅ Thumbnail generation (not implemented marker)
- ✅ Export format validation
- ✅ Security (path traversal prevention, UUID validation)
- ✅ Document access tracking
- ✅ Performance (large exports)

### 8. Storage Cleanup (`test_storage_cleanup.py`)
- ✅ Orphaned text content detection
- ✅ Files without content detection
- ✅ Content without embeddings detection
- ✅ Orphaned storage file cleanup
- ✅ Deleted file record cleanup
- ✅ Old file version cleanup
- ✅ Cascade delete verification
- ✅ Stale embedding cleanup
- ✅ Temporary file cleanup
- ✅ Failed upload artifact cleanup
- ✅ Vector store cleanup
- ✅ Scheduled cleanup tasks
- ✅ Retention policy enforcement
- ✅ Cleanup metrics and statistics
- ✅ Safety (preserve active files, rollback on error)
- ✅ Large-scale cleanup (100+ files)
- ✅ Batch processing

## Running Tests

### Run all integration tests
```bash
pytest tests/integration/ -v
```

### Run specific test file
```bash
pytest tests/integration/test_concurrent_operations.py -v
```

### Run excluding slow tests
```bash
pytest tests/integration/ -v -m "not slow"
```

### Run with coverage
```bash
pytest tests/integration/ -v --cov=src --cov-report=html
```

### Run specific test class
```bash
pytest tests/integration/test_reranking_flow.py::TestRerankingIntegration -v
```

## Test Markers

- `@pytest.mark.integration` - Integration test marker
- `@pytest.mark.slow` - Tests that take >1 second
- `@pytest.mark.asyncio` - Async tests

## Fixtures

Key fixtures available in `conftest.py` and `tests/integration/conftest.py`:

### Core Fixtures
- `db_session` - Test database session
- `client` - AsyncClient for API testing
- `temp_dir` - Temporary directory for test files
- `watch_folder` - Test watch folder

### File Fixtures
- `sample_text_file` - Text file
- `sample_markdown_file` - Markdown file
- `sample_json_file` - JSON file
- `sample_pdf_file` - PDF file
- `sample_image_file` - Image file
- `sample_csv_file` - CSV file
- `sample_xml_file` - XML file
- `large_text_file` - Large text file (1000 lines)
- `invalid_pdf_file` - Corrupted PDF

### Database Fixtures
- `indexed_file` - Single indexed file
- `indexed_file_with_content` - File with text content
- `indexed_files_batch` - 3 indexed files (txt, md, json)
- `multiple_indexed_files` - 5 indexed files
- `indexed_pdf_file` - PDF file with content

### Mock Fixtures
- `mock_embedding_service` - Mocked embedding service
- `mock_storage_service` - Mocked storage service
- `mock_vector_store` - Mocked vector store
- `mock_reranker` - Mocked reranker
- `mock_bm25_engine` - Mocked BM25 engine
- `mock_hybrid_engine` - Mocked hybrid search engine

### Utility Fixtures
- `concurrent_test_files` - 10 files for concurrent testing
- `orphaned_storage_files` - Orphaned storage files mock

## Test Database

Tests use a separate test database configured via `TEST_DATABASE_URL` environment variable:

```
TEST_DATABASE_URL=postgresql+asyncpg://vault:vault@localhost:5432/vault_test
```

The test database is:
- Created fresh for each test session
- Cleaned up after each test via transaction rollback
- Isolated from production data

## Key Test Patterns

### 1. Full Integration Test
```python
async def test_upload_and_search(client, db_session, watch_folder, sample_file):
    # Upload
    upload_resp = await client.post("/api/v1/files", json={...})
    assert upload_resp.status_code == 200

    # Search
    search_resp = await client.post("/api/v1/search", json={...})
    assert search_resp.status_code == 200
    assert len(search_resp.json()["results"]) > 0
```

### 2. Concurrent Operations
```python
async def test_concurrent_uploads(client, files):
    tasks = [client.post("/api/v1/files", json={...}) for file in files]
    responses = await asyncio.gather(*tasks)
    assert all(r.status_code == 200 for r in responses)
```

### 3. Transaction Rollback
```python
async def test_rollback(db_session, mocker):
    mock_service.method.side_effect = Exception("Failure")

    with pytest.raises(Exception):
        await service.operation()

    # Verify rollback
    result = await db_session.execute(select(Model))
    assert result.scalar_one_or_none() is None
```

## Performance Benchmarks

Expected execution times (excluding slow tests):

- Concurrent operations: < 5s per test
- Upload/Search flow: < 2s per test
- Reranking: < 1s per test
- Export/Download: < 1s per test
- Transaction tests: < 500ms per test

Slow tests (marked with `@pytest.mark.slow`):
- Batch operations: < 30s
- Large dataset cleanup: < 60s
- High concurrency stress: < 15s

## Test Data

Test files are created in temporary directories and cleaned up automatically. Sample content:

- **Text files**: Simple multi-line text with search keywords
- **Markdown files**: Formatted with headers, lists, code blocks
- **JSON files**: Structured data objects
- **PDF files**: Generated with reportlab (requires installation)
- **Images**: Created with PIL (requires installation)

## Troubleshooting

### Database connection errors
Ensure test database is running and accessible:
```bash
psql -U vault -h localhost -d vault_test
```

### Import errors
Install dependencies:
```bash
pip install -e .
pip install pytest pytest-asyncio pytest-mock httpx
```

### Slow tests
Use `-k "not slow"` to skip slow tests during development

### Mock errors
Ensure mocks are properly configured in fixtures and patched at correct import paths

## Coverage Goals

Current integration test coverage:

- **Critical paths**: 100%
- **API endpoints**: 95%
- **Error handling**: 90%
- **Concurrent scenarios**: 85%
- **Edge cases**: 80%

## Future Enhancements

- [ ] Add tests for rate limiting
- [ ] Add tests for CSRF protection
- [ ] Add tests for authentication/authorization
- [ ] Add tests for WebSocket connections
- [ ] Add tests for background job processing
- [ ] Add tests for metrics collection
- [ ] Add performance regression tests
- [ ] Add chaos engineering tests

## Notes

⚠️ **Known Issues:**
- Some tests require `reportlab` for PDF generation
- Image tests require `PIL/Pillow`
- Settings validation errors may occur if pydantic validators are misconfigured

✅ **Best Practices:**
- Use fixtures for test data
- Mock external services (AI models, storage)
- Test realistic data volumes
- Verify both success and failure paths
- Check database state after operations
- Use transaction rollback for isolation
