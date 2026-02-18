# E2E Testing Quick Start Guide

## Setup (One-time)

```bash
cd vault/desktop
npm install
npx playwright install chromium
```

## Run Tests

### Run all tests
```bash
npm run test:e2e
```

### Run with visual UI
```bash
npm run test:e2e:ui
```

### Run specific test file
```bash
npx playwright test e2e/flows/upload-search.spec.ts
```

### Debug mode
```bash
npm run test:e2e:debug
```

### View results
```bash
npm run test:e2e:report
```

## Test Files

| File | Tests |
|------|-------|
| `upload-search.spec.ts` | File upload and search (8 tests) |
| `watch-folder.spec.ts` | Auto-indexing watch folders (7 tests) |
| `settings.spec.ts` | Settings configuration (11 tests) |
| `onboarding.spec.ts` | First-run experience (9 tests) |

## Common Commands

```bash
# Run one test by name
npx playwright test -g "should upload a single file"

# Run tests in headed mode (see browser)
npm run test:e2e:headed

# Generate code for new tests
npx playwright codegen http://localhost:1420

# Update snapshots (if using visual regression)
npx playwright test --update-snapshots
```

## Troubleshooting

### App won't start
1. Build the app: `npm run tauri:build`
2. Check if port 1420 is in use

### Tests fail immediately
- Run `npm run tauri:dev` manually first to verify app works
- Check that all dependencies are installed

### Timeout errors
- Increase timeout in `playwright.config.ts`
- Check if machine is under heavy load

## Quick Test Examples

### Upload a file and search
```typescript
const uploadPage = new UploadPage(page);
const searchPage = new SearchPage(page);

await uploadPage.selectFile('test-document.txt');
await uploadPage.waitForUploadComplete();

await searchPage.search('artificial intelligence');
await searchPage.waitForResults();
```

### Change a setting
```typescript
const settingsPage = new SettingsPage(page);

await settingsPage.open();
await settingsPage.selectTab('search');
await settingsPage.setInputValue('limit', '50');
await settingsPage.save();
```

## Need Help?

- Read full documentation: `e2e/README.md`
- Playwright docs: https://playwright.dev/
- Check test examples in `e2e/flows/`
