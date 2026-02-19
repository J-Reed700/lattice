-- FTS5 Document-Level Implementation Verification Queries
-- Run these queries to verify the fix is working correctly

-- ============================================
-- 1. COUNT VERIFICATION
-- ============================================
-- Check: FTS5 entries should equal number of documents (not chunks)

SELECT 'FTS5 Entry Count' as test, COUNT(*) as result FROM documents_fts;
SELECT 'Unique Document Count' as test, COUNT(DISTINCT document_id) as result FROM text_chunks;
SELECT 'Total Chunk Count' as test, COUNT(*) as result FROM text_chunks;

-- Expected: First two queries should return same number
-- Expected: Third query should be much larger (if multi-chunk documents exist)


-- ============================================
-- 2. DUPLICATE CHECK
-- ============================================
-- Check: No document_id should appear more than once in FTS5

SELECT 
    'Duplicate Check' as test,
    document_id, 
    COUNT(*) as occurrences
FROM documents_fts
GROUP BY document_id
HAVING COUNT(*) > 1;

-- Expected: 0 rows returned (no duplicates)


-- ============================================
-- 3. SAMPLE DOCUMENT INSPECTION
-- ============================================
-- Check: View actual FTS5 entries to verify concatenation

SELECT 
    document_id,
    LENGTH(content) as content_length,
    SUBSTR(content, 1, 100) || '...' as content_preview
FROM documents_fts
LIMIT 5;

-- Expected: content_length should be larger for multi-chunk documents
-- Expected: content should contain concatenated chunks with spaces


-- ============================================
-- 4. CHUNK VS FTS5 COMPARISON
-- ============================================
-- Check: Compare chunk count vs FTS5 entries per document

SELECT 
    tc.document_id,
    COUNT(tc.id) as chunk_count,
    COUNT(DISTINCT fts.rowid) as fts_entry_count,
    CASE 
        WHEN COUNT(DISTINCT fts.rowid) = 1 THEN 'PASS'
        ELSE 'FAIL'
    END as status
FROM text_chunks tc
LEFT JOIN documents_fts fts ON tc.document_id = fts.document_id
GROUP BY tc.document_id
ORDER BY chunk_count DESC
LIMIT 10;

-- Expected: fts_entry_count should be 1 for all documents
-- Expected: status should be 'PASS' for all rows


-- ============================================
-- 5. BM25 SEARCH TEST
-- ============================================
-- Check: Search returns no duplicate document_ids

SELECT 
    document_id,
    bm25(documents_fts) as score,
    COUNT(*) OVER (PARTITION BY document_id) as id_occurrences
FROM documents_fts
WHERE documents_fts MATCH 'test OR example OR document'
ORDER BY score
LIMIT 20;

-- Expected: id_occurrences should be 1 for all rows
-- Expected: Each document_id appears only once


-- ============================================
-- 6. TRIGGER VERIFICATION
-- ============================================
-- Check: All expected triggers exist

SELECT 
    name,
    CASE 
        WHEN name IN ('text_chunks_ai', 'text_chunks_ai_update', 'text_chunks_ad', 'text_chunks_au') 
        THEN 'EXPECTED'
        ELSE 'UNEXPECTED'
    END as status
FROM sqlite_master
WHERE type = 'trigger' 
  AND tbl_name = 'text_chunks'
  AND name LIKE 'text_chunks_%';

-- Expected: 4 triggers (text_chunks_ai, text_chunks_ai_update, text_chunks_ad, text_chunks_au)
-- Expected: All marked as 'EXPECTED'


-- ============================================
-- 7. CONTENT CONCATENATION CHECK
-- ============================================
-- Check: FTS5 content includes all chunks in order

SELECT 
    tc.document_id,
    GROUP_CONCAT(tc.content, ' ') as expected_content,
    fts.content as actual_content,
    CASE 
        WHEN GROUP_CONCAT(tc.content, ' ') = fts.content THEN 'PASS'
        ELSE 'FAIL'
    END as match_status
FROM text_chunks tc
JOIN documents_fts fts ON tc.document_id = fts.document_id
GROUP BY tc.document_id
LIMIT 5;

-- Expected: match_status should be 'PASS' for all rows
-- Expected: expected_content matches actual_content


-- ============================================
-- 8. MULTI-CHUNK DOCUMENT TEST
-- ============================================
-- Check: Documents with multiple chunks have proper FTS5 entries

SELECT 
    tc.document_id,
    COUNT(tc.id) as chunk_count,
    COUNT(fts.rowid) as fts_entries,
    CASE 
        WHEN COUNT(tc.id) > 1 AND COUNT(fts.rowid) = COUNT(tc.id) THEN 'PASS (1 FTS entry per chunk)'
        WHEN COUNT(tc.id) > 1 AND COUNT(fts.rowid) = 1 THEN 'PASS (1 FTS entry per document)'
        WHEN COUNT(tc.id) = 1 AND COUNT(fts.rowid) = 1 THEN 'PASS (single chunk)'
        ELSE 'FAIL'
    END as status
FROM text_chunks tc
LEFT JOIN documents_fts fts ON tc.document_id = fts.document_id
GROUP BY tc.document_id
HAVING COUNT(tc.id) > 1
LIMIT 10;

-- Expected: status should show 'PASS (1 FTS entry per document)'
-- Expected: fts_entries should be 1 regardless of chunk_count


-- ============================================
-- 9. PERFORMANCE CHECK
-- ============================================
-- Check: Index size comparison (run EXPLAIN QUERY PLAN)

EXPLAIN QUERY PLAN
SELECT document_id, bm25(documents_fts) as score
FROM documents_fts
WHERE documents_fts MATCH 'search query'
ORDER BY score
LIMIT 10;

-- Expected: Should use FTS5 index efficiently
-- Expected: No temporary B-tree or sorting needed


-- ============================================
-- 10. EDGE CASE: EMPTY DOCUMENTS
-- ============================================
-- Check: Documents with no chunks have no FTS5 entries

SELECT 
    d.id as document_id,
    COUNT(tc.id) as chunk_count,
    COUNT(fts.rowid) as fts_entries,
    CASE 
        WHEN COUNT(tc.id) = 0 AND COUNT(fts.rowid) = 0 THEN 'PASS (no chunks, no FTS)'
        WHEN COUNT(tc.id) = 0 AND COUNT(fts.rowid) > 0 THEN 'FAIL (orphan FTS entry)'
        ELSE 'PASS (has chunks and FTS)'
    END as status
FROM documents d
LEFT JOIN text_chunks tc ON d.id = tc.document_id
LEFT JOIN documents_fts fts ON d.id = fts.document_id
GROUP BY d.id
HAVING COUNT(tc.id) = 0
LIMIT 10;

-- Expected: status should be 'PASS' for all rows
-- Expected: Documents with no chunks should have no FTS5 entries


-- ============================================
-- SUMMARY QUERY
-- ============================================

SELECT 
    'Total Documents' as metric,
    COUNT(DISTINCT id) as value
FROM documents
UNION ALL
SELECT 
    'Total Chunks' as metric,
    COUNT(*) as value
FROM text_chunks
UNION ALL
SELECT 
    'Total FTS5 Entries' as metric,
    COUNT(*) as value
FROM documents_fts
UNION ALL
SELECT 
    'Unique FTS5 document_ids' as metric,
    COUNT(DISTINCT document_id) as value
FROM documents_fts
UNION ALL
SELECT 
    'Expected Ratio (FTS5/Documents)' as metric,
    ROUND(CAST(COUNT(DISTINCT fts.document_id) AS FLOAT) / COUNT(DISTINCT d.id), 2) as value
FROM documents d
LEFT JOIN documents_fts fts ON d.id = fts.document_id;

-- Expected: 'Expected Ratio' should be close to 1.0
-- Expected: Total FTS5 Entries ≈ Unique FTS5 document_ids ≈ Total Documents
