import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { DownloadedVisionDto } from '@/lib/bindings';
import { makeAppSettings } from '@/tests/fixtures/appSettings';
import type { DownloadedModel } from '@/types/downloadedModels';
import type { DownloadStatus } from '@/types/downloads';

import { LocalVisionControl } from './LocalVisionControl';

const hooks = vi.hoisted(() => ({
  downloads: [] as DownloadStatus[],
  settled: true,
  modelsFetched: true,
  start: vi.fn(),
  resume: vi.fn(),
  refresh: vi.fn(),
  update: vi.fn(),
  settings: undefined as ReturnType<typeof makeAppSettings> | undefined,
}));

vi.mock('@/features/model/hooks/useDownloadState', () => ({ useDownloadState: () => ({ downloads: hooks.downloads, downloadsSettled: hooks.settled }) }));
vi.mock('@/features/model/hooks/useDownloads', () => ({
  useDownloadActions: () => ({ startVisionProjectorDownload: hooks.start, resumeDownload: hooks.resume }),
}));
vi.mock('./ModelRolesContext', () => ({ useModelRoles: () => ({ refresh: hooks.refresh, modelsFetchedAfterMount: hooks.modelsFetched }) }));
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
  projector_downloaded: false, enabled: true, projector_bytes: 921_000_000, projector_path: '/models/qwen3.5-9b/mmproj.gguf',
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
  hooks.settled = true;
  hooks.modelsFetched = true;
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

  it('holds the button until the download list has settled', () => {
    hooks.settled = false;
    render(<LocalVisionControl model={model} vision={vision()} />);
    expect(screen.getByRole('button', { name: 'Download vision projector (878 MB)' })).toBeDisabled();
  });

  it('holds the button until the model list has been re-read', () => {
    hooks.modelsFetched = false;
    render(<LocalVisionControl model={model} vision={vision()} />);
    expect(screen.getByRole('button', { name: 'Download vision projector (878 MB)' })).toBeDisabled();
  });

  it('says why the last attempt failed', () => {
    hooks.downloads = [projectorDownload({ state: 'Failed', error_message: 'Disk full.' })];
    render(<LocalVisionControl model={model} vision={vision()} />);
    expect(screen.getByText("Couldn't download: Disk full.")).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Download vision projector (878 MB)' })).toBeEnabled();
  });

  it('follows the projector download the download list reports, matched by its path', () => {
    hooks.downloads = [projectorDownload({ destination: '\\models\\qwen3.5-9b\\mmproj.gguf' })];
    render(<LocalVisionControl model={model} vision={vision()} />);

    expect((screen.getByRole('progressbar') as HTMLProgressElement).value).toBe(42);
    expect(screen.getByRole('status', { name: 'Downloading the vision projector' })).toHaveTextContent('Downloading… 42%');
    expect(screen.queryByRole('switch')).toBeNull();
  });

  it('resumes a paused projector download', async () => {
    hooks.downloads = [projectorDownload({ state: 'Paused' })];
    hooks.resume.mockResolvedValue(undefined);
    render(<LocalVisionControl model={model} vision={vision()} />);

    await userEvent.click(screen.getByRole('button', { name: 'Resume' }));
    expect(hooks.resume).toHaveBeenCalledWith('dl-1');
  });

  it('ignores a download for some other file', () => {
    hooks.downloads = [projectorDownload({ destination: '/models/other/mmproj.gguf' })];
    render(<LocalVisionControl model={model} vision={vision()} />);
    expect(screen.queryByRole('status')).toBeNull();
  });

  it('says Finishing… and keeps re-reading the model after the projector completes', () => {
    vi.useFakeTimers();
    try {
      hooks.downloads = [projectorDownload({ state: 'Completed', percentage: 100, completed_at: new Date().toISOString() })];
      render(<LocalVisionControl model={model} vision={vision()} />);
      expect(screen.getByRole('status')).toHaveTextContent('Finishing…');
      act(() => { vi.advanceTimersByTime(2000); });
      expect(hooks.refresh).toHaveBeenCalledTimes(2);
    } finally {
      vi.useRealTimers();
    }
  });

  it('offers the download again once a finished projector has not landed within the window', () => {
    hooks.downloads = [projectorDownload({ state: 'Completed', percentage: 100, completed_at: new Date(Date.now() - 120_000).toISOString() })];
    render(<LocalVisionControl model={model} vision={vision()} />);
    expect(screen.getByRole('button', { name: 'Download vision projector (878 MB)' })).toBeEnabled();
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
