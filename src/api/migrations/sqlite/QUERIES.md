# Vault SQLite Query Examples

## Common Query Patterns

### 1. Vector Search Within Topic

```sql
-- Find similar documents within a specific topic using vector search
WITH query_embedding AS (
  SELECT ? as embedding  -- Your query embedding vector
)
SELECT 
  d.id,
  d.path,
  d.filename,
  d.content,
  t.name as topic_name,
  vec_distance(ve.embedding, query_embedding.embedding) as similarity
FROM vec_text_embeddings ve
JOIN embeddings e ON ve.embedding_id = e.id
JOIN documents d ON e.document_id = d.id
LEFT JOIN topics t ON d.topic_id = t.id
CROSS JOIN query_embedding
WHERE e.topic_id = ?  -- Filter by topic_id
  AND e.embedding_type = 'text'
ORDER BY similarity ASC
LIMIT 20;
```

### 2. Hybrid Search (Vector + FTS)

```sql
-- Combine semantic search with keyword matching
WITH vector_results AS (
  SELECT 
    d.id,
    d.path,
    d.filename,
    vec_distance(ve.embedding, ?) as vec_score
  FROM vec_text_embeddings ve
  JOIN embeddings e ON ve.embedding_id = e.id
  JOIN documents d ON e.document_id = d.id
  WHERE e.embedding_type = 'text'
  ORDER BY vec_score ASC
  LIMIT 100
),
fts_results AS (
  SELECT 
    document_id,
    rank as fts_score
  FROM fts_documents
  WHERE fts_documents MATCH ?  -- 'keyword query'
  ORDER BY rank
  LIMIT 100
)
SELECT 
  d.id,
  d.path,
  d.filename,
  d.content,
  COALESCE(vr.vec_score, 1.0) * 0.7 + COALESCE(fr.fts_score, 0) * 0.3 as hybrid_score
FROM documents d
LEFT JOIN vector_results vr ON d.id = vr.id
LEFT JOIN fts_results fr ON d.id = fr.document_id
WHERE vr.id IS NOT NULL OR fr.document_id IS NOT NULL
ORDER BY hybrid_score DESC
LIMIT 20;
```

### 3. Recent Documents by Topic

```sql
-- Get most recently modified documents in a topic
SELECT 
  d.id,
  d.path,
  d.filename,
  d.file_modified_at,
  d.size_bytes,
  t.name as topic_name
FROM documents d
JOIN topics t ON d.topic_id = t.id
WHERE t.id = ?
ORDER BY d.file_modified_at DESC
LIMIT 50;
```

### 4. Topic Document Count

```sql
-- Get all topics with their document counts (already maintained by triggers)
SELECT 
  id,
  name,
  document_count,
  silhouette_score,
  keywords_json
FROM topics
WHERE document_count > 0
ORDER BY document_count DESC;
```

### 5. Find Similar Topics

```sql
-- Find topics similar to a given topic using centroid vectors
WITH target_topic AS (
  SELECT centroid FROM vec_topic_centroids WHERE topic_id = ?
)
SELECT 
  t.id,
  t.name,
  t.document_count,
  t.keywords_json,
  vec_distance(vtc.centroid, target_topic.centroid) as similarity
FROM vec_topic_centroids vtc
JOIN topics t ON vtc.topic_id = t.id
CROSS JOIN target_topic
WHERE t.id != ?  -- Exclude the target topic itself
ORDER BY similarity ASC
LIMIT 10;
```

### 6. Documents Needing Indexing

```sql
-- Find documents that need (re)indexing using the view
SELECT 
  id,
  path,
  filename,
  index_status,
  index_error,
  needs_reindex
FROM v_documents_pending_index
ORDER BY file_modified_at DESC
LIMIT 100;
```

### 7. Search Within Topic by Keyword

```sql
-- Full-text search within a specific topic
SELECT 
  d.id,
  d.path,
  d.filename,
  d.content,
  fts.rank as relevance
FROM fts_documents fts
JOIN documents d ON fts.document_id = d.id
WHERE fts.fts_documents MATCH ?  -- 'search query'
  AND d.topic_id = ?
ORDER BY fts.rank
LIMIT 20;
```

### 8. Topic Statistics

```sql
-- Get comprehensive topic statistics using the view
SELECT * FROM v_topic_stats
ORDER BY document_count DESC;
```

### 9. Find Documents by Path Pattern

```sql
-- Find documents matching a path pattern (uses index)
SELECT 
  d.id,
  d.path,
  d.filename,
  d.size_bytes,
  t.name as topic_name
FROM documents d
LEFT JOIN topics t ON d.topic_id = t.id
WHERE d.path LIKE '/home/user/Documents%'
ORDER BY d.filename;
```

### 10. Documents by Extension in Topic

```sql
-- Get all PDFs in a specific topic (uses indexes)
SELECT 
  id,
  path,
  filename,
  size_bytes,
  file_modified_at
FROM documents
WHERE topic_id = ?
  AND extension = 'pdf'
ORDER BY file_modified_at DESC;
```

### 11. Multi-Modal Search

```sql
-- Search across text, image, and audio embeddings
WITH query_embedding AS (SELECT ? as embedding)
SELECT 
  d.id,
  d.path,
  d.filename,
  d.mime_type,
  e.embedding_type,
  vec_distance(ve.embedding, qe.embedding) as similarity
FROM embeddings e
JOIN documents d ON e.document_id = d.id
LEFT JOIN vec_text_embeddings ve ON ve.embedding_id = e.id AND e.embedding_type = 'text'
LEFT JOIN vec_image_embeddings ve ON ve.embedding_id = e.id AND e.embedding_type = 'image'
LEFT JOIN vec_audio_embeddings ve ON ve.embedding_id = e.id AND e.embedding_type = 'audio'
CROSS JOIN query_embedding qe
WHERE d.topic_id = ?  -- Optional topic filter
ORDER BY similarity ASC
LIMIT 20;
```

### 12. Batch Insert Embeddings

```sql
-- Insert document embedding and sync to vector table (transaction)
BEGIN TRANSACTION;

-- Insert embedding record
INSERT INTO embeddings (
  document_id, embedding_type, chunk_index, chunk_text,
  model_name, model_version, dimension, topic_id
) VALUES (?, 'text', 0, ?, 'bge-base-en-v1.5', '1.0', 768, ?);

-- Insert into vector table
INSERT INTO vec_text_embeddings (embedding_id, embedding)
VALUES (last_insert_rowid(), ?);  -- ? is the 768-dim vector

COMMIT;
```

### 13. Update Topic Assignment

```sql
-- Reassign documents to a new topic (trigger handles counts)
UPDATE documents 
SET topic_id = ?
WHERE id IN (
  SELECT id FROM documents WHERE topic_id = 'uncategorized' LIMIT 100
);
```

### 14. Search History Analytics

```sql
-- Get most popular searches
SELECT 
  query,
  COUNT(*) as search_count,
  AVG(execution_time_ms) as avg_time_ms,
  AVG(result_count) as avg_results
FROM search_history
WHERE created_at > unixepoch() - 86400 * 30  -- Last 30 days
GROUP BY query
ORDER BY search_count DESC
LIMIT 20;
```

### 15. Documents with Tags in Topic

```sql
-- Get documents with specific tags within a topic
SELECT 
  d.id,
  d.path,
  d.filename,
  t.name as topic_name,
  GROUP_CONCAT(tag.name, ', ') as tags
FROM documents d
JOIN topics t ON d.topic_id = t.id
LEFT JOIN document_tags dt ON d.id = dt.document_id
LEFT JOIN tags tag ON dt.tag_id = tag.id
WHERE d.topic_id = ?
  AND tag.name IN ('work', 'important')  -- Tag filter
GROUP BY d.id
ORDER BY d.file_modified_at DESC;
```

## Performance Notes

1. **Vector Search Performance**: 
   - sqlite-vec uses approximate nearest neighbor (ANN) for large datasets
   - Filtering by topic_id before vector search is highly efficient
   - Expect <50ms for searches within topics of 100K-500K documents

2. **Index Usage**:
   - `idx_documents_topic` - Used for topic filtering (partial index)
   - `idx_embeddings_topic` - Used for topic-filtered embedding queries
   - `idx_documents_modified_at` - Used for recency queries
   - `idx_documents_path` - Used for path lookups

3. **FTS5 Performance**:
   - Porter stemming enabled for better recall
   - Unicode61 tokenizer for international text
   - Expect <20ms for keyword searches

4. **Hybrid Search**:
   - Combines both vector and FTS results
   - Weight vector score higher (0.7) vs FTS (0.3)
   - Pre-filter both result sets to top 100 before combining

5. **Topic Filtering**:
   - Always filter by topic_id when possible
   - Reduces search space dramatically (100K→10K docs typical)
   - Partial indexes make WHERE topic_id IS NOT NULL very fast

## Optimization Tips

1. **Use STRICT tables** - Already enabled, catches type errors early
2. **Analyze regularly** - Run `ANALYZE` after bulk inserts
3. **Vacuum periodically** - Reclaim space after large deletes
4. **Use transactions** - Batch inserts in transactions (10-100 rows)
5. **Prepared statements** - Reuse compiled queries for repeated operations
