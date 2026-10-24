import { beforeEach, describe, expect, it } from 'vitest';

import { COMMAND_PLUGINS } from '@/shared/ipc/routes.generated';
import { mockIPC } from '@/tests/setup';

import { downloadApi } from './client';

const lastCall = () => mockIPC.mock.calls.at(-1) as [string, Record<string, unknown>];
const status = (state: string) => ({ id: 'd1', state, bytes_downloaded: 1, total_bytes: 2 });

beforeEach(() => { mockIPC.mockReset(); mockIPC.mockResolvedValue(null); });

describe('downloadApi', () => {
  it('sends every method to a registered command through the plugin that owns it', async () => {
    for (const [name, call] of Object.entries(downloadApi) as [string, (...args: unknown[]) => Promise<unknown>][]) {
      mockIPC.mockClear();
      mockIPC.mockResolvedValueOnce(name === 'listDownloads' ? [] : null);
      await call('arg-1');
      const [route] = lastCall();
      const [, plugin, command] = /^plugin:([^|]+)\|(.+)$/.exec(route) ?? [];
      expect(COMMAND_PLUGINS[command as keyof typeof COMMAND_PLUGINS], `${name} → ${route}`).toBe(plugin);
    }
  });

  it('names the download being acted on', async () => {
    await downloadApi.pauseDownload('d7');
    expect(lastCall()[1]).toEqual({ id: 'd7' });
    await downloadApi.startModelDownload({ modelId: 'herb' } as never);
    expect(lastCall()[1]).toEqual({ request: { modelId: 'herb' } });
  });

  it('lists downloads with their states checked', async () => {
    mockIPC.mockResolvedValueOnce([status('Downloading'), status('Paused'), status('Completed'), status('Failed'), status('Cancelled'), status('Pending')]);
    const result = await downloadApi.listDownloads();
    expect(result.ok && result.data.map((d) => d.state)).toEqual(['Downloading', 'Paused', 'Completed', 'Failed', 'Cancelled', 'Pending']);
  });

  it('rejects a state the renderer does not know', async () => {
    mockIPC.mockResolvedValueOnce([status('Exploded')]);
    await expect(downloadApi.listDownloads()).rejects.toThrow('Unknown download state: Exploded');
  });

  it('reads one download, a missing one, or a failure', async () => {
    mockIPC.mockResolvedValueOnce(status('Completed'));
    await expect(downloadApi.getDownloadStatus('d1')).resolves.toEqual({ ok: true, data: status('Completed') });
    mockIPC.mockResolvedValueOnce(null);
    await expect(downloadApi.getDownloadStatus('d1')).resolves.toEqual({ ok: true, data: null });
    mockIPC.mockRejectedValueOnce(new Error('store closed'));
    await expect(downloadApi.getDownloadStatus('d1')).resolves.toMatchObject({ ok: false, error: 'store closed' });
    mockIPC.mockRejectedValueOnce(new Error('store closed'));
    await expect(downloadApi.listDownloads()).resolves.toMatchObject({ ok: false, error: 'store closed' });
  });
});
