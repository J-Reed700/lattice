import { Page, Locator, expect } from '@playwright/test';
import { BasePage } from './BasePage';

export class SettingsPage extends BasePage {
  readonly settingsHeading: Locator;
  readonly saveButton: Locator;
  readonly cancelButton: Locator;
  readonly successNotification: Locator;

  readonly generalTab: Locator;
  readonly searchTab: Locator;
  readonly indexingTab: Locator;
  readonly modelTab: Locator;
  readonly storageTab: Locator;
  readonly advancedTab: Locator;

  constructor(page: Page) {
    super(page);
    this.settingsHeading = page.locator('h1, h2').filter({ hasText: /Settings/i });
    this.saveButton = page.getByRole('button', { name: /Save|Apply/i });
    this.cancelButton = page.getByRole('button', { name: /Cancel|Close/i });
    this.successNotification = page.locator('[role="status"], [class*="success"]');

    this.generalTab = page.getByRole('tab', { name: /General/i });
    this.searchTab = page.getByRole('tab', { name: /Search/i });
    this.indexingTab = page.getByRole('tab', { name: /Indexing/i });
    this.modelTab = page.getByRole('tab', { name: /Model|AI/i });
    this.storageTab = page.getByRole('tab', { name: /Storage/i });
    this.advancedTab = page.getByRole('tab', { name: /Advanced/i });
  }

  async open(): Promise<void> {
    const settingsButton = this.page.getByRole('button', { name: /Settings/i })
      .or(this.page.getByRole('link', { name: /Settings/i }));
    await settingsButton.click();
    await expect(this.settingsHeading).toBeVisible({ timeout: 5000 });
  }

  async selectTab(tabName: string): Promise<void> {
    const tab = this.page.getByRole('tab', { name: new RegExp(tabName, 'i') });
    await tab.click();
    await expect(tab).toHaveAttribute('aria-selected', 'true');
  }

  async save(): Promise<void> {
    await this.saveButton.click();
    await this.waitForSaveSuccess();
  }

  async cancel(): Promise<void> {
    await this.cancelButton.click();
  }

  async waitForSaveSuccess(timeout = 5000): Promise<void> {
    await expect(this.successNotification).toBeVisible({ timeout });
  }

  async verifySaved(): Promise<void> {
    await expect(this.successNotification).toContainText(/saved|applied/i);
  }

  async toggleSwitch(label: string): Promise<void> {
    const toggle = this.page.getByRole('switch', { name: new RegExp(label, 'i') })
      .or(this.page.getByLabel(new RegExp(label, 'i')));
    await toggle.click();
  }

  async setSwitchValue(label: string, enabled: boolean): Promise<void> {
    const toggle = this.page.getByRole('switch', { name: new RegExp(label, 'i') })
      .or(this.page.getByLabel(new RegExp(label, 'i')));

    const currentState = await toggle.getAttribute('aria-checked');
    const isCurrentlyEnabled = currentState === 'true';

    if (isCurrentlyEnabled !== enabled) {
      await toggle.click();
    }
  }

  async setInputValue(label: string, value: string): Promise<void> {
    const input = this.page.getByLabel(new RegExp(label, 'i'));
    await input.fill(value);
  }

  async selectDropdownOption(label: string, option: string): Promise<void> {
    const dropdown = this.page.getByLabel(new RegExp(label, 'i'));
    await dropdown.click();
    await this.page.getByRole('option', { name: option }).click();
  }

  async verifySettingValue(label: string, expectedValue: string): Promise<void> {
    const input = this.page.getByLabel(new RegExp(label, 'i'));
    await expect(input).toHaveValue(expectedValue);
  }

  async verifySwitchEnabled(label: string): Promise<void> {
    const toggle = this.page.getByRole('switch', { name: new RegExp(label, 'i') })
      .or(this.page.getByLabel(new RegExp(label, 'i')));
    await expect(toggle).toHaveAttribute('aria-checked', 'true');
  }

  async verifySwitchDisabled(label: string): Promise<void> {
    const toggle = this.page.getByRole('switch', { name: new RegExp(label, 'i') })
      .or(this.page.getByLabel(new RegExp(label, 'i')));
    await expect(toggle).toHaveAttribute('aria-checked', 'false');
  }
}

export class SearchSettingsTab extends SettingsPage {
  async setSearchResultLimit(limit: number): Promise<void> {
    await this.setInputValue('results', limit.toString());
  }

  async enableSemanticSearch(): Promise<void> {
    await this.setSwitchValue('semantic', true);
  }

  async disableSemanticSearch(): Promise<void> {
    await this.setSwitchValue('semantic', false);
  }
}

export class IndexingSettingsTab extends SettingsPage {
  readonly addFolderButton: Locator;
  readonly folderList: Locator;

  constructor(page: Page) {
    super(page);
    this.addFolderButton = page.getByRole('button', { name: /Add Folder|Watch Folder/i });
    this.folderList = page.locator('[data-testid="folder-list"], [class*="folder"]');
  }

  async addWatchFolder(): Promise<void> {
    const fileChooserPromise = this.page.waitForEvent('filechooser');
    await this.addFolderButton.click();
    await fileChooserPromise;
  }

  async removeFolderByPath(folderPath: string): Promise<void> {
    const folder = this.page.locator(`[data-path="${folderPath}"]`);
    const removeButton = folder.locator('button[aria-label*="Remove"]');
    await removeButton.click();
  }

  async verifyFolderInList(folderPath: string): Promise<void> {
    const folder = this.page.getByText(folderPath);
    await expect(folder).toBeVisible();
  }
}
