import { describe, expect, it } from 'vitest';

import { makeAppSettings } from '../../tests/fixtures/appSettings';
import { resolveChatModel } from '../chatModelSelection';

describe('chat provider selection', () => {
  it('prefers configured llama.cpp over the legacy Ollama shortcut in Auto', () => {
    const settings = makeAppSettings().llm;
    settings.provider = 'auto';
    settings.llamaCpp.model = 'qwen.gguf';
    expect(resolveChatModel(settings, '__ollama_server__')).toBe('qwen.gguf');
    expect(resolveChatModel(settings, null)).toBe('qwen.gguf');
    expect(resolveChatModel(settings, 'local.gguf')).toBe('local.gguf');
    settings.provider = 'local';
    expect(resolveChatModel(settings, '__ollama_server__')).toBeNull();
  });
  it('enables llama.cpp without a download and gives it precedence over local models', () => {
    const settings = makeAppSettings().llm;
    settings.provider = 'llamacpp';
    settings.llamaCpp.model = 'qwen.gguf';
    expect(resolveChatModel(settings, null)).toBe('qwen.gguf');
    expect(resolveChatModel(settings, 'local.gguf')).toBe('qwen.gguf');
    settings.llamaCpp.model = '';
    expect(resolveChatModel(settings, 'local.gguf')).toBeNull();
  });
  it('preserves explicit Ollama and local selection and auto fallback', () => {
    const settings = makeAppSettings().llm;
    settings.model = 'ollama-model';
    settings.provider = 'ollama';
    expect(resolveChatModel(settings, 'local.gguf')).toBe('ollama-model');
    settings.provider = 'local';
    expect(resolveChatModel(settings, null)).toBeNull();
    settings.provider = 'auto';
    expect(resolveChatModel(settings, 'local.gguf')).toBe('local.gguf');
    expect(resolveChatModel(settings, null)).toBe('ollama-model');
  });
});
