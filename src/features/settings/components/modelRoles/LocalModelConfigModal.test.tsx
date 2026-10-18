import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import { makeAppSettings } from '@/tests/fixtures/appSettings';
import type { DownloadedModel } from '@/types/downloadedModels';

import { LocalModelConfigModal } from './LocalModelConfigModal';

const mockUpdateSettings = vi.fn().mockResolvedValue({ ok: true });
let mockSettings = makeAppSettings();

vi.mock('@/features/settings/hooks/useSettingsQuery', () => ({
  useSettingsQuery: () => ({ data: mockSettings }),
  useUpdateSettingsMutation: () => ({
    mutateAsync: (...args: unknown[]) => mockUpdateSettings(...args),
    isPending: false,
  }),
}));

function sampleModel(overrides: Partial<DownloadedModel> = {}): DownloadedModel {
  return {
    id: 'm1',
    model_name: 'Qwen2.5-Coder-7B-Instruct-Q4_K_M.gguf',
    model_id: 'qwen2.5-coder-7b',
    file_path: '/models/Qwen2.5-Coder-7B-Instruct-Q4_K_M.gguf',
    file_size_bytes: 4_680_000_000,
    model_type: 'chat',
    downloaded_at: '2026-09-01T00:00:00Z',
    last_used_at: null,
    use_count: 0,
    is_active_for_chat: true,
    is_active_for_embedding: false,
    is_active_for_utility: false,
    backend: 'local',
    trained_context_length: 32768,
    ...overrides,
  };
}

describe('LocalModelConfigModal', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    mockSettings = makeAppSettings();
    mockUpdateSettings.mockReset().mockResolvedValue({ ok: true });
  });

  it('renders model info and supported context badge', () => {
    render(
      <TooltipProvider>
        <LocalModelConfigModal
          isOpen
          onClose={vi.fn()}
          model={sampleModel()}
        />
      </TooltipProvider>,
    );

    expect(
      screen.getByRole('heading', { name: 'Qwen2.5-Coder-7B-Instruct-Q4_K_M.gguf' }),
    ).toBeInTheDocument();
    expect(screen.getByText('Model supports up to')).toBeInTheDocument();
    expect(screen.getByText((32768).toLocaleString())).toBeInTheDocument();
    expect(screen.getByText('Q4_K_M')).toBeInTheDocument();
  });

  it('renders interactive tooltip triggers with accessible descriptions for all 4 settings', () => {
    render(
      <TooltipProvider>
        <LocalModelConfigModal
          isOpen
          onClose={vi.fn()}
          model={sampleModel()}
        />
      </TooltipProvider>,
    );

    const helpButtons = screen.getAllByRole('button', { name: /^More information:/ });
    expect(helpButtons).toHaveLength(4);

    expect(helpButtons[0]).toHaveAttribute(
      'title',
      expect.stringContaining('Tokens the model keeps in its active context window'),
    );
    expect(helpButtons[1]).toHaveAttribute(
      'title',
      expect.stringContaining('Parallel slots (-np)'),
    );
    expect(helpButtons[2]).toHaveAttribute(
      'title',
      expect.stringContaining('Fix the random number generator seed'),
    );
    expect(helpButtons[3]).toHaveAttribute(
      'title',
      expect.stringContaining('Quantize the K and V caches to q8_0'),
    );
  });

  it('toggles Auto context length', async () => {
    const user = userEvent.setup();
    render(
      <TooltipProvider>
        <LocalModelConfigModal
          isOpen
          onClose={vi.fn()}
          model={sampleModel()}
        />
      </TooltipProvider>,
    );

    const autoBtn = screen.getByRole('button', { name: 'Auto' });
    expect(screen.getByText(/Auto is active/i)).toBeInTheDocument();

    // Toggle Auto OFF
    await user.click(autoBtn);
    expect(screen.queryByText(/Auto is active/i)).not.toBeInTheDocument();

    // Toggle Auto back ON
    await user.click(autoBtn);
    expect(screen.getByText(/Auto is active/i)).toBeInTheDocument();
  });

  it('saves settings with localModelConfigs payload when Save is clicked', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();

    render(
      <TooltipProvider>
        <LocalModelConfigModal
          isOpen
          onClose={onClose}
          model={sampleModel()}
        />
      </TooltipProvider>,
    );

    // Turn off Auto to set a fixed context length
    const autoBtn = screen.getByRole('button', { name: 'Auto' });
    await user.click(autoBtn);

    // Toggle KV cache quantization switch
    const kvSwitch = screen.getByRole('switch', { name: 'KV Cache Quantization' });
    await user.click(kvSwitch);

    // Click Save
    const saveButton = screen.getByRole('button', { name: 'Save' });
    await user.click(saveButton);

    expect(mockUpdateSettings).toHaveBeenCalledWith({
      category: 'llm',
      updates: expect.objectContaining({
        localModelConfigs: expect.objectContaining({
          'qwen2.5-coder-7b': expect.objectContaining({
            contextLength: 8192,
            kvCacheQuantization: true,
            seed: null,
          }),
        }),
      }),
    });
    expect(onClose).toHaveBeenCalled();
  });

  it('triggers onLoadModel when Load Model is clicked', async () => {
    const user = userEvent.setup();
    const onLoadModel = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();

    render(
      <TooltipProvider>
        <LocalModelConfigModal
          isOpen
          onClose={onClose}
          model={sampleModel()}
          onLoadModel={onLoadModel}
        />
      </TooltipProvider>,
    );

    const loadButton = screen.getByRole('button', { name: 'Load Model' });
    await user.click(loadButton);

    expect(mockUpdateSettings).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
    expect(onLoadModel).toHaveBeenCalled();
  });

  it('keeps the vision switch, which the model row owns, when it saves', async () => {
    const user = userEvent.setup();
    mockSettings.llm.localModelConfigs = { 'qwen2.5-coder-7b': { contextLength: null, vision: false } };

    render(
      <TooltipProvider>
        <LocalModelConfigModal isOpen onClose={vi.fn()} model={sampleModel()} />
      </TooltipProvider>,
    );
    await user.click(screen.getByRole('button', { name: 'Save' }));

    expect(mockUpdateSettings).toHaveBeenCalledWith({
      category: 'llm',
      updates: expect.objectContaining({
        localModelConfigs: { 'qwen2.5-coder-7b': expect.objectContaining({ vision: false }) },
      }),
    });
    mockSettings = makeAppSettings();
  });
});
