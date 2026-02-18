import { Page, Locator, expect } from '@playwright/test';
import { BasePage } from './BasePage';

export class SearchPage extends BasePage {
  readonly searchInput: Locator;
  readonly semanticModeButton: Locator;
  readonly keywordModeButton: Locator;
  readonly hybridModeButton: Locator;
  readonly searchResults: Locator;
  readonly loadingIndicator: Locator;

  constructor(page: Page) {
    super(page);
    this.searchInput = page.locator('input[placeholder*="Search"], [data-testid="search-input"]');
    this.semanticModeButton = page.getByRole('button', { name: /Semantic/i });
    this.keywordModeButton = page.getByRole('button', { name: /Keyword/i });
    this.hybridModeButton = page.getByRole('button', { name: /Hybrid/i });
    this.searchResults = page.locator('[data-testid="search-result"], .search-result, [class*="SearchResult"]');
    this.loadingIndicator = page.locator('[data-testid="loading"], .loading, [class*="loading"]');
  }

  async search(query: string): Promise<void> {
    await this.searchInput.fill(query);
    await this.page.waitForTimeout(500);
  }

  async clearSearch(): Promise<void> {
    await this.searchInput.fill('');
  }

  async selectSearchMode(mode: 'semantic' | 'keyword' | 'hybrid'): Promise<void> {
    const buttonMap = {
      semantic: this.semanticModeButton,
      keyword: this.keywordModeButton,
      hybrid: this.hybridModeButton,
    };

    await buttonMap[mode].click();
    await expect(buttonMap[mode]).toHaveAttribute('aria-pressed', 'true');
  }

  async waitForResults(timeout = 10000): Promise<void> {
    await this.page.waitForSelector(
      '[data-testid="search-result"], .search-result, [class*="SearchResult"]',
      { timeout, state: 'visible' }
    );
  }

  async getResultsCount(): Promise<number> {
    await this.page.waitForTimeout(1000);
    return this.searchResults.count();
  }

  async getResultTitles(): Promise<string[]> {
    await this.waitForResults();
    const titles = await this.searchResults.allTextContents();
    return titles.filter(Boolean);
  }

  async clickResult(index: number): Promise<void> {
    await this.searchResults.nth(index).click();
  }

  async clickFirstResult(): Promise<void> {
    await this.clickResult(0);
  }

  async verifyResultContains(text: string, index = 0): Promise<void> {
    const result = this.searchResults.nth(index);
    await expect(result).toContainText(text);
  }

  async verifyNoResults(): Promise<void> {
    const noResultsText = this.page.getByText(/No results|No documents found/i);
    await expect(noResultsText).toBeVisible({ timeout: 5000 });
  }

  async waitForLoadingToFinish(): Promise<void> {
    const isLoading = await this.loadingIndicator.isVisible().catch(() => false);
    if (isLoading) {
      await this.loadingIndicator.waitFor({ state: 'hidden', timeout: 10000 });
    }
  }

  async performSearchWithMode(
    query: string,
    mode: 'semantic' | 'keyword' | 'hybrid'
  ): Promise<void> {
    await this.selectSearchMode(mode);
    await this.search(query);
    await this.waitForLoadingToFinish();
  }

  async verifySearchInputValue(expectedValue: string): Promise<void> {
    await expect(this.searchInput).toHaveValue(expectedValue);
  }

  async verifySearchModeSelected(mode: 'semantic' | 'keyword' | 'hybrid'): Promise<void> {
    const buttonMap = {
      semantic: this.semanticModeButton,
      keyword: this.keywordModeButton,
      hybrid: this.hybridModeButton,
    };

    await expect(buttonMap[mode]).toHaveAttribute('aria-pressed', 'true');
  }
}
