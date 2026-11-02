import type { Page } from '@playwright/test';

import { CONVERSATION, installChatFixture } from './chat';
import { serveCommands, type MockArgs } from './tauri';

export const SAID = 'Why do some volcanoes erupt?';

const WHISPER = { modelId: 'whisper-tiny-q8', modelName: 'Whisper Tiny', downloadBytes: 41_841_632, license: 'MIT' };

/** A chat with Whisper and the voices downloaded. */
export async function installVoiceFixture(page: Page) {
  const backend = await installChatFixture(page);
  const { handlers } = await serveCommands(page, () => ({}));
  const speechStatus = handlers.get_speech_status;
  await serveCommands(page, () => ({
    get_speech_status: async (args: MockArgs) => ({ ...(await speechStatus?.(args)), modelReady: true }),
    get_transcription_status: () => ({ modelReady: true, modelName: 'Whisper Tiny', voiceModel: WHISPER }),
    transcribe_speech: () => ({ text: SAID, language: 'en', durationMs: 3_000 }),
    // A fifth of a second of silence at Kokoro's rate.
    synthesize_speech: () => ({ sampleRate: 24_000, durationMs: 200, pcm: Buffer.alloc(9_600).toString('base64') }),
  }));
  return backend;
}

/** A chat with Whisper not yet downloaded. */
export async function installMissingWhisper(page: Page) {
  const backend = await installChatFixture(page);
  await serveCommands(page, () => ({
    get_transcription_status: () => ({ modelReady: false, modelName: null, voiceModel: WHISPER }),
  }));
  return backend;
}

export const startVoice = (page: Page) => page.getByRole('button', { name: 'Start a voice conversation' });
export const endVoice = (page: Page) => page.getByRole('button', { name: 'End the voice conversation' });
export const openChat = (page: Page) => page.goto(`/chat?conversationId=${CONVERSATION.id}`);
