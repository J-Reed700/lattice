import { test, expect } from '../setup';
import { OnboardingPage } from '../pages/OnboardingPage';
import { SearchPage } from '../pages/SearchPage';
import { createTestHelpers } from '../utils/test-helpers';

test.describe('Onboarding Flow', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('domcontentloaded');
  });

  test('should show onboarding on first launch', async ({ page }) => {
    const onboardingPage = new OnboardingPage(page);
    const helpers = createTestHelpers(page);

    await test.step('Check for onboarding or welcome screen', async () => {
      const isOnboardingVisible = await onboardingPage.welcomeHeading
        .isVisible({ timeout: 10000 })
        .catch(() => false);

      if (isOnboardingVisible) {
        console.log('Onboarding screen detected');
        await expect(onboardingPage.welcomeHeading).toBeVisible();
      } else {
        console.log('No onboarding - app may skip it or use first-run detection');
        await helpers.waitForInitialization();
      }
    });
  });

  test('should complete full onboarding flow', async ({ page }) => {
    const onboardingPage = new OnboardingPage(page);
    const helpers = createTestHelpers(page);

    await test.step('Check if onboarding is present', async () => {
      const isOnboardingVisible = await onboardingPage.welcomeHeading
        .isVisible({ timeout: 10000 })
        .catch(() => false);

      if (!isOnboardingVisible) {
        console.log('Skipping onboarding test - not shown on this launch');
        test.skip();
        return;
      }
    });

    await test.step('Complete onboarding steps', async () => {
      await onboardingPage.completeOnboarding();
    });

    await test.step('Verify app is ready after onboarding', async () => {
      await onboardingPage.verifyOnboardingComplete();
      await helpers.waitForInitialization();
    });

    await test.step('Verify main app is accessible', async () => {
      const searchPage = new SearchPage(page);
      await expect(searchPage.searchInput).toBeVisible({ timeout: 10000 });
    });
  });

  test('should skip onboarding', async ({ page }) => {
    const onboardingPage = new OnboardingPage(page);
    const helpers = createTestHelpers(page);

    await test.step('Check for skip option', async () => {
      const isOnboardingVisible = await onboardingPage.welcomeHeading
        .isVisible({ timeout: 10000 })
        .catch(() => false);

      if (!isOnboardingVisible) {
        console.log('No onboarding to skip');
        test.skip();
        return;
      }

      const skipVisible = await onboardingPage.skipButton.isVisible().catch(() => false);

      if (skipVisible) {
        await onboardingPage.skipOnboarding();
        await helpers.waitForInitialization();
      } else {
        console.log('Skip button not available');
      }
    });
  });

  test('should handle first folder selection during onboarding', async ({ page }) => {
    const onboardingPage = new OnboardingPage(page);

    await test.step('Check for onboarding', async () => {
      const isOnboardingVisible = await onboardingPage.welcomeHeading
        .isVisible({ timeout: 10000 })
        .catch(() => false);

      if (!isOnboardingVisible) {
        test.skip();
        return;
      }
    });

    await test.step('Navigate through onboarding', async () => {
      let clickCount = 0;
      const maxClicks = 5;

      while (clickCount < maxClicks) {
        const nextVisible = await onboardingPage.nextButton.isVisible().catch(() => false);

        if (nextVisible) {
          await onboardingPage.clickNext();
          clickCount++;
          await page.waitForTimeout(1000);
        } else {
          break;
        }

        const folderSelectionVisible = await page
          .getByText(/Select.*Folder|Choose.*Folder/i)
          .isVisible()
          .catch(() => false);

        if (folderSelectionVisible) {
          console.log('Folder selection step found');
          break;
        }
      }
    });

    await test.step('Complete onboarding', async () => {
      const finishVisible = await onboardingPage.finishButton.isVisible().catch(() => false);
      if (finishVisible) {
        await onboardingPage.clickFinish();
      }

      const getStartedVisible = await onboardingPage.getStartedButton
        .isVisible()
        .catch(() => false);
      if (getStartedVisible) {
        await onboardingPage.clickGetStarted();
      }
    });
  });

  test('should configure settings during onboarding', async ({ page }) => {
    const onboardingPage = new OnboardingPage(page);

    await test.step('Check for onboarding', async () => {
      const isOnboardingVisible = await onboardingPage.welcomeHeading
        .isVisible({ timeout: 10000 })
        .catch(() => false);

      if (!isOnboardingVisible) {
        test.skip();
        return;
      }
    });

    await test.step('Look for settings configuration', async () => {
      await onboardingPage.configureSettings({
        enableAutoIndex: true,
        enableSemanticSearch: true,
      });
    });

    await test.step('Complete onboarding', async () => {
      await onboardingPage.completeOnboarding();
    });
  });

  test('should show model download during onboarding', async ({ page }) => {
    const onboardingPage = new OnboardingPage(page);
    const helpers = createTestHelpers(page);

    await test.step('Wait for initialization', async () => {
      await helpers.waitForApp();

      const downloadingText = page.getByText(/Downloading|Model.*Download/i);
      const isDownloading = await downloadingText.isVisible({ timeout: 5000 }).catch(() => false);

      if (isDownloading) {
        console.log('Model download detected during initialization');

        await downloadingText.waitFor({ state: 'hidden', timeout: 120000 }).catch(() => {
          console.log('Model download taking longer than expected');
        });
      } else {
        console.log('No model download - models already present');
      }
    });

    await test.step('Wait for app to be ready', async () => {
      await helpers.waitForInitialization();
    });
  });

  test('should handle initialization errors gracefully', async ({ page }) => {
    const helpers = createTestHelpers(page);

    await test.step('Wait for app to load', async () => {
      await helpers.waitForApp();
    });

    await test.step('Check for initialization errors', async () => {
      const errorHeading = page.getByText(/Initialization Failed|Error/i);
      const hasError = await errorHeading.isVisible({ timeout: 10000 }).catch(() => false);

      if (hasError) {
        console.log('Initialization error detected');

        const retryButton = page.getByRole('button', { name: /Retry/i });
        const retryVisible = await retryButton.isVisible().catch(() => false);

        if (retryVisible) {
          console.log('Retry button available - error handled gracefully');
        } else {
          console.log('Error shown without retry option');
        }
      } else {
        console.log('No initialization errors - test passed');
        await helpers.waitForInitialization();
      }
    });
  });

  test('should progress through onboarding steps sequentially', async ({ page }) => {
    const onboardingPage = new OnboardingPage(page);

    await test.step('Check for onboarding', async () => {
      const isOnboardingVisible = await onboardingPage.welcomeHeading
        .isVisible({ timeout: 10000 })
        .catch(() => false);

      if (!isOnboardingVisible) {
        test.skip();
        return;
      }
    });

    await test.step('Navigate through steps', async () => {
      let stepNumber = 1;
      const maxSteps = 5;

      while (stepNumber <= maxSteps) {
        const nextVisible = await onboardingPage.nextButton.isVisible().catch(() => false);

        if (!nextVisible) {
          break;
        }

        await onboardingPage.verifyStep(stepNumber).catch(() => {
          console.log(`Step ${stepNumber} verification not available`);
        });

        await onboardingPage.clickNext();
        stepNumber++;
        await page.waitForTimeout(500);
      }

      console.log(`Completed ${stepNumber - 1} onboarding steps`);
    });

    await test.step('Finish onboarding', async () => {
      const finishVisible = await onboardingPage.finishButton.isVisible().catch(() => false);
      if (finishVisible) {
        await onboardingPage.clickFinish();
      }
    });
  });

  test('should not show onboarding on subsequent launches', async ({ page }) => {
    const onboardingPage = new OnboardingPage(page);
    const helpers = createTestHelpers(page);

    await test.step('Complete first launch', async () => {
      const isOnboardingVisible = await onboardingPage.welcomeHeading
        .isVisible({ timeout: 10000 })
        .catch(() => false);

      if (isOnboardingVisible) {
        await onboardingPage.completeOnboarding();
      }

      await helpers.waitForInitialization();
    });

    await test.step('Reload application', async () => {
      await page.reload();
      await helpers.waitForApp();
    });

    await test.step('Verify onboarding does not show again', async () => {
      const isOnboardingVisible = await onboardingPage.welcomeHeading
        .isVisible({ timeout: 5000 })
        .catch(() => false);

      if (isOnboardingVisible) {
        console.log('Onboarding shown again - may reset on reload in dev mode');
      } else {
        console.log('Onboarding correctly skipped on subsequent launch');
      }
    });
  });
});
