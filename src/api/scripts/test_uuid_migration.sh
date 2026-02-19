#!/bin/bash
# Test UUID migration against sample data

set -e

echo "🧪 UUID Migration Test Suite"
echo "================================"

# Configuration
TEST_DB_NAME="vault_test_uuid_migration"
POSTGRES_USER="${POSTGRES_USER:-vault}"
POSTGRES_PASSWORD="${POSTGRES_PASSWORD:-vault}"
POSTGRES_HOST="${POSTGRES_HOST:-localhost}"
POSTGRES_PORT="${POSTGRES_PORT:-5432}"

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo ""
echo -e "${YELLOW}📦 Step 1: Create test database${NC}"
PGPASSWORD=$POSTGRES_PASSWORD psql -U $POSTGRES_USER -h $POSTGRES_HOST -p $POSTGRES_PORT -c "DROP DATABASE IF EXISTS $TEST_DB_NAME;" postgres || true
PGPASSWORD=$POSTGRES_PASSWORD psql -U $POSTGRES_USER -h $POSTGRES_HOST -p $POSTGRES_PORT -c "CREATE DATABASE $TEST_DB_NAME;" postgres

echo ""
echo -e "${YELLOW}📦 Step 2: Apply base schema (before migration)${NC}"
export DATABASE_URL="postgresql+asyncpg://$POSTGRES_USER:$POSTGRES_PASSWORD@$POSTGRES_HOST:$POSTGRES_PORT/$TEST_DB_NAME"

# Get the revision hash before the UUID migration
UUID_MIGRATION_REVISION="20251120_migrate_uuids"
BEFORE_REVISION="20251117_add_user_mfa"

echo "   Upgrading to revision: $BEFORE_REVISION"
cd /home/user/Recall/src/api
poetry run alembic upgrade $BEFORE_REVISION

echo ""
echo -e "${YELLOW}📦 Step 3: Seed test data${NC}"
poetry run python scripts/seed_test_data.py

echo ""
echo -e "${YELLOW}📊 Step 4: Capture pre-migration metrics${NC}"
poetry run python scripts/measure_storage.py --output /tmp/pre_migration.json

echo ""
echo -e "${YELLOW}⬆️  Step 5: Run UUID migration${NC}"
poetry run alembic upgrade head

echo ""
echo -e "${YELLOW}📊 Step 6: Capture post-migration metrics${NC}"
poetry run python scripts/measure_storage.py --output /tmp/post_migration.json

echo ""
echo -e "${YELLOW}✅ Step 7: Verify data integrity${NC}"
poetry run python scripts/verify_uuid_migration.py

echo ""
echo -e "${YELLOW}📊 Step 8: Compare storage${NC}"
poetry run python scripts/compare_storage.py /tmp/pre_migration.json /tmp/post_migration.json

echo ""
echo -e "${YELLOW}⬇️  Step 9: Test downgrade${NC}"
poetry run alembic downgrade -1

echo ""
echo -e "${YELLOW}✅ Step 10: Verify rollback integrity${NC}"
poetry run python scripts/verify_string_uuids.py

echo ""
echo -e "${GREEN}🎉 Migration test complete!${NC}"
echo ""
echo "Cleanup: PGPASSWORD=$POSTGRES_PASSWORD psql -U $POSTGRES_USER -h $POSTGRES_HOST -c 'DROP DATABASE $TEST_DB_NAME;' postgres"
