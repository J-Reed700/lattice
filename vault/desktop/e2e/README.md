# E2E Test Suite for Recall/Vault Desktop

## Overview

This directory contains End-to-End (E2E) tests for the Recall/Vault desktop application. The tests simulate real user interactions with the Tauri desktop app, covering critical user flows including file upload, search, watch folders, settings, and onboarding.

## Technology Stack

- **Playwright**: Browser automation and testing framework
- **Tauri 2.0**: Desktop application framework
- **TypeScript**: Type-safe test development
- **Page Object Model**: Maintainable test architecture

## Directory Structure

```
e2e/
├── flows/                    # Test specifications
│   ├── upload-search.spec.ts    # File upload and search tests
│   ├── watch-folder.spec.ts     # Watch folder functionality
│   ├── settings.spec.ts         # Settings configuration tests
│   └── onboarding.spec.ts       # First-run onboarding tests
├── pages/                    # Page Object Models
│   ├── BasePage.ts             # Base page class
│   ├── SearchPage.ts           # Search interface
│   ├── UploadPage.ts           # File upload interface
│   ├── SettingsPage.ts         # Settings panel
│   ├── WatchFolderPage.ts      # Watch folder management
│   └── OnboardingPage.ts       # Onboarding flow
├── fixtures/                 # Test data
│   ├── test-document.txt       # Sample text document
│   ├── sample-notes.md         # Markdown document
│   └── code-sample.txt         # Code file
├── utils/                    # Helper utilities
│   └── test-helpers.ts         # Common test functions
└── setup.ts                  # Test configuration and setup
```

## Running Tests

### Prerequisites

1. Install dependencies:
   ```bash
   npm install
   ```

2. Install Playwright browsers:
   ```bash
   npx playwright install chromium
   ```

### Run All Tests

```bash
npm run test:e2e
```

### Run with UI Mode

Playwright UI mode provides a visual interface for running and debugging tests:

```bash
npm run test:e2e:ui
```

### Run in Headed Mode

See the browser window during test execution:

```bash
npm run test:e2e:headed
```

### Run Specific Test File

```bash
npx playwright test e2e/flows/upload-search.spec.ts
```

### Debug Tests

```bash
npm run test:e2e:debug
```

### View Test Report

After running tests, view the HTML report:

```bash
npm run test:e2e:report
```

## Test Coverage

### 1. Upload and Search Flow (`upload-search.spec.ts`)

Tests the core functionality of uploading documents and searching for them:

- **Single file upload**: Upload a file and verify it's indexed
- **Multiple file upload**: Batch upload and search across files
- **All search modes**: Test semantic, keyword, and hybrid search
- **No results handling**: Verify graceful handling of empty results
- **Result interaction**: Click and open search results
- **Search clearing**: Clear search and restore results
- **Mode persistence**: Verify search mode selection persists
- **Special characters**: Handle special characters in search queries

### 2. Watch Folder Flow (`watch-folder.spec.ts`)

Tests automatic file indexing from watched folders:

- **Add watch folder**: Add a folder for automatic monitoring
- **Auto-indexing**: Verify files are automatically indexed
- **New file detection**: Detect and index files added after watching
- **Remove folder**: Remove a watch folder
- **Indexing progress**: Display indexing progress UI
- **Nested folders**: Handle subdirectories recursively
- **Multiple folders**: Manage multiple watch folders simultaneously
- **Empty folders**: Handle empty folders gracefully

### 3. Settings Flow (`settings.spec.ts`)

Tests application configuration and settings:

- **Open/close settings**: Navigate to settings panel
- **Tab navigation**: Switch between settings tabs
- **Toggle switches**: Enable/disable settings options
- **Save changes**: Persist settings changes
- **Settings persistence**: Verify settings persist after reload
- **Input validation**: Validate input field constraints
- **Search settings**: Configure search-specific options
- **Reset to defaults**: Reset settings to default values
- **Help tooltips**: Display contextual help
- **Theme toggle**: Switch between light and dark themes
- **Keyboard shortcuts**: Configure keyboard shortcuts

### 4. Onboarding Flow (`onboarding.spec.ts`)

Tests first-run experience and setup:

- **First launch detection**: Show onboarding on first launch
- **Complete onboarding**: Walk through all onboarding steps
- **Skip onboarding**: Allow users to skip setup
- **Folder selection**: Select initial folder during onboarding
- **Settings configuration**: Configure basic settings during setup
- **Model download**: Handle AI model downloads
- **Error handling**: Gracefully handle initialization errors
- **Sequential steps**: Progress through steps in order
- **No repeat**: Don't show onboarding on subsequent launches

## Page Object Model

Tests use the Page Object Model pattern for maintainability:

### BasePage

Base class providing common functionality:
- Navigation
- Element interaction
- Wait utilities
- Screenshot capture

### SearchPage

Search interface interactions:
- `search(query)`: Perform a search
- `selectSearchMode(mode)`: Choose search mode
- `getResultsCount()`: Count search results
- `clickFirstResult()`: Open first result
- `verifyNoResults()`: Check for empty state

### UploadPage

File upload interactions:
- `selectFile(filename)`: Upload single file
- `selectMultipleFiles(filenames)`: Upload multiple files
- `dragAndDropFile(filename)`: Drag and drop upload
- `waitForUploadComplete()`: Wait for indexing
- `verifySuccessMessage()`: Check success notification

### SettingsPage

Settings panel interactions:
- `open()`: Open settings
- `selectTab(tabName)`: Switch tabs
- `toggleSwitch(label)`: Toggle setting
- `setInputValue(label, value)`: Set input field
- `save()`: Save changes

## Test Helpers

Common utilities in `test-helpers.ts`:

- `waitForApp()`: Wait for app to load
- `waitForInitialization()`: Wait for initialization
- `waitForSearchResults()`: Wait for search to complete
- `uploadTestFile(filename)`: Upload a test file
- `searchFor(query, mode)`: Perform search with mode
- `navigateToView(view)`: Navigate to app section
- `takeScreenshot(name)`: Capture screenshot

## Test Data

### Fixtures

Test files located in `e2e/fixtures/`:

1. **test-document.txt**: Comprehensive text document with AI/ML content
2. **sample-notes.md**: Markdown document with formatting
3. **code-sample.txt**: JavaScript code for code search testing

### Creating Test Data

To add new test fixtures:

1. Create file in `e2e/fixtures/`
2. Reference in tests using `path.resolve(__dirname, '../fixtures', filename)`
3. Use in upload tests or watch folder tests

## Best Practices

### 1. Use Page Objects

```typescript
// Good
const searchPage = new SearchPage(page);
await searchPage.search('query');

// Avoid
await page.fill('input[placeholder*="Search"]', 'query');
```

### 2. Use Test Steps

```typescript
await test.step('Upload document', async () => {
  await uploadPage.selectFile('test.txt');
  await uploadPage.waitForUploadComplete();
});
```

### 3. Handle Timing

```typescript
// Wait for conditions, not arbitrary timeouts
await searchPage.waitForResults();

// Only use timeouts when necessary
await page.waitForTimeout(1000);
```

### 4. Assertions

```typescript
// Use Playwright's expect
await expect(searchPage.searchInput).toBeVisible();

// Get counts
const count = await searchPage.getResultsCount();
expect(count).toBeGreaterThan(0);
```

### 5. Error Handling

```typescript
// Handle optional features gracefully
await settingsPage.save().catch(() => {
  console.log('Auto-save enabled - no save button');
});
```

## Debugging

### Debug Single Test

```bash
npx playwright test upload-search.spec.ts --debug
```

### Show Browser

```bash
npx playwright test --headed --slowmo=1000
```

### Screenshots and Videos

Failed tests automatically capture:
- Screenshots: `e2e-results/artifacts/`
- Videos: `e2e-results/artifacts/`
- Traces: `e2e-results/artifacts/`

### Playwright Inspector

The test will pause and open Playwright Inspector:

```typescript
await page.pause();
```

## CI/CD Integration

### GitHub Actions

The `.github/workflows/e2e-tests.yml` workflow runs E2E tests on:
- Windows
- macOS
- Linux

Triggered on:
- Push to main/develop
- Pull requests

### Local CI Simulation

```bash
CI=true npm run test:e2e
```

## Troubleshooting

### Tests Timeout

**Issue**: Tests timeout waiting for app
**Solution**: Increase timeout in `playwright.config.ts`:
```typescript
timeout: 120000, // 2 minutes
```

### Flaky Tests

**Issue**: Tests occasionally fail
**Solution**:
1. Add proper waits
2. Check for race conditions
3. Use `test.retry` for retries

### App Not Starting

**Issue**: Tauri app doesn't start
**Solution**:
1. Build app first: `npm run tauri:build`
2. Check port conflicts (default: 1420)
3. Verify dependencies installed

### File Not Found

**Issue**: Test fixture not found
**Solution**: Ensure files exist in `e2e/fixtures/`

## Performance

### Test Duration

Expected run times:
- Upload & Search: ~2-3 minutes
- Watch Folder: ~3-4 minutes
- Settings: ~2-3 minutes
- Onboarding: ~1-2 minutes
- **Total**: ~10-15 minutes

### Optimization

- Run tests in parallel where possible
- Use `fullyParallel: false` in config (Tauri limitation)
- Limit workers to 1 for Tauri tests

## Contributing

### Adding New Tests

1. Create test file in `e2e/flows/`
2. Import necessary page objects
3. Follow existing test structure
4. Add test documentation
5. Update this README

### Adding Page Objects

1. Create page class in `e2e/pages/`
2. Extend `BasePage`
3. Define locators in constructor
4. Add interaction methods
5. Document public methods

### Test Naming

- Use descriptive names: `should upload file and find in search`
- Start with action: "should", "can", "handles"
- Be specific: Include expected outcome

## Resources

- [Playwright Documentation](https://playwright.dev/)
- [Tauri Testing Guide](https://tauri.app/v1/guides/testing/)
- [Page Object Model](https://playwright.dev/docs/pom)
- [Best Practices](https://playwright.dev/docs/best-practices)

## Support

For issues or questions:
1. Check this README
2. Review test examples
3. Consult Playwright docs
4. Ask in project repository

## License

MIT License - See repository root for details
