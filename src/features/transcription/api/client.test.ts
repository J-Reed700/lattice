import { beforeEach, describe, expect, it } from 'vitest';

import { mockIPC } from '@/tests/setup';

import { transcriptionApi } from './client';

const lastCall = () => mockIPC.mock.calls.at(-1) as [string, Record<string, unknown>];

beforeEach(() => { mockIPC.mockReset(); mockIPC.mockResolvedValue(null); });

describe('transcriptionApi', () => {
  it('sends a recording to be heard', async () => {
    mockIPC.mockResolvedValueOnce({ text: 'Hello.', language: 'en', durationMs: 900 });
    const result = await transcriptionApi.transcribeSpeech('AAAA');
    expect(lastCall()).toEqual(['plugin:transcription|transcribe_speech', { pcm: 'AAAA' }]);
    expect(result).toEqual({ ok: true, data: { text: 'Hello.', language: 'en', durationMs: 900 } });
  });

  it('asks whether a model is downloaded', async () => {
    await transcriptionApi.getTranscriptionStatus();
    expect(lastCall()[0]).toBe('plugin:transcription|get_transcription_status');
  });
});
