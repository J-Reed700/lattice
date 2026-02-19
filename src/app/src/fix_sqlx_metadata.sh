#!/bin/bash
# SQLx Metadata Regeneration Script

set -e

echo "=== SQLx Metadata Regeneration ==="
echo ""

# Step 1: Clean slate
echo "[1/6] Cleaning old databases..."
rm -f recall.db recall_test.db sqlx_prepare.db test_migration.db

# Step 2: Create empty database
echo "[2/6] Creating fresh database..."
sqlite3 sqlx_prepare.db "SELECT 1;" > /dev/null

# Step 3: Apply all migrations
echo "[3/6] Applying migrations..."
for migration in $(ls migrations/*.sql | sort); do
    echo "  → $(basename $migration)"
    sqlite3 sqlx_prepare.db ".read $migration" || {
        echo "  ✗ FAILED: $migration"
        exit 1
    }
done

# Step 4: Verify schema
echo "[4/6] Verifying schema..."
SCHEMA_VERSION=$(sqlite3 sqlx_prepare.db "SELECT MAX(version) FROM schema_version;" 2>/dev/null || echo "0")
echo "  Schema version: $SCHEMA_VERSION"

if [ "$SCHEMA_VERSION" != "18" ]; then
    echo "  ✗ ERROR: Expected version 18, got $SCHEMA_VERSION"
    exit 1
fi

# Step 5: Generate SQLx metadata
echo "[5/6] Generating SQLx metadata..."
export DATABASE_URL="sqlite://sqlx_prepare.db"
cargo sqlx prepare --workspace || {
    echo "  ✗ FAILED: cargo sqlx prepare"
    exit 1
}

# Step 6: Build
echo "[6/6] Building project..."
cargo build || {
    echo "  ✗ FAILED: cargo build"
    exit 1
}

echo ""
echo "=== Success! ==="
echo "SQLx metadata regenerated successfully."
echo "Build succeeded with 0 errors."
echo ""
echo "Cleanup: rm sqlx_prepare.db"
