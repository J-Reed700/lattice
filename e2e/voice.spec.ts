import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { expect, type Page } from '@playwright/test';

import { ANSWER_CHUNKS, CONVERSATION } from './fixtures/chat';
import { argsOf, journeyTest } from './fixtures/shell';
import { serveCommands } from './fixtures/tauri';
import { endVoice, installVoiceFixture, openChat, SAID, startVoice } from './fixtures/voice';

/** A WAV a fake microphone can play: a second of quiet, two seconds of voice-loud tone, then quiet. */
function fakeSpeech(): string {
  const rate = 48_000;
  const samples = Int16Array.from({ length: rate * 8 }, (_, i) => {
    const seconds = i / rate;
    return seconds > 1 && seconds < 3 ? Math.round(6_000 * Math.sin(2 * Math.PI * 300 * seconds)) : 0;
  });
  const header = Buffer.alloc(44);
  header.write('RIFF', 0);
  header.writeUInt32LE(36 + samples.byteLength, 4);
  header.write('WAVEfmt ', 8);
  header.writeUInt32LE(16, 16);
  header.writeUInt16LE(1, 20);
  header.writeUInt16LE(1, 22);
  header.writeUInt32LE(rate, 24);
  header.writeUInt32LE(rate * 2, 28);
  header.writeUInt16LE(2, 32);
  header.writeUInt16LE(16, 34);
  header.write('data', 36);
  header.writeUInt32LE(samples.byteLength, 40);
  const file = join(mkdtempSync(join(tmpdir(), 'lattice-voice-')), 'speech.wav');
  writeFileSync(file, Buffer.concat([header, Buffer.from(samples.buffer)]));
  return file;
}

const test = journeyTest(installVoiceFixture);

test.use({
  launchOptions: {
    args: ['--use-fake-ui-for-media-stream', '--use-fake-device-for-media-stream', `--use-file-for-fake-audio-capture=${fakeSpeech()}`],
  },
});
test.skip(({ browserName }) => browserName !== 'chromium', 'Only Chromium has a fake microphone.');

const status = (page: Page, text: string) => page.getByRole('status').filter({ hasText: text });

async function stream(page: Page, requestId: string, content: string) {
  await page.evaluate(({ conversationId, requestId, content }) => {
    window.__LATTICE_IPC__.emit('llm-stream', { conversationId, requestId, content, done: false });
  }, { conversationId: CONVERSATION.id!, requestId, content });
}

test('a spoken question goes out as a turn, and its answer is spoken as it is written', async ({ page, backend }) => {
  await openChat(page);
  await startVoice(page).click();
  await expect(endVoice(page)).toBeVisible();

  // The worklet's audio reached the detector, which found the speech and its end.
  await expect.poll(() => argsOf(backend, 'transcribe_speech').length, { timeout: 20_000 }).toBeGreaterThan(0);
  const { pcm } = argsOf(backend, 'transcribe_speech')[0] as { pcm: string };
  expect(Buffer.from(pcm, 'base64').length / 2 / 16_000).toBeGreaterThan(1);

  await expect.poll(() => argsOf(backend, 'chat_with_conversation').length).toBe(1);
  const turn = argsOf(backend, 'chat_with_conversation')[0] as { message: string; requestId: string };
  expect(turn.message).toBe(SAID);
  await expect(page.getByText(SAID, { exact: true })).toBeVisible();

  // The first sentence is spoken before the turn is done.
  await stream(page, turn.requestId, ANSWER_CHUNKS[0]);
  await stream(page, turn.requestId, ANSWER_CHUNKS[1]);
  await expect.poll(() => argsOf(backend, 'synthesize_speech').map((call) => call.text)).toContain('It comes down to silica.');
  await expect(status(page, 'Speaking')).toBeVisible();

  backend.state.finish.release();
  await expect(page.locator('#message-msg-answer')).toContainText('It comes down to silica.');
  await expect(status(page, 'Listening…')).toBeVisible({ timeout: 20_000 });

  await endVoice(page).click();
  await expect(startVoice(page)).toBeVisible();
});

test('talking over a reply stops it, and what was said goes out instead', async ({ page, backend }) => {
  // Each sentence plays for longer than the fake microphone takes to come round to its tone again.
  await serveCommands(page, () => ({
    synthesize_speech: () => ({ sampleRate: 24_000, durationMs: 15_000, pcm: Buffer.alloc(24_000 * 15 * 2).toString('base64') }),
  }));
  const turns = () => argsOf(backend, 'chat_with_conversation').filter((call) => !call.cancelOnly);
  const stops = () => argsOf(backend, 'chat_with_conversation').filter((call) => call.cancelOnly);

  await openChat(page);
  await startVoice(page).click();
  await expect.poll(() => turns().length, { timeout: 20_000 }).toBe(1);
  await stream(page, String(turns()[0].requestId), ANSWER_CHUNKS.join(''));
  await expect(status(page, 'Speaking')).toBeVisible();

  await expect.poll(() => stops().length, { timeout: 20_000 }).toBe(1);
  await expect.poll(() => turns().length, { timeout: 20_000 }).toBe(2);
  expect(turns()[1].message).toBe(SAID);

  backend.state.finish.release();
  await expect(page.locator('#message-msg-answer')).toContainText('It comes down to silica.');
  await endVoice(page).click();
});
