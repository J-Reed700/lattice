#!/bin/bash
set -e

echo "Testing migration system..."

# Create a temporary test database
TEST_DB="test_migrations.db"
rm -f "$TEST_DB" "$TEST_DB-wal" "$TEST_DB-shm"

echo "✓ Cleaned up any existing test database"

# Create database with migrations using SQLite directly
sqlite3 "$TEST_DB" <<EOF
.bail on

-- Apply pragmas
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;

-- Source all migrations in order
.read migrations/001_core_schema.sql
.read migrations/002_fts5_search.sql
.read migrations/003_file_storage.sql
.read migrations/004_web_metadata.sql
.read migrations/005_conversations.sql
.read migrations/006_mentions.sql
.read migrations/007_favorites.sql
.read migrations/008_recent_documents.sql
.read migrations/009_batch_jobs.sql
.read migrations/010_web_assets.sql
.read migrations/011_rich_metadata.sql
.read migrations/012_chunk_metadata.sql
.read migrations/013_download_sessions.sql
.read migrations/014_downloaded_models.sql
.read migrations/015_performance_indexes.sql

-- Verify schema version
SELECT 'Current schema version: ' || MAX(version) FROM schema_version;

-- Verify tables exist
SELECT 'Tables created: ' || COUNT(*) FROM sqlite_master WHERE type='table';

-- List all tables
SELECT 'Table list:';
SELECT name FROM sqlite_master WHERE type='table' ORDER BY name;

EOF

echo "✓ All migrations applied successfully"

# Verify final schema version
VERSION=$(sqlite3 "$TEST_DB" "SELECT MAX(version) FROM schema_version")
if [ "$VERSION" = "15" ]; then
    echo "✓ Schema version is correct: $VERSION"
else
    echo "✗ Schema version is incorrect: $VERSION (expected 15)"
    exit 1
fi

# Count tables
TABLES=$(sqlite3 "$TEST_DB" "SELECT COUNT(*) FROM sqlite_master WHERE type='table'")
echo "✓ Created $TABLES tables"

# Verify critical tables exist
CRITICAL_TABLES="documents text_chunks text_embeddings tags watch_folders files file_references documents_fts conversations favorites recent_documents batch_jobs batch_job_items web_assets download_sessions downloaded_models schema_version"

for TABLE in $CRITICAL_TABLES; do
    EXISTS=$(sqlite3 "$TEST_DB" "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='$TABLE'")
    if [ "$EXISTS" = "1" ]; then
        echo "  ✓ $TABLE"
    else
        echo "  ✗ $TABLE (missing)"
        exit 1
    fi
done

echo ""
echo "✅ Migration system test PASSED"
echo ""

# Cleanup
rm -f "$TEST_DB" "$TEST_DB-wal" "$TEST_DB-shm"
