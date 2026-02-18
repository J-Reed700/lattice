import { Page, Locator, expect } from '@playwright/test';
import { BasePage } from './BasePage';
import path from 'path';

export class UploadPage extends BasePage {
  readonly selectFileButton: Locator;
  readonly selectMultipleFilesButton: Locator;
  readonly selectFolderButton: Locator;
  readonly dropZone: Locator;
  readonly uploadingIndicator: Locator;
  readonly successMessage: Locator;
  readonly errorMessage: Locator;

  constructor(page: Page) {
    super(page);
    this.selectFileButton = page.getByRole('button', { name: /Select File/i }).first();
    this.selectMultipleFilesButton = page.getByRole('button', { name: /Select Multiple Files/i });
    this.selectFolderButton = page.getByRole('button', { name: /Select Folder/i });
    this.dropZone = page.locator('[class*="border-dashed"], [data-testid="drop-zone"]');
    this.uploadingIndicator = page.getByText(/Indexing files/i);
    this.successMessage = page.locator('[class*="bg-green"], [role="status"]');
    this.errorMessage = page.locator('[class*="bg-red"], [role="alert"]');
  }

  async selectFile(filename: string): Promise<void> {
    const filePath = path.resolve(__dirname, '../fixtures', filename);
    const fileChooserPromise = this.page.waitForEvent('filechooser');
    await this.selectFileButton.click();
    const fileChooser = await fileChooserPromise;
    await fileChooser.setFiles(filePath);
  }

  async selectMultipleFiles(filenames: string[]): Promise<void> {
    const filePaths = filenames.map((name) =>
      path.resolve(__dirname, '../fixtures', name)
    );
    const fileChooserPromise = this.page.waitForEvent('filechooser');
    await this.selectMultipleFilesButton.click();
    const fileChooser = await fileChooserPromise;
    await fileChooser.setFiles(filePaths);
  }

  async dragAndDropFile(filename: string): Promise<void> {
    const filePath = path.resolve(__dirname, '../fixtures', filename);

    const dataTransfer = await this.page.evaluateHandle((filePath) => {
      const dt = new DataTransfer();
      return dt;
    }, filePath);

    await this.dropZone.dispatchEvent('drop', { dataTransfer });
  }

  async waitForUploadComplete(timeout = 30000): Promise<void> {
    await this.uploadingIndicator.waitFor({ state: 'visible', timeout: 5000 }).catch(() => {});

    await this.uploadingIndicator.waitFor({ state: 'hidden', timeout });
  }

  async waitForSuccess(timeout = 10000): Promise<void> {
    await expect(this.successMessage).toBeVisible({ timeout });
  }

  async verifySuccessMessage(expectedText?: string): Promise<void> {
    await expect(this.successMessage).toBeVisible();
    if (expectedText) {
      await expect(this.successMessage).toContainText(expectedText);
    }
  }

  async verifyErrorMessage(expectedText?: string): Promise<void> {
    await expect(this.errorMessage).toBeVisible();
    if (expectedText) {
      await expect(this.errorMessage).toContainText(expectedText);
    }
  }

  async uploadAndWait(filename: string): Promise<void> {
    await this.selectFile(filename);
    await this.waitForUploadComplete();
    await this.waitForSuccess();
  }

  async uploadMultipleAndWait(filenames: string[]): Promise<void> {
    await this.selectMultipleFiles(filenames);
    await this.waitForUploadComplete();
    await this.waitForSuccess();
  }

  async verifyDropZoneActive(): Promise<void> {
    await expect(this.dropZone).toHaveClass(/border-blue|bg-blue/);
  }
}
