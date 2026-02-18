# UUID Migration Test Suite - Quick Start

## TL;DR

```bash
cd /home/user/Recall/vault/backend
./scripts/test_uuid_migration.sh
```

Expected: **✅ All checks pass, ~45% storage reduction**

## One-Liners

```bash
# Run full test suite
./scripts/test_uuid_migration.sh

# Cleanup after tests
PGPASSWORD=vault psql -U vault -c 'DROP DATABASE vault_test_uuid_migration;' postgres
```

## What It Tests

1. ✅ **String(36) → UUID conversion** works correctly
2. ✅ **Data integrity preserved** (no data loss)
3. ✅ **Foreign keys intact** (relationships work)
4. ✅ **Storage reduced** by 30-55%
5. ✅ **Rollback functional** (can revert safely)

## Expected Output

```
🧪 UUID Migration Test Suite
================================

📦 Step 1: Create test database
📦 Step 2: Apply base schema (before migration)
📦 Step 3: Seed test data
   ✅ Seeded 885 records

📊 Step 4: Capture pre-migration metrics
   ✅ Total database size: 15 MB

⬆️  Step 5: Run UUID migration
   Migrating watch_folders...
   Migrating files...
   ...
   UUID migration complete!

📊 Step 6: Capture post-migration metrics
   ✅ Total database size: 8 MB

✅ Step 7: Verify data integrity
   ✅ All UUID columns converted to native type
   ✅ All foreign key constraints intact
   ✅ No orphaned records
   ✅ VERIFICATION PASSED

📊 Step 8: Compare storage
   Total Database:
     Before: 15 MB
     After:  8 MB
     Saved:  7 MB (47%)
   ✅ Migration successful!

⬇️  Step 9: Test downgrade
   Reverting UUID migration...

✅ Step 10: Verify rollback integrity
   ✅ All UUID columns back to String(36)
   ✅ ROLLBACK VERIFICATION PASSED

🎉 Migration test complete!
```

## Failure? Check These

```bash
# 1. PostgreSQL running?
docker compose -f docker/docker-compose.dev.yml ps

# 2. Can connect?
psql -U vault -h localhost -c "SELECT 1;"

# 3. Alembic working?
poetry run alembic current

# 4. View detailed logs
./scripts/test_uuid_migration.sh 2>&1 | tee test.log
```

## Production Deployment

```bash
# 1. BACKUP FIRST!
pg_dump -U vault -d vault > backup_$(date +%Y%m%d).sql

# 2. Run migration
poetry run alembic upgrade head

# 3. Verify
poetry run python scripts/verify_uuid_migration.py

# 4. Rollback if needed
poetry run alembic downgrade -1
```

## Files Created

| File | Purpose |
|------|---------|
| `test_uuid_migration.sh` | Main orchestrator |
| `seed_test_data.py` | Creates 885 test records |
| `measure_storage.py` | Captures storage metrics |
| `compare_storage.py` | Shows before/after comparison |
| `verify_uuid_migration.py` | Validates migration success |
| `verify_string_uuids.py` | Validates rollback success |

## Need Help?

See full documentation: `scripts/README_UUID_MIGRATION_TEST.md`
