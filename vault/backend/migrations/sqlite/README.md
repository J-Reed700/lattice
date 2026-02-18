# Vault SQLite Schema

Complete database schema for Vault with automatic topic discovery and multi-modal vector search.

## Overview

This schema is designed for:
- **100K-500K documents per topic**
- **Automatic topic discovery** via clustering
- **Multi-modal embeddings** (text, image, audio)
- **Hybrid search** (vector + FTS5)
- **Incremental indexing** with change detection
- **ACID compliance** with WAL mode

## File Structure

```
migrations/sqlite/
├── 001_initial_schema.sql    # Complete schema definition
├── run_migration.py           # Migration runner script
├── QUERIES.md                 # Query examples and patterns
└── README.md                  # This file
```

## Quick Start

```bash
# Install sqlite-vec extension first
# See: https://github.com/asg017/sqlite-vec

# Run initial migration
python run_migration.py vault.db

# Verify schema
sqlite3 vault.db ".schema"
```

## Schema Tables

### Core Tables
- **documents**: File metadata, content, topic assignment
- **embeddings**: Vector embeddings (text/image/audio)
- **topics**: Auto-discovered clusters with centroids
- **file_metadata**: Change detection for incremental indexing

### Vector Search (sqlite-vec)
- **vec_text_embeddings**: 768d text vectors
- **vec_image_embeddings**: 768d image vectors  
- **vec_audio_embeddings**: 768d audio vectors
- **vec_topic_centroids**: Topic similarity search

### Full-Text Search (FTS5)
- **fts_documents**: Document keyword search
- **fts_chunks**: Chunk-level search
- **fts_topics**: Topic keyword search

## Performance

- Within-topic vector search: <50ms (100K-500K docs)
- Full-text search: <20ms
- Hybrid search: <100ms
- Topic filtering: 10-50x speedup

## Key Features

1. **Automatic topic discovery** - Documents clustered by content
2. **Multi-modal search** - Text, image, audio in one system
3. **Hybrid search** - Combine vector + keyword matching
4. **Incremental indexing** - Only process changed files
5. **ACID transactions** - Data integrity guaranteed

## Documentation

- See QUERIES.md for usage examples
- See 001_initial_schema.sql for full schema
- See run_migration.py for migration tools
