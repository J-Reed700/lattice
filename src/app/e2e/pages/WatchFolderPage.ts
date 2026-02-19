import { Page, Locator, expect } from '@playwright/test';
import { BasePage } from './BasePage';

export class WatchFolderPage extends BasePage {
  readonly addFolderButton: Locator;
  readonly folderList: Locator;
  readonly indexingProgress: Locator;
  readonly folderItems: Locator;

  constructor(page: Page) {
    super(page);
    this.addFolderButton = page.getByRole('button', { name: /Add Folder|Watch Folder|Select Folder/i });
    this.folderList = page.locator('[data-testid="folder-list"], [class*="folder-list"]');
    this.indexingProgress = page.locator('[data-testid="indexing-progress"], [class*="progress"]');
    this.folderItems = page.locator('[data-testid="folder-item"], [class*="folder-item"]');
  }

  async addFolder(): Promise<void> {
    await this.addFolderButton.click();
    await this.page.waitForTimeout(1000);
  }

  async waitForIndexingComplete(timeout = 60000): Promise<void> {
    const isIndexing = await this.indexingProgress.isVisible().catch(() => false);

    if (isIndexing) {
      await this.indexingProgress.waitFor({ state: 'hidden', timeout });
    }

    await this.page.waitForTimeout(2000);
  }

  async verifyFolderAdded(folderName: string): Promise<void> {
    const folder = this.page.getByText(folderName);
    await expect(folder).toBeVisible({ timeout: 10000 });
  }

  async getFolderCount(): Promise<number> {
    return this.folderItems.count();
  }

  async removeFolder(folderName: string): Promise<void> {
    const folderRow = this.page.locator(`[data-testid="folder-item"]:has-text("${folderName}")`);
    const removeButton = folderRow.locator('button[aria-label*="Remove"], button:has-text("Remove")');
    await removeButton.click();

    const confirmButton = this.page.getByRole('button', { name: /Confirm|Yes|Remove/i });
    if (await confirmButton.isVisible().catch(() => false)) {
      await confirmButton.click();
    }
  }

  async verifyFolderRemoved(folderName: string): Promise<void> {
    const folder = this.page.getByText(folderName);
    await expect(folder).not.toBeVisible();
  }

  async getFolderIndexStats(folderName: string): Promise<{ indexed: number; total: number }> {
    const folderRow = this.page.locator(`[data-testid="folder-item"]:has-text("${folderName}")`);
    const statsText = await folderRow.textContent();

    const match = statsText?.match(/(\d+)\s*\/\s*(\d+)/);
    if (match) {
      return { indexed: parseInt(match[1]), total: parseInt(match[2]) };
    }

    return { indexed: 0, total: 0 };
  }

  async verifyIndexingInProgress(): Promise<void> {
    await expect(this.indexingProgress).toBeVisible();
  }

  async verifyIndexingComplete(): Promise<void> {
    await expect(this.indexingProgress).not.toBeVisible();
  }

  async pauseIndexing(): Promise<void> {
    const pauseButton = this.page.getByRole('button', { name: /Pause/i });
    await pauseButton.click();
  }

  async resumeIndexing(): Promise<void> {
    const resumeButton = this.page.getByRole('button', { name: /Resume/i });
    await resumeButton.click();
  }

  async cancelIndexing(): Promise<void> {
    const cancelButton = this.page.getByRole('button', { name: /Cancel|Stop/i });
    await cancelButton.click();
  }
}
