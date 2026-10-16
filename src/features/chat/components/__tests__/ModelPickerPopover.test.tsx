import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ModelPickerPopover } from '@/features/chat/components/ModelPickerPopover';
import { makeAppSettings } from '@/tests/fixtures/appSettings';
import type { DownloadedModel } from '@/types/downloadedModels';

const models = vi.hoisted(() => ({ current: [] as DownloadedModel[] }));
const appSettings = vi.hoisted(() => ({
  current: undefined as ReturnType<typeof makeAppSettings> | undefined,
}));

vi.mock('@/features/model/hooks/useDownloadedModels', () => ({
  useDownloadedModels: () => ({ downloadedModels: models.current }),
}));

vi.mock('@/features/settings/hooks/useSettingsQuery', () => ({
  useSettingsQuery: () => ({ data: appSettings.current }),
}));

vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: (selector: (_s: { moveConversationToSpace: () => void }) => unknown) =>
    selector({ moveConversationToSpace: vi.fn() }),
}));

const model = (overrides: Partial<DownloadedModel>): DownloadedModel => ({
  id: 'row-1',
  model_name: 'Llama 3.2 3B',
  model_id: 'llama-3.2-3b',
  file_path: '/models/llama.gguf',
  file_size_bytes: 1,
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

const open = async (activeModelId?: string, onSelect = vi.fn()) => {
  render(
    <ModelPickerPopover activeModelId={activeModelId ?? 'llama-3.2-3b'} onSelect={onSelect}>
      <button type="button">Try with…</button>
    </ModelPickerPopover>
  );
  await userEvent.click(screen.getByText('Try with…'));
};

describe('ModelPickerPopover', () => {
  beforeEach(() => {
    appSettings.current = undefined;
    models.current = [];
  });

  it('lists only chat models', async () => {
    models.current = [
      model({ id: 'a' }),
      model({ id: 'b', model_name: 'BGE small', model_type: 'text_embeddings' }),
    ];
    await open();

    expect(screen.getByText('Llama 3.2 3B')).toBeInTheDocument();
    expect(screen.queryByText('BGE small')).not.toBeInTheDocument();
  });

  it('groups by backend and marks the active row', async () => {
    models.current = [
      model({ id: 'a', is_active_for_chat: true }),
      model({
        id: 'b',
        model_name: 'mistral:latest',
        model_id: 'mistral:latest',
        backend: 'ollama',
      }),
    ];
    await open();

    expect(screen.getByText('Local')).toBeInTheDocument();
    expect(screen.getByText('Ollama')).toBeInTheDocument();
    // The active row is the one the label points at.
    const activeRow = screen.getByText('Llama 3.2 3B').closest('button');
    expect(activeRow?.querySelector('svg.lucide-check')).not.toBeNull();
  });

  it('omits group labels when only one backend is present', async () => {
    models.current = [model({ id: 'a' })];
    await open();

    expect(screen.queryByText('Local')).not.toBeInTheDocument();
    expect(screen.queryByText('Ollama')).not.toBeInTheDocument();
  });

  it('says so plainly when nothing else is installed', async () => {
    models.current = [];
    await open();

    expect(screen.getByText('No other models installed.')).toBeInTheDocument();
  });

  it('shows configured llama.cpp and local model without unconfigured Ollama server row', async () => {
    const s = makeAppSettings();
    s.llm.provider = 'llamacpp';
    s.llm.llamaCpp = {
      url: 'http://localhost:8080',
      model: '/models/Qwen3.8-27B-Q5_K_S.gguf',
      authHeaderName: '',
      authHeaderValue: '',
    };
    s.llm.ollamaConfigured = false;
    appSettings.current = s;

    models.current = [
      model({
        id: 'row-local',
        model_name: 'ornith-ai/Ornith-1.5-9B-GGUF',
        model_id: 'ornith-ai/Ornith-1.5-9B-GGUF',
      }),
      model({
        id: 'row-ollama',
        model_name: 'Ollama Server',
        model_id: '__ollama_server__',
        backend: 'ollama',
      }),
    ];

    const onSelect = vi.fn();
    await open(undefined, onSelect);

    expect(screen.getByText('Local')).toBeInTheDocument();
    expect(screen.getByText('llama.cpp')).toBeInTheDocument();
    expect(screen.queryByText('Ollama')).not.toBeInTheDocument();
    expect(screen.queryByText('Ollama Server')).not.toBeInTheDocument();

    expect(screen.getByText('ornith-ai/Ornith-1.5-9B-GGUF')).toBeInTheDocument();
    expect(screen.getByText('Qwen3.8-27B-Q5_K_S.gguf')).toBeInTheDocument();
    expect(screen.getByText('localhost:8080')).toBeInTheDocument();

    const llamaRow = screen.getByText('Qwen3.8-27B-Q5_K_S.gguf').closest('button');
    expect(llamaRow?.querySelector('svg.lucide-check')).not.toBeNull();

    const localRow = screen.getByText('ornith-ai/Ornith-1.5-9B-GGUF').closest('button');
    expect(localRow?.querySelector('svg.lucide-check')).toBeNull();

    await userEvent.click(screen.getByText('Qwen3.8-27B-Q5_K_S.gguf'));
    expect(onSelect).toHaveBeenCalledWith('__llamacpp_server__', 'llama.cpp · Qwen3.8-27B-Q5_K_S.gguf');
  });

  it('switches the LLM provider when selecting a local model over llama.cpp and back', async () => {
    const { renderHook, act } = await import('@testing-library/react');
    const { useChatSwitchActions } = await import('@/features/chat/hooks/useChatSwitchActions');
    const VaultAPI = (await import('@/lib/api')).default;

    const s = makeAppSettings();
    s.llm.provider = 'llamacpp';
    s.llm.llamaCpp = {
      url: 'http://localhost:8080',
      model: '/models/Qwen3.8-27B-Q5_K_S.gguf',
      authHeaderName: '',
      authHeaderValue: '',
    };
    appSettings.current = s;

    const localOrnith = model({
      id: 'row-local',
      model_name: 'ornith-ai/Ornith-1.5-9B-GGUF',
      model_id: 'ornith-ai/Ornith-1.5-9B-GGUF',
    });
    models.current = [localOrnith];

    const setActiveChatModel = vi.fn().mockResolvedValue(undefined);
    const updateSpy = vi
      .spyOn(VaultAPI, 'updateSettings')
      .mockImplementation(async ({ updates }) => {
        s.llm = { ...s.llm, ...updates };
        return { ok: true, data: s };
      });

    const { result, rerender } = renderHook(() =>
      useChatSwitchActions({
        activeConversationId: 'conv-1',
        applyFocusDocuments: vi.fn(),
        activeModel: null,
        setActiveChatModel,
        downloadedModels: models.current,
      })
    );

    await act(async () => {
      await result.current.handleSwitchActiveModel(
        'ornith-ai/Ornith-1.5-9B-GGUF',
        'ornith-ai/Ornith-1.5-9B-GGUF'
      );
    });

    expect(setActiveChatModel).toHaveBeenCalledWith('ornith-ai/Ornith-1.5-9B-GGUF');
    expect(updateSpy).toHaveBeenCalledWith({
      category: 'llm',
      updates: { provider: 'local' },
    });
    expect(s.llm.provider).toBe('local');

    rerender();
    await act(async () => {
      await result.current.handleSwitchActiveModel(
        '__llamacpp_server__',
        'llama.cpp · Qwen3.8-27B-Q5_K_S.gguf'
      );
    });

    expect(updateSpy).toHaveBeenLastCalledWith({
      category: 'llm',
      updates: { provider: 'llamacpp' },
    });
    expect(s.llm.provider).toBe('llamacpp');
    updateSpy.mockRestore();
  });
});
