import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ModelPickerPopover } from '@/features/chat/components/ModelPickerPopover';
import type { ChatModelChoice, LoadedModelDto } from '@/lib/bindings';
import { makeAppSettings } from '@/tests/fixtures/appSettings';
import type { DownloadedModel } from '@/types/downloadedModels';
import type { ChatModelOption } from '@/utils/chatModelSelection';

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

const loaded = vi.hoisted(() => ({ current: [] as LoadedModelDto[] }));
vi.mock('@/features/model/hooks/useLoadedModels', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/features/model/hooks/useLoadedModels')>()),
  useLoadedModels: () => loaded.current,
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

const open = async (choice: ChatModelChoice = { kind: 'default' }, onSelect = vi.fn()) => {
  render(
    <ModelPickerPopover choice={choice} onSelect={onSelect}>
      <button type="button">Try with…</button>
    </ModelPickerPopover>
  );
  await userEvent.click(screen.getByText('Try with…'));
};

describe('ModelPickerPopover', () => {
  beforeEach(() => {
    appSettings.current = undefined;
    models.current = [];
    loaded.current = [];
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

  it('groups by backend and, with no "Use default" row, checks the model that answers', async () => {
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
    expect(screen.getByRole('group', { name: 'Local' })).toBeInTheDocument();
    expect(screen.getByRole('menuitemradio', { name: /^Llama 3\.2 3B/ })).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByRole('menuitemradio', { name: /^mistral:latest/ })).toHaveAttribute('aria-checked', 'false');
    expect(screen.queryByRole('menuitemradio', { name: /^Use default/ })).toBeNull();
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
    await open({ kind: 'default' }, onSelect);

    expect(screen.getByText('Local')).toBeInTheDocument();
    expect(screen.getByText('llama.cpp')).toBeInTheDocument();
    expect(screen.queryByText('Ollama')).not.toBeInTheDocument();
    expect(screen.queryByText('Ollama Server')).not.toBeInTheDocument();

    expect(screen.getByText('ornith-ai/Ornith-1.5-9B-GGUF')).toBeInTheDocument();
    expect(screen.getByText('Qwen3.8-27B-Q5_K_S.gguf')).toBeInTheDocument();
    expect(screen.getByText('localhost:8080')).toBeInTheDocument();

    expect(screen.getByRole('menuitemradio', { name: /^Qwen3\.8-27B/ })).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByRole('menuitemradio', { name: /^ornith-ai/ })).toHaveAttribute('aria-checked', 'false');

    await userEvent.click(screen.getByText('Qwen3.8-27B-Q5_K_S.gguf'));
    expect(onSelect).toHaveBeenCalledWith('__llamacpp_server__', 'llama.cpp · Qwen3.8-27B-Q5_K_S.gguf');
  });

  it('puts "Use default" first and checks it for a chat on the default', async () => {
    models.current = [model({ id: 'a', is_active_for_chat: true }), model({ id: 'b', model_id: 'ornith', model_name: 'Ornith-1.5-9B' })];
    const onUseDefault = vi.fn();
    const defaultOption = { label: 'Llama 3.2 3B' } as ChatModelOption;
    render(
      <ModelPickerPopover choice={{ kind: 'default' }} defaultOption={defaultOption} onUseDefault={onUseDefault} onSelect={vi.fn()}>
        <button type="button">Model</button>
      </ModelPickerPopover>
    );
    await userEvent.click(screen.getByText('Model'));

    const rows = screen.getAllByRole('menuitemradio');
    expect(rows[0]).toHaveAccessibleName(/Use default \(Llama 3\.2 3B\)/);
    expect(rows[0]).toHaveAttribute('aria-checked', 'true');
    // The default model's own row is not checked: picking it pins this chat to it.
    expect(screen.getByRole('menuitemradio', { name: /^Llama 3\.2 3B/ })).toHaveAttribute('aria-checked', 'false');
    expect(screen.getByRole('menu', { name: 'Model for this chat' })).toBeInTheDocument();
    // The menu opens on the checked row.
    expect(rows[0]).toHaveFocus();

    await userEvent.click(rows[0]);
    expect(onUseDefault).toHaveBeenCalledTimes(1);
  });

  it("checks the chat's own model and not the default row", async () => {
    models.current = [model({ id: 'a', is_active_for_chat: true }), model({ id: 'b', model_id: 'ornith', model_name: 'Ornith-1.5-9B' })];
    render(
      <ModelPickerPopover
        choice={{ kind: 'explicit', model: { kind: 'local', model_id: 'ornith' } }}
        defaultOption={null}
        onUseDefault={vi.fn()}
        onSelect={vi.fn()}
      >
        <button type="button">Model</button>
      </ModelPickerPopover>
    );
    await userEvent.click(screen.getByText('Model'));

    expect(screen.getByRole('menuitemradio', { name: /^Use default/ })).toHaveAttribute('aria-checked', 'false');
    expect(screen.getByText('No chat model is selected in Settings → Models.')).toBeInTheDocument();
    expect(screen.getByRole('menuitemradio', { name: /^Ornith-1\.5-9B/ })).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByRole('menuitemradio', { name: /^Llama 3\.2 3B/ })).toHaveAttribute('aria-checked', 'false');
  });

  it('moves between rows with the arrow keys, Home and End', async () => {
    models.current = [model({ id: 'a' }), model({ id: 'b', model_id: 'b', model_name: 'Second' }), model({ id: 'c', model_id: 'c', model_name: 'Third' })];
    await open();
    const rows = screen.getAllByRole('menuitemradio');
    expect(rows[0]).toHaveFocus();
    await userEvent.keyboard('{ArrowDown}');
    expect(rows[1]).toHaveFocus();
    await userEvent.keyboard('{End}');
    expect(rows[2]).toHaveFocus();
    await userEvent.keyboard('{ArrowDown}');
    expect(rows[0]).toHaveFocus();
    await userEvent.keyboard('{ArrowUp}');
    expect(rows[2]).toHaveFocus();
    await userEvent.keyboard('{Home}');
    expect(rows[0]).toHaveFocus();
  });

  it('shows the window a model runs with and badges a model that cannot call tools', async () => {
    models.current = [
      model({ id: 'a', configured_context_length: 8704, supports_tool_calling: false }),
      model({ id: 'b', model_id: 'big', model_name: 'Qwen3.5-27B', trained_context_length: 32768, supports_tool_calling: true }),
    ];
    await open();

    const small = screen.getByRole('menuitemradio', { name: /^Llama 3\.2 3B/ });
    expect(small).toHaveTextContent('8,704 ctx · set by you');
    expect(small).toHaveTextContent('no tools');
    const big = screen.getByRole('menuitemradio', { name: /^Qwen3\.5-27B/ });
    expect(big).toHaveTextContent('32K ctx');
    expect(big).not.toHaveTextContent('no tools');
  });

  it('says "sees images" beside a model declared to see, and a cloud model its provider documents as seeing', async () => {
    const s = makeAppSettings();
    s.llm.provider = 'anthropic';
    s.llm.model = 'claude-test';
    s.llm.llamaCpp = { url: 'http://127.0.0.1:8080', model: 'served-model', authHeaderName: '', authHeaderValue: '' };
    s.llm.visionDeclarations = { 'llamacpp:served-model': { enabled: true, geometry: { kind: 'fixed', side: 896, tokens: 256 } } };
    appSettings.current = s;
    models.current = [model({ id: 'a' })];
    await open();

    const served = within(screen.getByRole('menuitemradio', { name: /^served-model/ })).getByText('sees images');
    expect(served).toHaveAttribute('title', 'Pictures you attach are shown to this model.');
    expect(screen.getByRole('menuitemradio', { name: /^claude-test/ })).toHaveTextContent('sees images');
    expect(screen.getByRole('menuitemradio', { name: /^Llama 3\.2 3B/ })).not.toHaveTextContent('sees images');
  });

  it('drops the badge when a declaration turns vision off, even over a provider default', async () => {
    const s = makeAppSettings();
    s.llm.provider = 'openai';
    s.llm.model = 'gpt-4o-test';
    s.llm.visionDeclarations = { 'cloud:openai/gpt-4o-test': { enabled: false, geometry: { kind: 'fixed', side: 896, tokens: 256 } } };
    appSettings.current = s;
    await open();

    expect(screen.getByRole('menuitemradio', { name: /^gpt-4o-test/ })).not.toHaveTextContent('sees images');
  });

  it('says in words, with the reason as its tooltip, that a loaded model runs on the CPU', async () => {
    models.current = [model({ id: 'a' }), model({ id: 'b', model_id: 'big', model_name: 'Qwen3.5-27B' })];
    loaded.current = [{
      model: { kind: 'local', model_id: 'llama-3.2-3b' }, provider: 'local', context_tokens: 8192, in_flight: 0,
      pinned_by: [], loaded_at: '', last_used_at: '', gpu_bytes: null, degraded: 'Metal buffer limit',
    }];
    await open();

    const badge = within(screen.getByRole('menuitemradio', { name: /^Llama 3\.2 3B/ })).getByText('on CPU');
    expect(badge).toHaveAttribute('title', 'Llama 3.2 3B is running on the CPU: Metal buffer limit. Answers will be slow.');
    expect(within(screen.getByRole('menuitemradio', { name: /^Qwen3\.5-27B/ })).queryByText('on CPU')).toBeNull();
  });
});
