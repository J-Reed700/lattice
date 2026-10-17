import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import { useModelWarmupStore } from '@/features/model/stores/modelWarmupStore';
import type { LearningWindowShortfallDto, LoadedModelDto } from '@/lib/bindings';
import { makeAppSettings } from '@/tests/fixtures/appSettings';
import type { LLMSettings } from '@/types/api/settings';
import type { DownloadedModel } from '@/types/downloadedModels';

import { LocalModelRow } from './LocalModelRow';

const roles = vi.hoisted(() => ({ llmSettings: undefined as LLMSettings | undefined }));
const hooks = vi.hoisted(() => ({
  warmUpActiveChatModel: vi.fn(),
  warmUpActiveUtilityModel: vi.fn(),
  warmUpLearningModel: vi.fn(),
  deleteDownloadedModel: vi.fn(),
  loaded: [] as LoadedModelDto[],
  shortfall: null as LearningWindowShortfallDto | null,
}));

vi.mock('./ModelRolesContext', () => ({
  useModelRoles: () => ({ refresh: vi.fn(), assignRole: vi.fn(), chatProvider: roles.llmSettings?.provider, llmSettings: roles.llmSettings }),
}));

vi.mock('@/features/model/hooks/useDownloadedModels', () => ({
  useDownloadedModels: () => hooks,
}));

vi.mock('@/features/model/hooks/useLearningWindowShortfall', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/features/model/hooks/useLearningWindowShortfall')>()),
  useLearningWindowShortfall: () => hooks.shortfall,
}));

vi.mock('@/features/model/hooks/useLoadedModels', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/features/model/hooks/useLoadedModels')>()),
  useLoadedModels: () => hooks.loaded,
}));

const UNUSABLE =
  "Lattice's bundled llama-server can't run on this machine: " +
  'dyld[1006]: Library not loaded: @rpath/libllama-common.0.dylib. ' +
  'Reinstall Lattice.' +
  '\n\nllama-server output (last 1 lines):\n  dyld[1006]: Library not loaded';

function utilityModel(): DownloadedModel {
  return {
    id: 'm1',
    model_name: 'Qwen3 4B',
    model_id: 'qwen3-4b',
    file_path: '/models/qwen3-4b.gguf',
    file_size_bytes: 2_500_000_000,
    model_type: 'chat',
    downloaded_at: '2026-09-01T00:00:00Z',
    last_used_at: null,
    use_count: 0,
    is_active_for_chat: false,
    is_active_for_embedding: false,
    is_active_for_utility: true,
    backend: 'local',
  };
}

function renderRow(model = utilityModel()) {
  return render(
    <TooltipProvider>
      <LocalModelRow model={model} />
    </TooltipProvider>,
  );
}

describe('LocalModelRow load status', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useModelWarmupStore.getState().reset();
    roles.llmSettings = undefined;
    hooks.loaded = [];
    hooks.shortfall = null;
  });

  it("shows the backend's reason when the model failed to load for a role it holds", () => {
    useModelWarmupStore.getState().setRolePhase('utility', 'failed', UNUSABLE);

    renderRow();

    const alert = screen.getByRole('alert');
    expect(alert).toHaveTextContent(
      "Didn't load: Lattice's bundled llama-server can't run on this machine",
    );
    expect(alert).toHaveTextContent('Reinstall Lattice');
    expect(alert).not.toHaveTextContent('fetch-llama-binaries');
    expect(alert).not.toHaveTextContent('llama-server output');
    expect(alert).toHaveAttribute('title', UNUSABLE);
  });

  it('ignores failures of roles the model does not hold', () => {
    useModelWarmupStore.getState().setRolePhase('chat', 'failed', UNUSABLE);

    renderRow();

    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it("ignores a failure that belongs to the model previously assigned to the role", () => {
    // Model 'other' failed for utility, then utility was reassigned to m1.
    // m1 was never loaded, so its row must not carry the old model's error.
    useModelWarmupStore.getState().setRolePhase('utility', 'failed', UNUSABLE, 'other');

    renderRow();

    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(useModelWarmupStore.getState().utility).toMatchObject({
      phase: 'idle',
      modelId: 'm1',
    });
  });

  it('records a manual warm-up failure so the row shows it', async () => {
    const user = userEvent.setup();
    hooks.warmUpActiveUtilityModel.mockRejectedValue(new Error(UNUSABLE));

    renderRow();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Warm up' }));

    expect(await screen.findByRole('alert')).toHaveTextContent("Didn't load:");
    expect(useModelWarmupStore.getState().utility).toMatchObject({
      phase: 'failed',
      error: UNUSABLE,
    });
  });

  it('warms the Learning model when the row holds that role, and changes no setting on failure', async () => {
    const user = userEvent.setup();
    const settings = makeAppSettings().llm;
    settings.learningModel = { kind: 'local', model_id: 'qwen3-4b' };
    roles.llmSettings = settings;
    hooks.warmUpActiveUtilityModel.mockResolvedValue(undefined);
    hooks.warmUpLearningModel.mockRejectedValue(new Error('Not enough memory'));

    renderRow();
    expect(screen.getByRole('button', { name: 'Learning' })).toHaveAttribute('aria-pressed', 'true');
    await user.click(screen.getByRole('button', { name: 'Warm up' }));

    expect(hooks.warmUpLearningModel).toHaveBeenCalledTimes(1);
    expect(await screen.findByRole('alert')).toHaveTextContent("Didn't load: Not enough memory");
    expect(useModelWarmupStore.getState().learning).toMatchObject({ phase: 'failed', modelId: 'm1' });
    expect(settings.learningModel).toEqual({ kind: 'local', model_id: 'qwen3-4b' });
  });

  it('says a loaded model runs on the CPU, and why, in words', async () => {
    const user = userEvent.setup();
    hooks.warmUpActiveUtilityModel.mockResolvedValue(undefined);
    hooks.loaded = [{
      model: { kind: 'local', model_id: 'qwen3-4b' }, provider: 'local', context_tokens: 8192, in_flight: 0,
      pinned_by: [], loaded_at: '', last_used_at: '', gpu_bytes: null, degraded: 'Metal buffer limit',
    }];

    renderRow();
    expect(screen.getByRole('status')).toHaveTextContent('Loaded (on CPU: Metal buffer limit)');
    await user.click(screen.getByRole('button', { name: 'Warm up' }));
    expect(await screen.findByText('Ready (on CPU: Metal buffer limit)')).toBeInTheDocument();
  });

  it('warns before deleting the Learning model that lessons will stop', async () => {
    const user = userEvent.setup();
    const settings = makeAppSettings().llm;
    settings.learningModel = { kind: 'local', model_id: 'qwen3-4b' };
    roles.llmSettings = settings;

    renderRow();
    await user.click(screen.getByRole('button', { name: 'Delete' }));
    expect(await screen.findByText(
      'Remove Qwen3 4B and its files? This cannot be undone. This is your Learning model. Lessons will stop until you choose another.'
    )).toBeInTheDocument();
  });

  it('says under the Learning model what the last refused learning work needed, until its window covers it', () => {
    const settings = makeAppSettings().llm;
    settings.learningModel = { kind: 'local', model_id: 'qwen3-4b' };
    roles.llmSettings = settings;
    hooks.shortfall = {
      model: { kind: 'local', model_id: 'qwen3-4b' }, needed_tokens: 20_000, window_tokens: 8_704, window_is_saved: true,
    };

    const { rerender } = renderRow({ ...utilityModel(), configured_context_length: 8_704 });
    expect(screen.getByRole('status')).toHaveTextContent(
      'Learning work needs a context window of 20,000 tokens; this model has 8,704.'
    );

    rerender(
      <TooltipProvider>
        <LocalModelRow model={{ ...utilityModel(), configured_context_length: 32_768 }} />
      </TooltipProvider>,
    );
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
  });
});
