import { beforeEach, describe, expect, it } from 'vitest';

import { mockIPC } from '@/tests/setup';

import { speechApi } from './client';

const lastCall = () => mockIPC.mock.calls.at(-1) as [string, Record<string, unknown>];

beforeEach(() => { mockIPC.mockReset(); mockIPC.mockResolvedValue(null); });

describe('speechApi', () => {
  it('asks the speech plugin whether the voices are downloaded', async () => {
    await speechApi.getSpeechStatus();
    expect(lastCall()[0]).toBe('plugin:speech|get_speech_status');
  });

  it('sends the text, voice and pace to synthesise', async () => {
    mockIPC.mockResolvedValueOnce({ sampleRate: 24000, pcm: '', durationMs: 0 });
    const result = await speechApi.synthesizeSpeech('Hello.', 'bf_emma', 1.25);
    expect(lastCall()).toEqual(['plugin:speech|synthesize_speech', { text: 'Hello.', voice: 'bf_emma', speed: 1.25 }]);
    expect(result).toEqual({ ok: true, data: { sampleRate: 24000, pcm: '', durationMs: 0 } });
  });
});
