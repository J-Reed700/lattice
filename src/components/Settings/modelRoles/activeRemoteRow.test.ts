import { describe, expect, it } from 'vitest';

import { activeRemoteRow } from './activeRemoteRow';

import type { LLMSettings } from '../../../types/api/settings';

// activeRemoteRow only reads `provider` and `llamaCpp.url`, so the fixture
// supplies just those and casts — no need to mirror the whole LLMSettingsDto.
function settings(overrides: Partial<LLMSettings> = {}): LLMSettings {
  return {
    provider: 'auto',
    llamaCpp: { url: '', model: '', authHeaderName: '', authHeaderValue: '' },
    ...overrides,
  } as LLMSettings;
}

describe('activeRemoteRow', () => {
  it('shows Ollama when nothing is configured', () => {
    expect(activeRemoteRow(settings())).toBe('ollama');
  });

  it('shows llama.cpp when a connection is set up under Auto', () => {
    expect(
      activeRemoteRow(
        settings({
          llamaCpp: { url: 'http://localhost:8080', model: 'qwen', authHeaderName: '', authHeaderValue: '' },
        }),
      ),
    ).toBe('llamacpp');
  });

  it('treats a whitespace-only llama.cpp URL as not configured', () => {
    expect(
      activeRemoteRow(
        settings({ llamaCpp: { url: '   ', model: 'qwen', authHeaderName: '', authHeaderValue: '' } }),
      ),
    ).toBe('ollama');
  });

  it('prefers an explicit Ollama provider even when llama.cpp is also set up', () => {
    expect(
      activeRemoteRow(
        settings({
          provider: 'ollama',
          llamaCpp: { url: 'http://localhost:8080', model: 'qwen', authHeaderName: '', authHeaderValue: '' },
        }),
      ),
    ).toBe('ollama');
  });

  it('prefers an explicit llama.cpp provider even before the URL is filled in', () => {
    expect(activeRemoteRow(settings({ provider: 'llamacpp' }))).toBe('llamacpp');
  });

  it('falls back to Ollama when settings are not loaded yet', () => {
    expect(activeRemoteRow(null)).toBe('ollama');
    expect(activeRemoteRow(undefined)).toBe('ollama');
  });
});
