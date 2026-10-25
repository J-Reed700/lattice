#!/bin/bash
set -euo pipefail

echo "Testing migration system..."

# Resolve migrations independently of the caller's working directory.
TAURI_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEST_DB="$(mktemp "${TMPDIR:-/tmp}/lattice-migrations.XXXXXX")"
trap 'rm -f "$TEST_DB" "$TEST_DB-wal" "$TEST_DB-shm"' EXIT

# Keep WAL enabled as it is for the application database.
sqlite3 "$TEST_DB" "PRAGMA journal_mode = WAL;"

# Apply every migration in timestamp order. SQLx runs each migration in its own
# transaction, and foreign-key enforcement must be enabled before that begins.
for MIGRATION in "$TAURI_DIR"/migrations/*.sql; do
    sqlite3 "$TEST_DB" <<SQL
.bail on
PRAGMA foreign_keys = ON;
BEGIN;
.read "$MIGRATION"
COMMIT;
SQL
done

echo "✓ Ordered migration chain applied successfully"

# Verify the baseline schema version survives the complete migration chain.
EXPECTED_VERSION=16
VERSION=$(sqlite3 "$TEST_DB" "SELECT MAX(version) FROM schema_version")
if [ "$VERSION" = "$EXPECTED_VERSION" ]; then
    echo "✓ Schema version is correct: $VERSION"
else
    echo "✗ Schema version is incorrect: $VERSION (expected $EXPECTED_VERSION)"
    exit 1
fi

FK_ERRORS=$(sqlite3 "$TEST_DB" "PRAGMA foreign_key_check")
if [ -n "$FK_ERRORS" ]; then
    echo "✗ Foreign key check failed: $FK_ERRORS"
    exit 1
fi
INTEGRITY=$(sqlite3 "$TEST_DB" "PRAGMA integrity_check")
if [ "$INTEGRITY" != "ok" ]; then
    echo "✗ Integrity check failed: $INTEGRITY"
    exit 1
fi
echo "✓ Foreign key and integrity checks passed"

# Count tables
TABLES=$(sqlite3 "$TEST_DB" "SELECT COUNT(*) FROM sqlite_master WHERE type='table'")
echo "✓ Created $TABLES tables"

# Verify critical tables exist
CRITICAL_TABLES="documents text_chunks text_embeddings tags watch_folders passage_references embedding_generation_vectors vector_index_state chunks_fts documents_fts conversations conversation_search_fts favorites recent_documents batch_import_items web_assets download_sessions models model_files document_summaries schema_version"

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
