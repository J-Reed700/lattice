# FTS5 Document-Level Trigger Edge Case Analysis

## Edge Case 1: Empty Documents (No Chunks)
**Scenario:** Document exists but has no text chunks
**Behavior:** 
- No INSERT trigger fires (no chunks to insert)
- No FTS5 entry created
- BM25 won't return this document (correct - no searchable content)
**Status:** HANDLED ✓

## Edge Case 2: Single Chunk Document
**Scenario:** Document has exactly 1 chunk
**Behavior:**
- `text_chunks_ai` trigger fires (WHEN COUNT = 1)
- Creates single FTS5 entry with chunk content
- BM25 returns document once (correct)
**Status:** HANDLED ✓

## Edge Case 3: Multi-Chunk Document (2-100+ chunks)
**Scenario:** Document has multiple chunks inserted sequentially
**Behavior:**
- First chunk: `text_chunks_ai` fires, creates FTS5 entry
- Second+ chunk: `text_chunks_ai_update` fires (WHEN COUNT > 1)
  - Deletes existing FTS5 entry
  - Re-inserts with all chunks concatenated via GROUP_CONCAT
- Final state: Single FTS5 entry with all content
**Status:** HANDLED ✓

## Edge Case 4: Very Large Document (100+ chunks)
**Scenario:** Document with 100+ chunks
**Considerations:**
- SQLite GROUP_CONCAT has 1GB limit (plenty for text)
- Multiple DELETE/INSERT operations during batch insert
- Performance impact: Each chunk after first triggers full reconstruction
**Optimization Needed:** Consider batch insert detection or disable triggers during bulk operations
**Status:** FUNCTIONAL but may have performance impact

## Edge Case 5: Chunk Deletion
**Scenario:** Delete a chunk from multi-chunk document
**Behavior:**
- `text_chunks_ad` trigger fires
- Deletes existing FTS5 entry
- Re-inserts with remaining chunks concatenated
- If last chunk deleted, no re-insert (document_id won't exist in subquery)
**Status:** HANDLED ✓

## Edge Case 6: Chunk Update
**Scenario:** Update chunk content
**Behavior:**
- `text_chunks_au` trigger fires
- Deletes existing FTS5 entry
- Re-inserts with updated content concatenated
**Status:** HANDLED ✓

## Edge Case 7: Document with Single Chunk Deleted
**Scenario:** Delete the only chunk of a document
**Behavior:**
- `text_chunks_ad` trigger fires
- Deletes FTS5 entry
- No re-insert (subquery returns no rows)
**Status:** HANDLED ✓

## Performance Analysis

### Before (Chunk-Level):
- Document with 50 chunks → 50 FTS5 entries
- BM25 query returns 50 duplicates, GROUP BY required
- Index size: 50x bloat
- Query time: Slower due to GROUP BY and larger index

### After (Document-Level):
- Document with 50 chunks → 1 FTS5 entry
- BM25 query returns 1 result per document
- Index size: 50x smaller
- Query time: Faster (no GROUP BY needed, smaller index)

### Trade-off:
- Insert/Update operations are more expensive (DELETE + re-INSERT with GROUP_CONCAT)
- But this is acceptable because:
  1. Inserts are rare compared to searches
  2. Performance cost is during indexing, not querying
  3. Search quality and correctness are more important

## Migration Path
1. Run `migrate_to_document_level_fts()` on existing databases
2. Drops old triggers
3. Clears FTS5 table
4. Rebuilds with one entry per document
5. Creates new triggers

## Success Criteria Verification
✓ One FTS5 entry per document (not per chunk)
✓ BM25 returns unique document_ids (no duplicates) 
✓ Triggers handle document CRUD operations
✓ Content concatenation preserves search quality
✓ Edge cases properly handled
