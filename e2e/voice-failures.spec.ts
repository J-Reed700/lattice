import { expect } from '@playwright/test';

import { journeyTest } from './fixtures/shell';
import { endVoice, installMissingWhisper, installVoiceFixture, openChat, startVoice } from './fixtures/voice';

journeyTest(installMissingWhisper)('the mic says where to get Whisper when it is not downloaded', async ({ page }) => {
  await openChat(page);
  await startVoice(page).click();

  await expect(page.getByText('Download Whisper first')).toBeVisible();
  await expect(startVoice(page)).toBeVisible();
  await page.getByRole('button', { name: 'Open settings' }).click();
  await expect(page).toHaveURL(/tab=read-aloud/);
});

journeyTest(installVoiceFixture)('a mic that can’t open says why, and can be pressed again', async ({ page, context }) => {
  await context.clearPermissions();
  await openChat(page);
  await startVoice(page).click();

  await expect(page.getByText("Couldn't use the microphone")).toBeVisible({ timeout: 20_000 });
  await expect(endVoice(page)).toHaveCount(0);
  await expect(startVoice(page)).toBeEnabled();
});
