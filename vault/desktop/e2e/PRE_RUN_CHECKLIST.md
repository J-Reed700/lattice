# Pre-Run Checklist for E2E Tests

## Before Running Tests

### 1. Environment Setup ✅

- [ ] Node.js 18+ installed
- [ ] npm dependencies installed (`npm install`)
- [ ] Playwright browsers installed (`npx playwright install chromium`)
- [ ] Tauri app builds successfully (`npm run tauri:build`)

### 2. System Requirements ✅

- [ ] Minimum 8GB RAM available
- [ ] 2GB free disk space
- [ ] Port 1420 not in use
- [ ] No other Tauri instances running

### 3. Test Environment ✅

- [ ] Test fixtures exist in `e2e/fixtures/`
- [ ] Database clean (or using test database)
- [ ] No cached data interfering with tests
- [ ] Logging configured properly

## Quick Verification

Run the setup verification script:

```bash
node e2e/verify-setup.ts
```

Expected output: ✅ All checks passed!

## Running Tests - Step by Step

### Step 1: Verify App Builds

```bash
npm run tauri:build
```

✅ Should complete without errors

### Step 2: Run Single Test (Smoke Test)

```bash
npx playwright test e2e/flows/upload-search.spec.ts --grep "should upload a single file"
```

✅ Should pass in ~30 seconds

### Step 3: Run Full Suite

```bash
npm run test:e2e
```

✅ Should complete in 10-15 minutes

## Troubleshooting Common Issues

### Issue: Tests timeout immediately

**Solution**:
```bash
# Kill any running Tauri instances
taskkill /F /IM recall-desktop.exe

# Clear port 1420
netstat -ano | findstr :1420
```

### Issue: App won't start

**Solution**:
```bash
# Rebuild the app
npm run tauri:build

# Check for errors in build output
```

### Issue: File upload fails

**Solution**:
```bash
# Verify test fixtures exist
ls e2e/fixtures/

# Should show:
# - test-document.txt
# - sample-notes.md
# - code-sample.txt
```

### Issue: Search returns no results

**Solution**:
- Wait longer for indexing to complete
- Check if database is being created
- Verify AI models are loaded

## Post-Run Verification

### Check Test Results

```bash
# View HTML report
npm run test:e2e:report
```

### Review Artifacts

- Screenshots: `e2e-results/artifacts/`
- Videos: `e2e-results/artifacts/`
- Traces: `e2e-results/artifacts/`

### Expected Results

- ✅ 35/35 tests passing
- ✅ No flaky tests
- ✅ Total runtime < 15 minutes
- ✅ No critical errors

## CI/CD Checklist

### Before Pushing to CI

- [ ] All tests pass locally
- [ ] No hardcoded paths
- [ ] No test interdependencies
- [ ] Proper cleanup after tests
- [ ] CI configuration valid

### CI Environment

- [ ] GitHub Actions workflow exists
- [ ] Secrets configured (if needed)
- [ ] Timeout values appropriate
- [ ] Artifact upload configured
- [ ] Multi-platform tests enabled

## Test Maintenance Checklist

### Weekly

- [ ] Run full test suite
- [ ] Check for flaky tests
- [ ] Review failed tests
- [ ] Update test data if needed

### Monthly

- [ ] Review test coverage
- [ ] Update documentation
- [ ] Refactor duplicate code
- [ ] Check for deprecated APIs

### Per Release

- [ ] Run full test suite on all platforms
- [ ] Verify CI/CD pipeline
- [ ] Update test fixtures
- [ ] Document known issues

## Ready to Run? Final Check

✅ All environment requirements met
✅ Dependencies installed
✅ App builds successfully
✅ Smoke test passes
✅ Test fixtures present

**You're ready to run the E2E test suite!**

```bash
npm run test:e2e
```

## Quick Commands Reference

```bash
# Run all tests
npm run test:e2e

# Run with UI
npm run test:e2e:ui

# Run specific test
npx playwright test e2e/flows/upload-search.spec.ts

# Debug mode
npm run test:e2e:debug

# View report
npm run test:e2e:report

# Headed mode (see browser)
npm run test:e2e:headed
```

## Need Help?

1. Check `e2e/README.md` for detailed documentation
2. Review `e2e/QUICK_START.md` for quick reference
3. Check test examples in `e2e/flows/`
4. Consult Playwright docs: https://playwright.dev/

---

**Pro Tip**: Run tests in UI mode first to see what's happening:
```bash
npm run test:e2e:ui
```
