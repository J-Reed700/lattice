import { Page, Locator, expect } from '@playwright/test';
import { BasePage } from './BasePage';

export class OnboardingPage extends BasePage {
  readonly welcomeHeading: Locator;
  readonly nextButton: Locator;
  readonly skipButton: Locator;
  readonly getStartedButton: Locator;
  readonly finishButton: Locator;
  readonly progressIndicator: Locator;

  constructor(page: Page) {
    super(page);
    this.welcomeHeading = page.locator('h1, h2').filter({ hasText: /Welcome|Get Started/i });
    this.nextButton = page.getByRole('button', { name: /Next|Continue/i });
    this.skipButton = page.getByRole('button', { name: /Skip/i });
    this.getStartedButton = page.getByRole('button', { name: /Get Started/i });
    this.finishButton = page.getByRole('button', { name: /Finish|Done/i });
    this.progressIndicator = page.locator('[role="progressbar"], [data-testid="progress"]');
  }

  async verifyOnboardingScreen(): Promise<void> {
    await expect(this.welcomeHeading).toBeVisible({ timeout: 10000 });
  }

  async clickNext(): Promise<void> {
    await this.nextButton.click();
    await this.page.waitForTimeout(500);
  }

  async clickSkip(): Promise<void> {
    await this.skipButton.click();
  }

  async clickGetStarted(): Promise<void> {
    await this.getStartedButton.click();
  }

  async clickFinish(): Promise<void> {
    await this.finishButton.click();
  }

  async completeOnboarding(): Promise<void> {
    await this.verifyOnboardingScreen();

    while (await this.nextButton.isVisible().catch(() => false)) {
      await this.clickNext();
      await this.page.waitForTimeout(500);
    }

    if (await this.finishButton.isVisible().catch(() => false)) {
      await this.clickFinish();
    } else if (await this.getStartedButton.isVisible().catch(() => false)) {
      await this.clickGetStarted();
    }
  }

  async skipOnboarding(): Promise<void> {
    if (await this.skipButton.isVisible().catch(() => false)) {
      await this.clickSkip();
    }
  }

  async verifyStep(stepNumber: number): Promise<void> {
    const stepIndicator = this.page.locator(`[data-step="${stepNumber}"], [aria-current="step"]`);
    await expect(stepIndicator).toBeVisible();
  }

  async verifyOnboardingComplete(): Promise<void> {
    await expect(this.welcomeHeading).not.toBeVisible({ timeout: 5000 });
  }

  async selectFirstFolder(): Promise<void> {
    const selectFolderButton = this.page.getByRole('button', { name: /Select Folder|Choose Folder/i });
    if (await selectFolderButton.isVisible().catch(() => false)) {
      await selectFolderButton.click();
      await this.page.waitForTimeout(1000);
    }
  }

  async configureSettings(settings: {
    enableAutoIndex?: boolean;
    enableSemanticSearch?: boolean;
  }): Promise<void> {
    if (settings.enableAutoIndex !== undefined) {
      const autoIndexToggle = this.page.getByLabel(/Auto.*Index|Automatic/i);
      if (await autoIndexToggle.isVisible().catch(() => false)) {
        const isChecked = await autoIndexToggle.isChecked();
        if (isChecked !== settings.enableAutoIndex) {
          await autoIndexToggle.click();
        }
      }
    }

    if (settings.enableSemanticSearch !== undefined) {
      const semanticToggle = this.page.getByLabel(/Semantic.*Search/i);
      if (await semanticToggle.isVisible().catch(() => false)) {
        const isChecked = await semanticToggle.isChecked();
        if (isChecked !== settings.enableSemanticSearch) {
          await semanticToggle.click();
        }
      }
    }
  }
}
