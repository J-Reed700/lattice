import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { LocalModelRow } from './LocalModelRow';
import { VaultAPI } from '../../../lib/api';
import { useModelWarmupStore } from '../../../stores/modelWarmupStore';
import { TooltipProvider } from '../../ui/tooltip';

import type { DownloadedModel } from '../../../types/downloadedModels';

vi.mock('./ModelRolesContext', () => ({
  useModelRoles: () => ({ refresh: vi.fn(), assignRole: vi.fn() }),
}));

vi.mock('../../../hooks/useDownloadedModels', () => ({
  useDownloadedModels: () => ({ deleteDownloadedModel: vi.fn() }),
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

function renderRow() {
  return render(
    <TooltipProvider>
      <LocalModelRow model={utilityModel()} />
    </TooltipProvider>,
  );
}

describe('LocalModelRow load status', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useModelWarmupStore.getState().reset();
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
    vi.spyOn(VaultAPI, 'warmUpActiveUtilityModel').mockResolvedValue({
      ok: false,
      error: UNUSABLE,
    });

    renderRow();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Warm up' }));

    expect(await screen.findByRole('alert')).toHaveTextContent("Didn't load:");
    expect(useModelWarmupStore.getState().utility).toMatchObject({
      phase: 'failed',
      error: UNUSABLE,
    });
  });
});
