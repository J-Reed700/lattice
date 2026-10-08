import { describe, expect, it } from 'vitest';

import { configuredConnections } from './configuredConnections';
import { makeAppSettings } from '../../../tests/fixtures/appSettings';

describe('configured connections', () => {
  it('shows no connection before settings load', () => {
    expect(configuredConnections(undefined)).toEqual({ ollama: false, llamaCpp: false });
  });

  it.each(['auto', 'local', 'llamacpp', 'openai', 'anthropic'] as const)(
    'does not mistake the shared model setting for an Ollama connection under %s', provider => {
      const { llm } = makeAppSettings();
      llm.provider = provider;
      llm.model = 'any-model-id';
      expect(configuredConnections(llm)).toEqual({ ollama: false, llamaCpp: false });
    },
  );

  it('recognizes a saved endpoint without probing it or requiring it to be selected', () => {
    const { llm } = makeAppSettings();
    llm.ollamaUrl = 'https://ollama.example.com';
    expect(configuredConnections(llm).ollama).toBe(true);
  });

  it('treats a trailing slash on the default address as a default', () => {
    const { llm } = makeAppSettings();
    llm.ollamaUrl = ' http://localhost:11434/ ';
    expect(configuredConnections(llm).ollama).toBe(false);
  });

  it.each(['ollamaUrl', 'model'] as const)('does not show an incomplete saved Ollama connection with no %s', field => {
    const { llm } = makeAppSettings();
    llm.ollamaConfigured = true;
    llm[field] = ' ';
    expect(configuredConnections(llm).ollama).toBe(false);
  });
});
