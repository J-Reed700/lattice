#!/bin/bash
# Update SQLX Offline Cache
#
# This script updates the SQLX offline query cache (.sqlx/ directory)
# to ensure CI/CD pipelines can build without a live database connection.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

echo "========================================="
echo "SQLX Offline Cache Update Script"
echo "========================================="
echo ""

# Step 1: Check for database setup
echo "Step 1: Checking database setup..."

if [ ! -f "sqlx_prepare.db" ]; then
    echo "❌ ERROR: sqlx_prepare.db not found"
    echo ""
    echo "Please ensure the database is set up first:"
    echo "  1. The migrations are in: migrations/20250101000000_init_schema.sql"
    echo "  2. Create database: sqlite3 sqlx_prepare.db < migrations/20250101000000_init_schema.sql"
    echo "  3. Add conversation tables from init_schema.sql"
    echo ""
    exit 1
fi

if [ ! -d "migrations" ]; then
    echo "❌ ERROR: migrations directory not found"
    exit 1
fi

echo "✓ Database setup found"
echo ""

# Step 2: Set environment variables
echo "Step 2: Setting environment variables..."
export DATABASE_URL="sqlite://sqlx_prepare.db"
export SQLX_OFFLINE="false"
echo "✓ DATABASE_URL=$DATABASE_URL"
echo "✓ SQLX_OFFLINE=$SQLX_OFFLINE"
echo ""

# Step 3: Verify compilation works
echo "Step 3: Checking if project compiles..."
echo "NOTE: The project must compile without errors before SQLX cache can be updated"
echo ""

if ! cargo check --lib 2>&1 | tail -20; then
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

if cargo sqlx prepare; then
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
    echo "  2. Verify all tables exist: sqlite3 sqlx_prepare.db '.tables'"
    echo "  3. Run migrations if needed"
    echo ""
    exit 1
fi
