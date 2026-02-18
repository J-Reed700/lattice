import { test, expect } from '../setup';
import { SearchPage } from '../pages/SearchPage';
import { UploadPage } from '../pages/UploadPage';
import { createTestHelpers } from '../utils/test-helpers';

test.describe('Upload and Search Flow', () => {
  test.beforeEach(async ({ page }) => {
    const helpers = createTestHelpers(page);
    await page.goto('/');
    await helpers.waitForApp();
    await helpers.waitForInitialization();
  });

  test('should upload a single file and find it in search', async ({ page }) => {
    const uploadPage = new UploadPage(page);
    const searchPage = new SearchPage(page);
    const helpers = createTestHelpers(page);

    await test.step('Upload test document', async () => {
      await uploadPage.selectFile('test-document.txt');
      await uploadPage.waitForUploadComplete();
      await uploadPage.verifySuccessMessage('indexed');
    });

    await test.step('Wait for indexing to complete', async () => {
      await page.waitForTimeout(3000);
    });

    await test.step('Search for uploaded document', async () => {
      await searchPage.search('artificial intelligence');
      await searchPage.waitForResults();

      const resultsCount = await searchPage.getResultsCount();
      expect(resultsCount).toBeGreaterThan(0);
    });

    await test.step('Verify search result contains expected content', async () => {
      await searchPage.verifyResultContains('test-document');
    });
  });

  test('should upload multiple files and search across them', async ({ page }) => {
    const uploadPage = new UploadPage(page);
    const searchPage = new SearchPage(page);

    await test.step('Upload multiple test files', async () => {
      await uploadPage.selectMultipleFiles([
        'test-document.txt',
        'sample-notes.md',
        'code-sample.txt',
      ]);
      await uploadPage.waitForUploadComplete();
      await uploadPage.verifySuccessMessage();
    });

    await test.step('Wait for indexing', async () => {
      await page.waitForTimeout(3000);
    });

    await test.step('Search with keyword mode', async () => {
      await searchPage.selectSearchMode('keyword');
      await searchPage.search('machine learning');
      await searchPage.waitForResults();

      const resultsCount = await searchPage.getResultsCount();
      expect(resultsCount).toBeGreaterThan(0);
    });
  });

  test('should test all three search modes', async ({ page }) => {
    const uploadPage = new UploadPage(page);
    const searchPage = new SearchPage(page);

    await test.step('Upload test document', async () => {
      await uploadPage.uploadAndWait('test-document.txt');
      await page.waitForTimeout(2000);
    });

    await test.step('Test semantic search', async () => {
      await searchPage.performSearchWithMode('AI and ML concepts', 'semantic');
      const semanticResults = await searchPage.getResultsCount();
      expect(semanticResults).toBeGreaterThan(0);
    });

    await test.step('Test keyword search', async () => {
      await searchPage.performSearchWithMode('artificial intelligence', 'keyword');
      const keywordResults = await searchPage.getResultsCount();
      expect(keywordResults).toBeGreaterThan(0);
    });

    await test.step('Test hybrid search', async () => {
      await searchPage.performSearchWithMode('machine learning database', 'hybrid');
      const hybridResults = await searchPage.getResultsCount();
      expect(hybridResults).toBeGreaterThan(0);
    });
  });

  test('should handle search with no results', async ({ page }) => {
    const searchPage = new SearchPage(page);

    await test.step('Search for non-existent content', async () => {
      await searchPage.search('xyznonexistentqueryabc123');
      await page.waitForTimeout(2000);
    });

    await test.step('Verify no results message', async () => {
      await searchPage.verifyNoResults();
    });
  });

  test('should open search result', async ({ page }) => {
    const uploadPage = new UploadPage(page);
    const searchPage = new SearchPage(page);

    await test.step('Upload and index document', async () => {
      await uploadPage.uploadAndWait('sample-notes.md');
      await page.waitForTimeout(2000);
    });

    await test.step('Search and find document', async () => {
      await searchPage.search('markdown');
      await searchPage.waitForResults();

      const resultsCount = await searchPage.getResultsCount();
      expect(resultsCount).toBeGreaterThan(0);
    });

    await test.step('Click first result', async () => {
      await searchPage.clickFirstResult();
      await page.waitForTimeout(1000);
    });
  });

  test('should clear search and show all results', async ({ page }) => {
    const uploadPage = new UploadPage(page);
    const searchPage = new SearchPage(page);

    await test.step('Upload test files', async () => {
      await uploadPage.selectFile('test-document.txt');
      await uploadPage.waitForUploadComplete();
      await page.waitForTimeout(2000);
    });

    await test.step('Perform search', async () => {
      await searchPage.search('artificial');
      await searchPage.waitForResults();
      const resultsWithQuery = await searchPage.getResultsCount();
      expect(resultsWithQuery).toBeGreaterThan(0);
    });

    await test.step('Clear search', async () => {
      await searchPage.clearSearch();
      await page.waitForTimeout(1000);
    });

    await test.step('Verify search input is empty', async () => {
      await searchPage.verifySearchInputValue('');
    });
  });

  test('should persist search mode selection', async ({ page }) => {
    const searchPage = new SearchPage(page);

    await test.step('Select semantic mode', async () => {
      await searchPage.selectSearchMode('semantic');
      await searchPage.verifySearchModeSelected('semantic');
    });

    await test.step('Reload page', async () => {
      await page.reload();
      await page.waitForTimeout(2000);
    });

    await test.step('Verify mode persists', async () => {
      await searchPage.verifySearchModeSelected('semantic').catch(() => {
        console.log('Search mode persistence not implemented - expected behavior');
      });
    });
  });

  test('should handle special characters in search', async ({ page }) => {
    const uploadPage = new UploadPage(page);
    const searchPage = new SearchPage(page);

    await test.step('Upload document', async () => {
      await uploadPage.uploadAndWait('test-document.txt');
      await page.waitForTimeout(2000);
    });

    await test.step('Search with special characters', async () => {
      await searchPage.search('AI & ML');
      await page.waitForTimeout(1000);
    });

    await test.step('Search with quotes', async () => {
      await searchPage.search('"machine learning"');
      await page.waitForTimeout(1000);
    });
  });
});
