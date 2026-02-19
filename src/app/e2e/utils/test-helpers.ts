import { Page, expect } from '@playwright/test';
import { ExtendedPage } from '../setup';
import fs from 'fs';
import path from 'path';

export class TestHelpers {
  constructor(private page: Page) {}

  async waitForApp(): Promise<void> {
    await this.page.waitForLoadState('domcontentloaded');
    await this.page.waitForSelector('body', { state: 'attached' });
  }

  async waitForInitialization(): Promise<void> {
    const initializingText = this.page.getByText(/Initializing Vault/i);

    const isVisible = await initializingText.isVisible().catch(() => false);

    if (isVisible) {
      await initializingText.waitFor({ state: 'hidden', timeout: 60000 });
    }

    await this.page.waitForSelector('[data-testid="app-ready"], .search-bar, input[placeholder*="Search"]', {
      timeout: 30000,
    });
  }

  async waitForErrorToDisappear(): Promise<void> {
    const errorElements = this.page.locator('[role="alert"], .error, [class*="error"]');
    const count = await errorElements.count();

    if (count > 0) {
      await errorElements.first().waitFor({ state: 'hidden', timeout: 10000 }).catch(() => {});
    }
  }

  async ensureNoErrors(): Promise<void> {
    const errorMessages = await this.page.locator('[role="alert"]:has-text("error"), .error-message, [class*="error"]:has-text("Failed")').all();

    if (errorMessages.length > 0) {
      const errorTexts = await Promise.all(
        errorMessages.map(async (el) => await el.textContent())
      );
      throw new Error(`Unexpected errors found: ${errorTexts.join(', ')}`);
    }
  }

  async typeWithDelay(selector: string, text: string, delay = 50): Promise<void> {
    const input = this.page.locator(selector);
    await input.fill('');
    await input.type(text, { delay });
  }

  async selectSearchMode(mode: 'semantic' | 'keyword' | 'hybrid'): Promise<void> {
    const modeButton = this.page.getByRole('button', { name: new RegExp(mode, 'i') });
    await modeButton.click();
    await expect(modeButton).toHaveAttribute('aria-pressed', 'true');
  }

  async waitForSearchResults(timeout = 10000): Promise<void> {
    await this.page.waitForSelector('[data-testid="search-results"], .search-results, [class*="search-result"]', {
      timeout,
    });
  }

  async getSearchResultsCount(): Promise<number> {
    const results = await this.page.locator('[data-testid="search-result"], .search-result, [class*="SearchResult"]').all();
    return results.length;
  }

  async clickFirstSearchResult(): Promise<void> {
    const firstResult = this.page.locator('[data-testid="search-result"], .search-result, [class*="SearchResult"]').first();
    await firstResult.click();
  }

  async uploadTestFile(filename: string): Promise<void> {
    const filePath = path.resolve(__dirname, '../fixtures', filename);

    if (!fs.existsSync(filePath)) {
      throw new Error(`Test file not found: ${filePath}`);
    }

    const fileChooserPromise = this.page.waitForEvent('filechooser');
    await this.page.getByText(/Select File|Choose File/i).click();
    const fileChooser = await fileChooserPromise;
    await fileChooser.setFiles(filePath);
  }

  async waitForSuccessMessage(text?: string, timeout = 10000): Promise<void> {
    const selector = text
      ? `[role="status"]:has-text("${text}"), .success:has-text("${text}")`
      : '[role="status"], .success, [class*="success"]';

    await this.page.waitForSelector(selector, { timeout });
  }

  async navigateToView(view: 'search' | 'files' | 'qa' | 'settings'): Promise<void> {
    const navButton = this.page.getByRole('button', { name: new RegExp(view, 'i') })
      .or(this.page.getByRole('link', { name: new RegExp(view, 'i') }));

    await navButton.click();
    await this.page.waitForTimeout(500);
  }

  async openSettings(): Promise<void> {
    await this.navigateToView('settings');
    await this.page.waitForSelector('[data-testid="settings-panel"], .settings, h1:has-text("Settings")', {
      timeout: 5000,
    });
  }

  async clickTab(tabName: string): Promise<void> {
    const tab = this.page.getByRole('tab', { name: new RegExp(tabName, 'i') });
    await tab.click();
    await expect(tab).toHaveAttribute('aria-selected', 'true');
  }

  async saveSettings(): Promise<void> {
    const saveButton = this.page.getByRole('button', { name: /Save|Apply/i });
    await saveButton.click();
    await this.waitForSuccessMessage('saved', 5000).catch(() => {});
  }

  async addWatchFolder(folderPath: string): Promise<void> {
    const addButton = this.page.getByRole('button', { name: /Add Folder|Select Folder/i });

    await addButton.click();
    await this.page.waitForTimeout(1000);
  }

  async searchFor(query: string, mode: 'semantic' | 'keyword' | 'hybrid' = 'hybrid'): Promise<void> {
    await this.selectSearchMode(mode);

    const searchInput = this.page.locator('input[placeholder*="Search"], [data-testid="search-input"]');
    await searchInput.fill(query);

    await this.page.waitForTimeout(500);
  }

  async clearSearch(): Promise<void> {
    const searchInput = this.page.locator('input[placeholder*="Search"], [data-testid="search-input"]');
    await searchInput.fill('');
  }

  async takeScreenshot(name: string): Promise<void> {
    await this.page.screenshot({
      path: path.resolve(__dirname, '../../e2e-results/screenshots', `${name}.png`),
      fullPage: true,
    });
  }

  async assertElementVisible(selector: string): Promise<void> {
    await expect(this.page.locator(selector)).toBeVisible();
  }

  async assertTextPresent(text: string | RegExp): Promise<void> {
    await expect(this.page.getByText(text)).toBeVisible();
  }

  async getPageTitle(): Promise<string> {
    return this.page.title();
  }

  async waitForTimeout(ms: number): Promise<void> {
    await this.page.waitForTimeout(ms);
  }
}

export function createTestHelpers(page: Page): TestHelpers {
  return new TestHelpers(page);
}
