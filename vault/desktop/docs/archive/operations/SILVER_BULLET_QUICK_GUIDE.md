# 🎯 OPERATION SILVER BULLET - Quick Testing Guide

**Estimated Time**: 10 minutes
**Goal**: Verify 3 critical behaviors for 100% RC1 confidence

---

## ⚡ SETUP (Do Once)

1. **Wait for build to complete** (Tauri is building now...)
2. **Launch the app**: Double-click the `.app` bundle when ready
3. **Download a test PDF**: Any PDF file will work (save its path)

---

## 🧪 TEST 1: DOPPELGÄNGER CHECK (3 minutes)

### What We're Testing
**Can the app handle importing the same file twice without creating duplicates?**

### Steps
1. **Import a PDF file** using the app's import function
   - Note: How many files showing in UI? Write down: ______

2. **Import THE EXACT SAME PDF again**
   - What happens? ______

3. **Check the UI**
   - Files showing now: ______

### ✅ PASS if:
- UI shows only **1 file** (not 2)
- No error/crash

### ❌ FAIL if:
- UI shows 2 copies of the same file
- App crashes
- Duplicate entries visible

**Result**: [ PASS / FAIL ]

---

## 🧪 TEST 2: TAG STORM (3 minutes)

### What We're Testing
**Can we create a new tag and apply it to multiple files simultaneously without race conditions?**

### Steps
1. **Import 5-10 different files** (any files)

2. **Select all of them** in the UI

3. **Create a NEW tag called "RC1-Test"**
   - Apply it to all selected files at once

4. **Check the tag selector dropdown**
   - How many "RC1-Test" entries appear: ______

### ✅ PASS if:
- Tag dropdown shows **exactly 1** "RC1-Test" entry
- All files show the tag
- No crash/error

### ❌ FAIL if:
- Multiple "RC1-Test" entries in dropdown
- App crashes with "UNIQUE constraint" error
- Some files missing the tag

**Result**: [ PASS / FAIL ]

---

## 🧪 TEST 3: ZOMBIE HUNT (4 minutes)

### What We're Testing
**Does deleting a file from the UI properly clean up the file from disk?**

### Steps
1. **Import a test file** (any file)
   - Note the filename: ______

2. **Delete it from the UI**
   - Right-click → Delete (or however delete works)

3. **Check if file is gone from disk**
   - Open: `~/Library/Application Support/com.vault.recall/files/`
   - Search for your filename
   - Is it there? ______

4. **Search for the deleted file in the app**
   - Does it appear in search results? ______

### ✅ PASS if:
- File **NOT found** in `files/` directory
- File **does NOT appear** in app search
- No errors

### ❌ FAIL if:
- File still exists on disk
- File still appears in search results
- Orphaned database records

**Result**: [ PASS / FAIL ]

---

## 📊 FINAL RESULTS

| Test | Result | Notes |
|------|--------|-------|
| 1. Doppelgänger | [ PASS / FAIL ] | ______ |
| 2. Tag Storm | [ PASS / FAIL ] | ______ |
| 3. Zombie Hunt | [ PASS / FAIL ] | ______ |

---

## 🚦 WHAT HAPPENS NEXT?

### If ALL 3 Tests PASS ✅
**Congratulations!** The 13 failing tests are confirmed obsolete.
- **Oracle's verdict**: Delete `service_integration_tests.rs.disabled`
- **Ship RC1 with 100/100 confidence**

### If ANY Test FAILS ❌
**We found a bug!** But that's good - better now than after release.
- Report which test(s) failed
- We'll investigate and fix
- Re-run tests after fix

---

## 💡 TIPS

- **Take your time** - accuracy > speed
- **Note any weird behavior** - even if test "passes"
- **Check console/logs** for errors
- **Screenshot unexpected behavior**

---

**Ready?** Let's verify this app is bulletproof! 🛡️
