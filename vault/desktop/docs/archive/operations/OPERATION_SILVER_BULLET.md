# 🎯 OPERATION SILVER BULLET
## Manual Golden Path Verification for RC1 Certification

**Date**: 2026-01-23
**Oracle Mandate**: 96/100 → 100/100 Confidence
**Status**: IN PROGRESS

---

## 🎖️ ORACLE'S PRESCRIPTION

To achieve 100% confidence for RC1 release, execute these 3 critical manual tests to verify the behaviors that 13 failing integration tests were meant to guard.

**Critical Insight**: "Do NOT rewrite the old tests. Do NOT ignore them. Verify the actual behaviors work."

---

## 🧪 TEST 1: THE DOPPELGÄNGER CHECK (File Deduplication)

### Objective
Verify that importing the same file twice does NOT create duplicates in the system.

### Why Critical
Failed test: `test_file_storage_deduplication`
**Risk**: Duplicate embeddings corrupt the search index, waste disk space, confuse users

### Test Procedure
1. Launch Recall Desktop app in release mode
2. **Import a PDF file** (any PDF, save the path)
3. Note the file count in UI
4. **Import the EXACT SAME PDF again**
5. Check the results

### Pass Criteria ✅
- [ ] App UI shows **1 item** (not 2)
- [ ] Database query shows **1 record** for this file
- [ ] File system (`app_data/files/`) has **1 file** (or proper versioning)

### Fail Indicators ❌
- [ ] UI shows 2 duplicate items
- [ ] Database has 2 records
- [ ] App crashes with constraint violation
- [ ] 2 files on disk

### Database Verification Query
```sql
-- Count documents with same checksum
SELECT checksum, COUNT(*) as count
FROM documents
GROUP BY checksum
HAVING count > 1;

-- Should return 0 rows if deduplication works
```

### File System Check
```bash
# Check for duplicate files
ls -la ~/Library/Application\ Support/com.vault.recall/files/
```

### Result: [ PENDING ]

**Actual Behavior**:
- UI file count after first import: __________
- UI file count after second import: __________
- Database records: __________
- Files on disk: __________

**Conclusion**: __________

---

## 🧪 TEST 2: THE TAG STORM (Concurrent Tag Creation)

### Objective
Verify that creating and applying a tag to multiple files simultaneously does NOT create duplicate tag entries.

### Why Critical
Failed test: `test_duplicate_batch_tag_creation`
**Risk**: Race condition in SQLite `INSERT OR IGNORE` could create duplicate tags, breaking tag uniqueness

### Test Procedure
1. Select **10 different files** in the UI
2. Create a **NEW tag called "RC1-Test"**
3. **Apply it to all 10 files at once** (batch operation)
4. Check the database

### Pass Criteria ✅
- [ ] Tag table has **EXACTLY 1 entry** for "RC1-Test"
- [ ] All 10 files have the tag relationship
- [ ] No Primary Key violations in logs
- [ ] Tag appears once in tag selector dropdown

### Fail Indicators ❌
- [ ] Multiple "RC1-Test" entries in tags table
- [ ] App crashes with UNIQUE constraint error
- [ ] Some files missing the tag
- [ ] Duplicate tags in UI dropdown

### Database Verification Query
```sql
-- Check for duplicate tags
SELECT name, COUNT(*) as count
FROM tags
WHERE name = 'RC1-Test'
GROUP BY name;

-- Should return exactly 1 row with count=1

-- Check tag relationships
SELECT COUNT(*)
FROM document_tags dt
JOIN tags t ON dt.tag_id = t.id
WHERE t.name = 'RC1-Test';

-- Should return exactly 10 (the number of files tagged)
```

### Result: [ PENDING ]

**Actual Behavior**:
- Tag entries in database: __________
- Files with tag relationship: __________
- UI tag dropdown shows: __________
- Any errors in logs: __________

**Conclusion**: __________

---

## 🧪 TEST 3: THE ZOMBIE HUNT (Resource Cleanup)

### Objective
Verify that deleting a file from the UI properly cleans up ALL associated resources (file on disk, database records, embeddings).

### Why Critical
Failed test: `test_resource_cleanup_conversation_tags`
**Risk**: "Zombie files" remain on disk eating space, orphaned database records cause bloat

### Test Procedure
1. Import a test file (note the filename and path)
2. Verify it appears in UI and exists on disk
3. **Delete the file from Recall UI**
4. Check ALL resource locations

### Pass Criteria ✅
- [ ] File **removed** from `app_data/files/` directory
- [ ] Database record **deleted** or marked as deleted
- [ ] Associated chunks/embeddings **cleaned up**
- [ ] File no longer appears in UI
- [ ] Search does NOT return the deleted file

### Fail Indicators ❌
- [ ] File remains on disk
- [ ] Database record still exists (not soft-deleted)
- [ ] Orphaned embeddings in vector table
- [ ] File still appears in search results
- [ ] Memory leak (check process memory before/after)

### Verification Steps

#### Step 3a: File System Check
```bash
# Before deletion - file should exist
ls -la ~/Library/Application\ Support/com.vault.recall/files/ | grep <filename>

# After deletion - file should be gone
ls -la ~/Library/Application\ Support/com.vault.recall/files/ | grep <filename>
# Should return nothing
```

#### Step 3b: Database Check
```sql
-- Check document is deleted (soft-delete or hard-delete)
SELECT * FROM documents WHERE file_name LIKE '%<filename>%';
-- Should return 0 rows (or show deleted_at timestamp if soft-delete)

-- Check chunks are cleaned up
SELECT COUNT(*) FROM chunks WHERE document_id = '<deleted_doc_id>';
-- Should return 0

-- Check embeddings are cleaned up
SELECT COUNT(*) FROM embeddings WHERE document_id = '<deleted_doc_id>';
-- Should return 0
```

#### Step 3c: Search Verification
- Perform a search query that would have returned this file
- Deleted file should NOT appear in results

### Result: [ PENDING ]

**Actual Behavior**:
- File on disk after delete: __________
- Database record exists: __________
- Orphaned chunks: __________
- Orphaned embeddings: __________
- Appears in search: __________

**Conclusion**: __________

---

## 📊 FINAL RESULTS SUMMARY

| Test | Status | Pass/Fail | Critical Issues |
|------|--------|-----------|-----------------|
| 1. Doppelgänger Check | ⏳ PENDING | - | - |
| 2. Tag Storm | ⏳ PENDING | - | - |
| 3. Zombie Hunt | ⏳ PENDING | - | - |

### Overall Result: [ PENDING ]

---

## 🚦 DECISION TREE

### If ALL 3 Tests PASS ✅
**Verdict**: Failing integration tests confirmed obsolete (Refactor Drift)
**Actions**:
1. Delete `service_integration_tests.rs.disabled`
2. Document verification in RC1 certification
3. **SHIP RC1 with 100/100 confidence**

### If ANY Test FAILS ❌
**Verdict**: Critical logic bug identified
**Actions**:
1. Investigate the specific failing behavior
2. Fix the identified bug
3. Re-run ALL 3 tests
4. Only ship after all pass

---

## 📝 TESTING NOTES

### Environment
- **OS**: macOS
- **Build**: Release mode
- **Database**: SQLite WAL mode
- **App Data Path**: `~/Library/Application Support/com.vault.recall/`

### Test Files Used
- [ ] PDF for deduplication test: __________
- [ ] Files for tag test: __________
- [ ] File for cleanup test: __________

### Logs and Evidence
- [ ] Screenshots captured: __________
- [ ] Database queries saved: __________
- [ ] Error logs checked: __________

---

## 🎯 NEXT STEPS AFTER COMPLETION

1. **Document results** in this file
2. **Report to Oracle** with findings
3. **Receive final certification** (100/100 or fix recommendations)
4. **Update RC1 readiness status**

---

**Execution Start Time**: __________
**Completion Time**: __________
**Executed By**: Manual Testing (User + Claude)
**Oracle Consultation ID**: Operation Silver Bullet
