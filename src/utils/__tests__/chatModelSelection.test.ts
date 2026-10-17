import { describe, expect, it } from 'vitest';

import type { DownloadedModel } from '@/types/downloadedModels';

import { makeAppSettings } from '../../tests/fixtures/appSettings';
import {
  buildChatModelOptions,
  cleanModelDisplayName,
  formatModelSize,
  formatServerEndpoint,
  LLAMACPP_SERVER_MODEL_ID,
  OLLAMA_SERVER_MODEL_ID,
  resolveActiveChatOption,
  resolveChatModel,
} from '../chatModelSelection';

const makeModel = (overrides: Partial<DownloadedModel>): DownloadedModel => ({
  id: 'row-1',
  model_name: 'Llama 3.2 3B',
  model_id: 'llama-3.2-3b',
  file_path: '/models/llama.gguf',
  file_size_bytes: 3.2 * 1024 * 1024 * 1024,
  downloaded_at: '2026-01-01T00:00:00Z',
  last_used_at: null,
  use_count: 0,
  is_active_for_chat: false,
  is_active_for_embedding: false,
  is_active_for_utility: false,
  backend: 'local',
  model_type: 'language_model',
  ...overrides,
});

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
  it('resolves cloud providers including Google to configured model', () => {
    const settings = makeAppSettings().llm;
    settings.model = 'gemini-2.5-flash';
    settings.provider = 'google';
    expect(resolveChatModel(settings, 'local.gguf')).toBe('gemini-2.5-flash');
    expect(resolveChatModel(settings, null)).toBe('gemini-2.5-flash');
  });
});

describe('cleanModelDisplayName', () => {
  it('strips /models/ from /models/Qwen3.8-27B-Q5_K_S.gguf and preserves ornith-ai/Ornith-1.5-9B-GGUF', () => {
    expect(cleanModelDisplayName('/models/Qwen3.8-27B-Q5_K_S.gguf')).toBe('Qwen3.8-27B-Q5_K_S.gguf');
    expect(cleanModelDisplayName('ornith-ai/Ornith-1.5-9B-GGUF')).toBe('ornith-ai/Ornith-1.5-9B-GGUF');
    expect(cleanModelDisplayName('Llama 3.2 3B')).toBe('Llama 3.2 3B');
    expect(cleanModelDisplayName('')).toBe('');
    expect(cleanModelDisplayName('   ')).toBe('');
  });

  it('handles other path formats including relative, home dir, and windows paths', () => {
    expect(cleanModelDisplayName('./relative/path/model.gguf')).toBe('model.gguf');
    expect(cleanModelDisplayName('../parent/model.gguf')).toBe('model.gguf');
    expect(cleanModelDisplayName('~/models/deepseek.gguf')).toBe('deepseek.gguf');
    expect(cleanModelDisplayName('C:\\Users\\Lattice\\models\\custom.gguf')).toBe('custom.gguf');
  });
});

describe('formatServerEndpoint and formatModelSize', () => {
  it('formats server endpoints cleanly', () => {
    expect(formatServerEndpoint('http://localhost:11434/')).toBe('localhost:11434');
    expect(formatServerEndpoint('https://api.ollama.com/v1/')).toBe('api.ollama.com/v1');
    expect(formatServerEndpoint('')).toBeNull();
  });

  it('formats model sizes in MB and GB', () => {
    expect(formatModelSize(0)).toBeNull();
    expect(formatModelSize(500 * 1024)).toBeNull();
    expect(formatModelSize(500 * 1024 * 1024)).toBe('500 MB');
    expect(formatModelSize(3.8 * 1024 * 1024 * 1024)).toBe('3.8 GB');
  });
});

describe('buildChatModelOptions', () => {
  it('hides the synthetic __ollama_server__ row when Ollama is not configured, includes configured llama.cpp with triggerLabel, and includes local downloaded models', () => {
    const settings = makeAppSettings().llm;
    settings.provider = 'llamacpp';
    settings.ollamaConfigured = false;
    settings.ollamaUrl = 'http://localhost:11434';
    settings.model = '';
    settings.llamaCpp.url = 'http://127.0.0.1:8080';
    settings.llamaCpp.model = '/models/Qwen3.8-27B-Q5_K_S.gguf';

    const localModel = makeModel({
      id: 'local-1',
      model_id: 'qwen-local',
      model_name: 'Qwen 3.8B',
      file_size_bytes: 4 * 1024 * 1024 * 1024,
      is_active_for_chat: false,
    });
    const syntheticOllama = makeModel({
      id: 'ollama-server',
      model_id: OLLAMA_SERVER_MODEL_ID,
      model_name: 'Ollama Server',
      backend: 'ollama',
    });

    const options = buildChatModelOptions(settings, [localModel, syntheticOllama]);

    // Synthetic __ollama_server__ is NOT present because Ollama is not configured
    expect(options.some((o) => o.id === OLLAMA_SERVER_MODEL_ID)).toBe(false);

    // Configured llama.cpp is present and active
    const llamaCppOption = options.find((o) => o.id === LLAMACPP_SERVER_MODEL_ID);
    expect(llamaCppOption).toBeDefined();
    expect(llamaCppOption?.triggerLabel).toBe('llama.cpp · Qwen3.8-27B-Q5_K_S.gguf');
    expect(llamaCppOption?.label).toBe('Qwen3.8-27B-Q5_K_S.gguf');
    expect(llamaCppOption?.subtitle).toBe('127.0.0.1:8080');
    expect(llamaCppOption?.isActive).toBe(true);

    // Local model is present
    const localOption = options.find((o) => o.id === 'qwen-local');
    expect(localOption).toBeDefined();
    expect(localOption?.label).toBe('Qwen 3.8B');
    expect(localOption?.triggerLabel).toBe('Qwen 3.8B');
    expect(localOption?.subtitle).toBe('4.0 GB · qwen-local');
    expect(localOption?.isActive).toBe(false);
  });
});

describe('resolveActiveChatOption', () => {
  it('marks the local model active when provider === local and llama.cpp active when provider === llamacpp', () => {
    const settings = makeAppSettings().llm;
    settings.provider = 'local';
    settings.llamaCpp.url = 'http://127.0.0.1:8080';
    settings.llamaCpp.model = '/models/Qwen3.8-27B-Q5_K_S.gguf';

    const localModel = makeModel({
      id: 'local-1',
      model_id: 'qwen-local',
      model_name: 'Qwen 3.8B',
      is_active_for_chat: true,
    });

    // When provider is 'local'
    const activeLocalOption = resolveActiveChatOption(settings, [localModel]);
    expect(activeLocalOption).toBeDefined();
    expect(activeLocalOption?.id).toBe('qwen-local');
    expect(activeLocalOption?.isActive).toBe(true);
    expect(activeLocalOption?.backend).toBe('local');

    // When provider is 'llamacpp'
    settings.provider = 'llamacpp';
    const activeLlamaOption = resolveActiveChatOption(settings, [localModel]);
    expect(activeLlamaOption).toBeDefined();
    expect(activeLlamaOption?.id).toBe(LLAMACPP_SERVER_MODEL_ID);
    expect(activeLlamaOption?.triggerLabel).toBe('llama.cpp · Qwen3.8-27B-Q5_K_S.gguf');
    expect(activeLlamaOption?.isActive).toBe(true);
    expect(activeLlamaOption?.backend).toBe('llamacpp');
  });
});
