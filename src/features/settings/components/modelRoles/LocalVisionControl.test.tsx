import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { DownloadedVisionDto } from '@/lib/bindings';
import { makeAppSettings } from '@/tests/fixtures/appSettings';
import type { DownloadedModel } from '@/types/downloadedModels';
import type { DownloadStatus } from '@/types/downloads';

import { LocalVisionControl } from './LocalVisionControl';

const hooks = vi.hoisted(() => ({
  downloads: [] as DownloadStatus[],
  start: vi.fn(),
  refresh: vi.fn(),
  update: vi.fn(),
  settings: undefined as ReturnType<typeof makeAppSettings> | undefined,
}));

vi.mock('@/features/model/hooks/useDownloadState', () => ({ useDownloadState: () => ({ downloads: hooks.downloads }) }));
vi.mock('@/features/model/hooks/useDownloads', () => ({
  useDownloadActions: () => ({ startVisionProjectorDownload: hooks.start }),
}));
vi.mock('./ModelRolesContext', () => ({ useModelRoles: () => ({ refresh: hooks.refresh }) }));
vi.mock('@/features/settings/hooks/useSettingsQuery', () => ({
  useSettingsQuery: () => ({ data: hooks.settings }),
  useUpdateSettingsMutation: () => ({ mutateAsync: hooks.update, isPending: false }),
}));

const model: DownloadedModel = {
  id: 'row-9b', model_name: 'Qwen 3.5 9B', model_id: 'qwen3.5-9b', file_path: '/models/qwen3.5-9b/qwen3.5-9b-q4.gguf',
  file_size_bytes: 5_600_000_000, model_type: 'chat', downloaded_at: '2026-10-01T00:00:00Z', last_used_at: null,
  use_count: 0, is_active_for_chat: true, is_active_for_embedding: false, is_active_for_utility: false, backend: 'local',
};
const vision = (overrides: Partial<DownloadedVisionDto> = {}): DownloadedVisionDto => ({
  projector_downloaded: false, enabled: true, projector_bytes: 921_000_000,
  geometry: { kind: 'grid', token_side: 32, min_pixels: 65536, max_pixels: 2097152, overhead_tokens: 2 },
  ...overrides,
});
const projectorDownload = (overrides: Partial<DownloadStatus>): DownloadStatus => ({
  id: 'dl-1', url: 'https://example.invalid/mmproj.gguf', destination: '/models/qwen3.5-9b/mmproj.gguf', state: 'Downloading',
  bytes_downloaded: 386_820_000, total_bytes: 921_000_000, bytes_per_second: 0, percentage: 42, eta_seconds: null,
  error_message: null, retry_count: 0, created_at: '', started_at: null, completed_at: null, model_name: null, model_id: null,
  ...overrides,
});

beforeEach(() => {
  vi.clearAllMocks();
  hooks.downloads = [];
  hooks.settings = makeAppSettings();
});

describe('LocalVisionControl', () => {
  it('offers the projector, with its size, before it is downloaded', async () => {
    hooks.start.mockResolvedValue('dl-1');
    render(<LocalVisionControl model={model} vision={vision()} />);

    await userEvent.click(screen.getByRole('button', { name: 'Download vision projector (878 MB)' }));

    expect(hooks.start).toHaveBeenCalledWith('row-9b');
    expect(screen.queryByRole('switch')).toBeNull();
  });

  it('follows the projector download the download list reports', () => {
    hooks.downloads = [projectorDownload({})];
    render(<LocalVisionControl model={model} vision={vision()} />);

    expect(screen.getByRole('progressbar', { name: 'Vision projector download' })).toHaveAttribute('aria-valuenow', '42');
    expect(screen.getByRole('status')).toHaveTextContent('42% of 878 MB');
    expect(screen.queryByRole('switch')).toBeNull();
  });

  it('re-reads the model once the projector is on disk', () => {
    hooks.downloads = [projectorDownload({ state: 'Completed', percentage: 100 })];
    hooks.start.mockResolvedValue(null);
    render(<LocalVisionControl model={model} vision={vision()} />);

    expect(hooks.refresh).toHaveBeenCalled();
  });

  it('lets a downloaded projector be switched off, keeping the rest of the model settings', async () => {
    hooks.settings!.llm.localModelConfigs = { 'qwen3.5-9b': { contextLength: 16384 } };
    hooks.update.mockResolvedValue({ ok: true });
    render(<LocalVisionControl model={model} vision={vision({ projector_downloaded: true })} />);

    const toggle = screen.getByRole('switch', { name: 'Let this model see images' });
    expect(toggle).toBeChecked();
    await userEvent.click(toggle);

    expect(hooks.update).toHaveBeenCalledWith({
      category: 'llm',
      updates: { localModelConfigs: { 'qwen3.5-9b': { contextLength: 16384, vision: false } } },
    });
  });
});
