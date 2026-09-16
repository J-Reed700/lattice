#!/bin/bash
set -e

echo "Testing migration system..."

# Create a temporary test database
TEST_DB="test_migrations.db"
rm -f "$TEST_DB" "$TEST_DB-wal" "$TEST_DB-shm"

echo "✓ Cleaned up any existing test database"

# Apply the canonical schema exactly as the app does (sqlx::migrate!("./migrations")).
sqlite3 "$TEST_DB" <<SQL
.bail on

PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;

.read migrations/20260916000000_init_schema.sql
SQL

echo "✓ Schema applied successfully"

# Verify recorded schema version
VERSION=$(sqlite3 "$TEST_DB" "SELECT MAX(version) FROM schema_version")
if [ "$VERSION" = "16" ]; then
    echo "✓ Schema version is correct: $VERSION"
else
    echo "✗ Schema version is incorrect: $VERSION (expected 16)"
    exit 1
fi

# Count tables
TABLES=$(sqlite3 "$TEST_DB" "SELECT COUNT(*) FROM sqlite_master WHERE type='table'")
echo "✓ Created $TABLES tables"

# Verify critical tables exist
CRITICAL_TABLES="documents text_chunks text_embeddings tags watch_folders files file_references chunks_fts documents_fts conversations conversation_search_fts favorites recent_documents batch_jobs batch_job_items web_assets download_sessions models model_files chunk_sparse_terms document_summaries schema_version"

for TABLE in $CRITICAL_TABLES; do
    EXISTS=$(sqlite3 "$TEST_DB" "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='$TABLE'")
    if [ "$EXISTS" = "1" ]; then
        echo "  ✓ $TABLE"
    else
        echo "  ✗ $TABLE (missing)"
        exit 1
    fi
done

# Verify seed rows survived the squash
for CHECK in "conversation_spaces:space_general" "collaborator_profiles:member_local_owner" "models:__ollama_server__"; do
    TABLE="${CHECK%%:*}"
    ID="${CHECK##*:}"
    EXISTS=$(sqlite3 "$TEST_DB" "SELECT COUNT(*) FROM $TABLE WHERE id = '$ID'")
    if [ "$EXISTS" = "1" ]; then
        echo "  ✓ seed $TABLE/$ID"
    else
        echo "  ✗ seed $TABLE/$ID (missing)"
        exit 1
    fi
done

echo ""
echo "✅ Migration system test PASSED"
echo ""

# Cleanup
rm -f "$TEST_DB" "$TEST_DB-wal" "$TEST_DB-shm"
