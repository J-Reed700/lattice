import { describe, expect, it } from 'vitest';

import type { DownloadedModel } from '@/types/downloadedModels';

import { makeAppSettings } from '../../tests/fixtures/appSettings';
import {
  buildChatModelOptions,
  cleanModelDisplayName,
  CLOUD_PROVIDER_MODEL_ID,
  contextFitWarning,
  formatContextLabel,
  formatModelSize,
  formatServerEndpoint,
  isUnavailableChatModel,
  LLAMACPP_SERVER_MODEL_ID,
  modelRefForOption,
  modelRefForRow,
  OLLAMA_SERVER_MODEL_ID,
  resolveDefaultChatOption,
  resolveEffectiveChatOption,
  tokensThatFit,
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

const settingsWith = (patch: (llm: ReturnType<typeof makeAppSettings>['llm']) => void) => {
  const llm = makeAppSettings().llm;
  patch(llm);
  return llm;
};

describe('the Settings default as a picker row', () => {
  it('prefers a configured llama.cpp over the legacy Ollama shortcut in Auto, and a local model over both', () => {
    const settings = settingsWith((llm) => {
      llm.provider = 'auto';
      llm.llamaCpp.url = 'http://127.0.0.1:8080';
      llm.llamaCpp.model = 'qwen.gguf';
    });
    const ollamaRow = makeModel({ id: 'o', model_id: OLLAMA_SERVER_MODEL_ID, backend: 'ollama', is_active_for_chat: true });
    expect(resolveDefaultChatOption(settings, [ollamaRow])?.ref).toEqual({ kind: 'llama_cpp', model: 'qwen.gguf' });
    expect(resolveDefaultChatOption(settings, [])?.ref).toEqual({ kind: 'llama_cpp', model: 'qwen.gguf' });
    const local = makeModel({ model_id: 'local.gguf', is_active_for_chat: true });
    expect(resolveDefaultChatOption(settings, [local])?.ref).toEqual({ kind: 'local', model_id: 'local.gguf' });
    settings.provider = 'local';
    expect(resolveDefaultChatOption(settings, [ollamaRow])).toBeNull();
  });

  it('gives llama.cpp precedence over a local model when it is the provider, and nothing without its model', () => {
    const settings = settingsWith((llm) => {
      llm.provider = 'llamacpp';
      llm.llamaCpp.url = 'http://127.0.0.1:8080';
      llm.llamaCpp.model = 'qwen.gguf';
    });
    const local = makeModel({ model_id: 'local.gguf', is_active_for_chat: true });
    expect(resolveDefaultChatOption(settings, [local])?.ref).toEqual({ kind: 'llama_cpp', model: 'qwen.gguf' });
    settings.llamaCpp.model = '';
    expect(resolveDefaultChatOption(settings, [local])).toBeNull();
  });

  it('follows an explicit Ollama provider, and falls back to it in Auto', () => {
    const settings = settingsWith((llm) => {
      llm.model = 'ollama-model';
      llm.provider = 'ollama';
      llm.ollamaConfigured = true;
    });
    const local = makeModel({ model_id: 'local.gguf', is_active_for_chat: true });
    expect(resolveDefaultChatOption(settings, [local])?.ref).toEqual({ kind: 'ollama', model: 'ollama-model' });
    settings.provider = 'local';
    expect(resolveDefaultChatOption(settings, [])).toBeNull();
    settings.provider = 'auto';
    expect(resolveDefaultChatOption(settings, [local])?.ref).toEqual({ kind: 'local', model_id: 'local.gguf' });
    expect(resolveDefaultChatOption(settings, [])?.ref).toEqual({ kind: 'ollama', model: 'ollama-model' });
  });

  it('names a cloud provider and its model, Google included', () => {
    const settings = settingsWith((llm) => {
      llm.model = 'gemini-2.5-flash';
      llm.provider = 'google';
    });
    const option = resolveDefaultChatOption(settings, [makeModel({ is_active_for_chat: true })]);
    expect(option?.ref).toEqual({ kind: 'cloud', provider: 'google', model: 'gemini-2.5-flash' });
    expect(option?.triggerLabel).toBe('Google · gemini-2.5-flash');
  });
});

describe('modelRefForOption', () => {
  const settings = settingsWith((llm) => {
    llm.provider = 'openai';
    llm.model = ' gpt-x ';
    llm.llamaCpp.model = '/models/served.gguf';
  });

  it('maps all five row shapes to the model they name', () => {
    expect(modelRefForOption({ id: LLAMACPP_SERVER_MODEL_ID, backend: 'llamacpp' }, settings))
      .toEqual({ kind: 'llama_cpp', model: '/models/served.gguf' });
    expect(modelRefForOption({ id: OLLAMA_SERVER_MODEL_ID, backend: 'ollama' }, settings))
      .toEqual({ kind: 'ollama', model: 'gpt-x' });
    expect(modelRefForOption({ id: CLOUD_PROVIDER_MODEL_ID, backend: 'cloud' }, settings))
      .toEqual({ kind: 'cloud', provider: 'openai', model: 'gpt-x' });
    expect(modelRefForOption({ id: 'ornith-1.5-9b', backend: 'local' }, settings))
      .toEqual({ kind: 'local', model_id: 'ornith-1.5-9b' });
    // An explicit Ollama row names its own tag, not the Settings tag.
    expect(modelRefForOption({ id: 'qwen3:8b', backend: 'ollama' }, settings))
      .toEqual({ kind: 'ollama', model: 'qwen3:8b' });
  });

  it('refuses a cloud row when Settings names no cloud provider', () => {
    const local = settingsWith((llm) => { llm.provider = 'local'; });
    expect(() => modelRefForOption({ id: CLOUD_PROVIDER_MODEL_ID, backend: 'cloud' }, local)).toThrow();
  });

  it('puts the same reference on each built row', () => {
    const withConnections = settingsWith((llm) => {
      llm.provider = 'ollama';
      llm.model = 'llama3.2:latest';
      llm.llamaCpp.url = 'http://127.0.0.1:8080';
      llm.llamaCpp.model = 'served.gguf';
    });
    const rows = [makeModel({ model_id: 'local-a' }), makeModel({ id: 'o2', model_id: 'qwen3:8b', backend: 'ollama' })];
    for (const option of buildChatModelOptions(withConnections, rows)) {
      expect(option.ref).toEqual(modelRefForOption(option, withConnections));
    }
  });
});

describe('modelRefForRow', () => {
  it('names a local row, an explicit Ollama row, and the connection row by its Settings tag', () => {
    const settings = settingsWith((llm) => { llm.model = 'qwen3:8b'; });
    expect(modelRefForRow(makeModel({ model_id: 'local-a' }), settings)).toEqual({ kind: 'local', model_id: 'local-a' });
    expect(modelRefForRow(makeModel({ model_id: 'mistral:latest', backend: 'ollama' }), settings))
      .toEqual({ kind: 'ollama', model: 'mistral:latest' });
    expect(modelRefForRow(makeModel({ model_id: OLLAMA_SERVER_MODEL_ID, backend: 'ollama' }), settings))
      .toEqual({ kind: 'ollama', model: 'qwen3:8b' });
    expect(modelRefForRow(makeModel({ model_id: OLLAMA_SERVER_MODEL_ID, backend: 'ollama' }), undefined)).toBeNull();
  });
});

describe('resolveEffectiveChatOption', () => {
  const settings = settingsWith((llm) => {
    llm.provider = 'local';
    llm.ollamaConfigured = true;
    llm.model = 'llama3.2:latest';
  });
  const big = makeModel({ id: 'r1', model_id: 'qwen3.5-27b', model_name: 'Qwen3.5-27B', is_active_for_chat: true });
  const small = makeModel({ id: 'r2', model_id: 'ornith-1.5-9b', model_name: 'Ornith-1.5-9B' });

  it('gives the Settings default to a chat on the default, and nothing when none is set', () => {
    expect(resolveEffectiveChatOption({ kind: 'default' }, settings, [big, small])).toMatchObject({ id: 'qwen3.5-27b', isDefault: true });
    expect(resolveEffectiveChatOption({ kind: 'default' }, settings, [small])).toBeNull();
  });

  it("gives a chat its own model even when Settings names another", () => {
    const chosen = resolveEffectiveChatOption(
      { kind: 'explicit', model: { kind: 'local', model_id: 'ornith-1.5-9b' } }, settings, [big, small]);
    expect(chosen).toMatchObject({ id: 'ornith-1.5-9b', label: 'Ornith-1.5-9B', isDefault: false });
  });

  it('reports a deleted local model as unavailable instead of substituting another', () => {
    const chosen = resolveEffectiveChatOption(
      { kind: 'explicit', model: { kind: 'local', model_id: 'ornith-1.5-9b' } }, settings, [big]);
    expect(isUnavailableChatModel(chosen)).toBe(true);
    expect(chosen).toEqual({
      unavailable: true,
      label: 'ornith-1.5-9b',
      reason: 'This chat is set to ornith-1.5-9b, which is no longer downloaded.',
    });
  });

  it('keeps a remote tag Settings has moved on from, and refuses one whose connection is gone', () => {
    const tag = { kind: 'explicit' as const, model: { kind: 'ollama' as const, model: 'qwen3:8b' } };
    expect(resolveEffectiveChatOption(tag, settings, [big])).toMatchObject({
      ref: { kind: 'ollama', model: 'qwen3:8b' }, triggerLabel: 'Ollama · qwen3:8b', backend: 'ollama',
    });
    const noOllama = settingsWith((llm) => { llm.ollamaUrl = ' '; });
    expect(resolveEffectiveChatOption(tag, noOllama, [big])).toEqual({
      unavailable: true,
      label: 'qwen3:8b',
      reason: 'This chat uses qwen3:8b on Ollama, but no Ollama connection is configured.',
    });
    const llama = { kind: 'explicit' as const, model: { kind: 'llama_cpp' as const, model: 'served.gguf' } };
    const noLlama = settingsWith((llm) => { llm.llamaCpp.url = ''; });
    expect(isUnavailableChatModel(resolveEffectiveChatOption(llama, noLlama, []))).toBe(true);
  });

  it('keeps a cloud model after Settings moves to another provider', () => {
    const cloud = { kind: 'explicit' as const, model: { kind: 'cloud' as const, provider: 'anthropic' as const, model: 'claude-x' } };
    expect(resolveEffectiveChatOption(cloud, settings, [big])).toMatchObject({ triggerLabel: 'Anthropic · claude-x', backend: 'cloud' });
  });

  it('assumes nothing is missing before Settings has loaded', () => {
    const tag = { kind: 'explicit' as const, model: { kind: 'ollama' as const, model: 'qwen3:8b' } };
    expect(isUnavailableChatModel(resolveEffectiveChatOption(tag, undefined, []))).toBe(false);
  });
});

describe('formatContextLabel', () => {
  it('shows a saved window exactly and the trained window it loads with otherwise rounded', () => {
    expect(formatContextLabel({ configured_context_length: 8704, trained_context_length: 32768 })).toBe('8,704 ctx · set by you');
    expect(formatContextLabel({ configured_context_length: null, trained_context_length: 32768 })).toBe('32K ctx');
    expect(formatContextLabel({ configured_context_length: null, trained_context_length: null })).toBeNull();
  });
});

describe('contextFitWarning', () => {
  const model = { fits_context_length: 192_512, fits_context_length_with_kv_quantization: 262_144 };

  it('warns only past what fits, for the KV cache the model uses', () => {
    expect(contextFitWarning(model, 262_144, false)).toBe(
      "May not fit in this machine's memory — about 192,512 tokens fit",
    );
    expect(contextFitWarning(model, 192_512, false)).toBeNull();
    expect(contextFitWarning(model, 262_144, true)).toBeNull();
    expect(tokensThatFit(model, 262_144, false)).toBe(192_512);
  });

  it('says nothing when the machine does not report its memory', () => {
    expect(contextFitWarning({ fits_context_length: null }, 262_144, false)).toBeNull();
    expect(contextFitWarning({}, 262_144, false)).toBeNull();
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

    // Configured llama.cpp is present, and it is the Settings default
    const llamaCppOption = options.find((o) => o.id === LLAMACPP_SERVER_MODEL_ID);
    expect(llamaCppOption).toBeDefined();
    expect(llamaCppOption?.triggerLabel).toBe('llama.cpp · Qwen3.8-27B-Q5_K_S.gguf');
    expect(llamaCppOption?.label).toBe('Qwen3.8-27B-Q5_K_S.gguf');
    expect(llamaCppOption?.subtitle).toBe('127.0.0.1:8080');
    expect(llamaCppOption?.isDefault).toBe(true);

    // Local model is present
    const localOption = options.find((o) => o.id === 'qwen-local');
    expect(localOption).toBeDefined();
    expect(localOption?.label).toBe('Qwen 3.8B');
    expect(localOption?.triggerLabel).toBe('Qwen 3.8B');
    expect(localOption?.subtitle).toBe('4.0 GB · qwen-local');
    expect(localOption?.isDefault).toBe(false);
  });
});

describe('resolveDefaultChatOption', () => {
  it('names the local model the default when provider === local, and llama.cpp when provider === llamacpp', () => {
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
    const defaultLocalOption = resolveDefaultChatOption(settings, [localModel]);
    expect(defaultLocalOption).toBeDefined();
    expect(defaultLocalOption?.id).toBe('qwen-local');
    expect(defaultLocalOption?.isDefault).toBe(true);
    expect(defaultLocalOption?.backend).toBe('local');

    // When provider is 'llamacpp'
    settings.provider = 'llamacpp';
    const defaultLlamaOption = resolveDefaultChatOption(settings, [localModel]);
    expect(defaultLlamaOption).toBeDefined();
    expect(defaultLlamaOption?.id).toBe(LLAMACPP_SERVER_MODEL_ID);
    expect(defaultLlamaOption?.triggerLabel).toBe('llama.cpp · Qwen3.8-27B-Q5_K_S.gguf');
    expect(defaultLlamaOption?.isDefault).toBe(true);
    expect(defaultLlamaOption?.backend).toBe('llamacpp');
  });
});
