# HNSW Index Optimization - Deployment Guide

## Migration Overview

Three migrations optimize the Recall vector database:

1. **002_optimize_hnsw_indexes.sql** - Tunes HNSW parameters
2. **003_enable_image_chunking.sql** - Enables chunk-level indexing
3. **HNSW_QUERY_OPTIMIZATION.md** - Runtime query optimization

## Pre-Deployment Checklist

### Check Current State

```sql
-- Check table sizes
SELECT tablename, pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) AS size
FROM pg_stat_user_tables
WHERE tablename IN ('text_embeddings', 'image_embeddings');

-- Verify no duplicate image_id values
SELECT image_id, COUNT(*) FROM image_embeddings GROUP BY image_id HAVING COUNT(*) > 1;
```

### Estimate Downtime

- **Migration 002**: 1-2 min per 100k text embeddings, 45-90 sec per 100k images
- **Migration 003**: <1 second
- **Example**: 500k text + 200k images = ~12-18 minutes

## Deployment Steps

### Standard Deployment

```bash
psql -U vault_user -d vault_db

\i vault/backend/migrations/versions/002_optimize_hnsw_indexes.sql
\i vault/backend/migrations/versions/003_enable_image_chunking.sql
```

### Zero-Downtime (>1M vectors)

```sql
-- Drop old indexes
DROP INDEX IF EXISTS idx_text_embeddings_vector;
DROP INDEX IF EXISTS idx_image_embeddings_vector;

-- Build concurrently (no write locks)
CREATE INDEX CONCURRENTLY idx_text_embeddings_vector ON text_embeddings
    USING hnsw (embedding vector_cosine_ops)
    WITH (m = 24, ef_construction = 200);

CREATE INDEX CONCURRENTLY idx_image_embeddings_vector ON image_embeddings
    USING hnsw (embedding vector_cosine_ops)
    WITH (m = 16, ef_construction = 200);
```

## Post-Deployment Verification

```sql
-- Verify index parameters
SELECT indexdef FROM pg_indexes WHERE indexname = 'idx_text_embeddings_vector';
-- Should show: WITH (m = 24, ef_construction = 200)

-- Test search performance
EXPLAIN ANALYZE
SELECT id FROM text_embeddings
ORDER BY embedding <=> '[0.1, 0.2, ...]'::vector
LIMIT 10;

-- Verify chunk_index column exists
SELECT column_name FROM information_schema.columns
WHERE table_name = 'image_embeddings' AND column_name = 'chunk_index';
```

## Expected Improvements

- **Recall**: 2-5% improvement at same ef_search values
- **Consistency**: More stable results across query patterns
- **Memory**: ~50% increase in text index size
- **Chunking**: Multiple embeddings per image now supported

## Rollback Procedures

### Rollback Migration 002

```sql
DROP INDEX IF EXISTS idx_text_embeddings_vector;
DROP INDEX IF EXISTS idx_image_embeddings_vector;

CREATE INDEX idx_text_embeddings_vector ON text_embeddings
    USING hnsw (embedding vector_cosine_ops)
    WITH (m = 16, ef_construction = 64);

CREATE INDEX idx_image_embeddings_vector ON image_embeddings
    USING hnsw (embedding vector_cosine_ops)
    WITH (m = 16, ef_construction = 64);
```

### Rollback Migration 003

**WARNING**: Cannot rollback if multiple chunks exist per image!

```sql
-- Check for duplicates first
SELECT image_id, COUNT(*) FROM image_embeddings GROUP BY image_id HAVING COUNT(*) > 1;

-- If none, proceed:
ALTER TABLE image_embeddings DROP CONSTRAINT image_embeddings_image_chunk_unique;
DROP INDEX idx_image_embeddings_image_chunk;
ALTER TABLE image_embeddings ADD CONSTRAINT image_embeddings_image_id_key UNIQUE (image_id);
CREATE INDEX idx_image_embeddings_image ON image_embeddings(image_id);
```

## Troubleshooting

### Index Build Hangs

```sql
-- Check progress
SELECT pid, query, state FROM pg_stat_activity WHERE query LIKE '%CREATE INDEX%';

-- Increase memory temporarily
SET maintenance_work_mem = '2GB';
```

### Out of Memory

```sql
-- Check current setting
SHOW maintenance_work_mem;

-- Increase for session
SET maintenance_work_mem = '4GB';
```

## Monitoring

```sql
-- Monitor query performance
CREATE EXTENSION IF NOT EXISTS pg_stat_statements;

SELECT LEFT(query, 80), calls, mean_exec_time
FROM pg_stat_statements
WHERE query LIKE '%embedding <=>%'
ORDER BY mean_exec_time DESC;

-- Track index usage
SELECT indexname, idx_scan, pg_size_pretty(pg_relation_size(indexname::regclass))
FROM pg_stat_user_indexes
WHERE indexname LIKE '%embedding%vector%';
```

## Next Steps

1. Review `HNSW_QUERY_OPTIMIZATION.md` for query tuning
2. Test with real queries to validate improvements
3. Adjust ef_search based on use case (60=fast, 100=balanced, 200=accurate)
4. Monitor memory usage and query performance

## Summary

✅ Migration 002: Better search quality (m=24 for text, ef_construction=200)
✅ Migration 003: Chunk-level image indexing enabled
✅ Query tuning: Runtime ef_search control for speed/accuracy balance
✅ Expected: 2-5% recall improvement, 50% more index memory
