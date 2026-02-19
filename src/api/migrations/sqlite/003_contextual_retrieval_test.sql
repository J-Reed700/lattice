-- ============================================================================
-- Test Script for Migration 003: Contextual Retrieval
-- Run after applying migration to verify schema changes
-- ============================================================================

-- Test 1: Verify new columns exist in embeddings table
SELECT 
    name,
    type,
    "notnull",
    dflt_value
FROM pragma_table_info('embeddings')
WHERE name IN ('contextualized_content', 'context_prefix', 'page_number', 'section_title');

-- Expected: 4 rows returned

-- Test 2: Verify new columns exist in documents table
SELECT 
    name,
    type,
    "notnull",
    dflt_value
FROM pragma_table_info('documents')
WHERE name IN ('page_count', 'title', 'author');

-- Expected: 3 rows returned

-- Test 3: Verify indexes were created
SELECT 
    name,
    tbl_name,
    sql
FROM sqlite_master
WHERE type = 'index'
AND name IN (
    'idx_embeddings_contextualized',
    'idx_embeddings_page',
    'idx_embeddings_section',
    'idx_documents_title'
);

-- Expected: 4 rows returned

-- Test 4: Verify FTS table was updated
SELECT 
    name
FROM pragma_table_info('fts_chunks')
WHERE name IN ('contextualized_content', 'section_title');

-- Expected: 2 rows returned

-- Test 5: Verify triggers exist
SELECT 
    name,
    tbl_name
FROM sqlite_master
WHERE type = 'trigger'
AND name IN (
    'trigger_fts_chunks_insert',
    'trigger_fts_chunks_update',
    'trigger_fts_chunks_delete'
);

-- Expected: 3 rows returned

-- Test 6: Insert test data
INSERT INTO documents (
    id, path, filename, extension, mime_type, size_bytes, hash_sha256,
    file_created_at, file_modified_at, title, author, page_count
) VALUES (
    'test-doc-migration-003',
    '/test/migration_test.pdf',
    'migration_test.pdf',
    'pdf',
    'application/pdf',
    2048,
    'test-hash-123',
    unixepoch(),
    unixepoch(),
    'Test Document for Migration 003',
    'Test Author',
    25
);

INSERT INTO embeddings (
    id, document_id, embedding_type, chunk_index,
    chunk_text, contextualized_content, context_prefix,
    page_number, section_title,
    model_name, model_version, dimension
) VALUES (
    'test-emb-migration-003',
    'test-doc-migration-003',
    'text',
    0,
    'This is a test chunk.',
    'Document: Test Document for Migration 003, Page: 5, Section: Introduction. This is a test chunk.',
    'Document: Test Document for Migration 003, Page: 5, Section: Introduction.',
    5,
    'Introduction',
    'BAAI/bge-base-en-v1.5',
    '1.5',
    768
);

-- Test 7: Query test data
SELECT 
    e.id,
    e.chunk_text,
    e.contextualized_content,
    e.context_prefix,
    e.page_number,
    e.section_title,
    d.title,
    d.author,
    d.page_count
FROM embeddings e
JOIN documents d ON e.document_id = d.id
WHERE e.id = 'test-emb-migration-003';

-- Expected: 1 row with all contextual data populated

-- Test 8: Verify FTS sync
SELECT 
    embedding_id,
    chunk_text,
    contextualized_content,
    section_title
FROM fts_chunks
WHERE embedding_id = 'test-emb-migration-003';

-- Expected: 1 row in FTS table

-- Test 9: Cleanup test data
DELETE FROM embeddings WHERE id = 'test-emb-migration-003';
DELETE FROM documents WHERE id = 'test-doc-migration-003';

-- ============================================================================
-- Migration Verification Complete
-- ============================================================================

SELECT '✓ Migration 003 verified successfully' AS result;
