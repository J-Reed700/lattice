#!/bin/bash
# Update SQLX Offline Cache
#
# This script updates the SQLX offline query cache (.sqlx/ directory)
# to ensure CI/CD pipelines can build without a live database connection.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

echo "========================================="
echo "SQLX Offline Cache Update Script"
echo "========================================="
echo ""

# Step 1: Check migration source
echo "Step 1: Checking migration source..."
if [ ! -d "migrations" ]; then
    echo "❌ ERROR: migrations directory not found"
    exit 1
fi

echo "✓ Migration chain found"
echo ""

# Step 2: Build an isolated database from the runtime migration chain.
echo "Step 2: Creating temporary migrated database..."
SQLX_PREPARE_DIR="$(mktemp -d)"
trap 'rm -rf "$SQLX_PREPARE_DIR"' EXIT
export DATABASE_URL="sqlite://$SQLX_PREPARE_DIR/sqlx_prepare.db"
export SQLX_OFFLINE="false"
touch "$SQLX_PREPARE_DIR/sqlx_prepare.db"
cargo sqlx migrate run --source migrations
echo "✓ Applied migrations to temporary database"
echo ""

# Step 3: Verify compilation works
echo "Step 3: Checking if project compiles..."
echo "NOTE: The project must compile without errors before SQLX cache can be updated"
echo ""

if ! cargo check --lib; then
    echo ""
    echo "❌ ERROR: Project has compilation errors"
    echo ""
    echo "SQLX cache cannot be updated until compilation errors are fixed."
    echo "Please fix the following types of errors first:"
    echo "  - Type mismatches (especially in repository trait implementations)"
    echo "  - Borrow checker errors (&self vs &mut self)"
    echo "  - Send/Sync trait issues in async functions"
    echo ""
    echo "Common fixes:"
    echo "  1. Ensure repository trait methods match their implementations"
    echo "  2. Fix &self vs &mut self in trait definitions"
    echo "  3. Ensure async traits are properly defined with #[async_trait]"
    echo ""
    exit 1
fi

echo "✓ Project compiles successfully"
echo ""

# Step 4: Run cargo sqlx prepare
echo "Step 4: Updating SQLX offline cache..."
echo "Running: cargo sqlx prepare"
echo ""

if cargo sqlx prepare -- --lib; then
    echo ""
    echo "========================================="
    echo "✓ SUCCESS: SQLX cache updated"
    echo "========================================="
    echo ""
    echo "The .sqlx/ directory has been updated with query metadata."
    echo "This allows CI/CD to build without a live database."
    echo ""
    echo "Next steps:"
    echo "  1. Review changes: git diff .sqlx/"
    echo "  2. Commit the updated cache: git add .sqlx/"
    echo "  3. Verify CI/CD builds work"
    echo ""
else
    echo ""
    echo "❌ ERROR: cargo sqlx prepare failed"
    echo ""
    echo "This usually means:"
    echo "  1. Database schema doesn't match queries"
    echo "  2. New queries were added but tables don't exist"
    echo "  3. SQL syntax errors in query macros"
    echo ""
    echo "To debug:"
    echo "  1. Check error output above"
    echo "  2. Verify the migration chain runs from an empty database"
    echo "  3. Confirm new query DTOs match their selected columns"
    echo ""
    exit 1
fi
