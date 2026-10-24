import { beforeEach, describe, expect, it } from 'vitest';

import { COMMAND_PLUGINS } from '@/shared/ipc/routes.generated';
import { mockIPC } from '@/tests/setup';

import { modelApi } from './client';

const lastCall = () => mockIPC.mock.calls.at(-1) as [string, Record<string, unknown>];

beforeEach(() => { mockIPC.mockReset(); mockIPC.mockResolvedValue(null); });

describe('modelApi', () => {
  it('sends every method to a registered command through the plugin that owns it', async () => {
    for (const [name, call] of Object.entries(modelApi) as [string, (...args: unknown[]) => Promise<unknown>][]) {
      mockIPC.mockClear();
      await call('arg-1', 'arg-2');
      const [route] = lastCall();
      const [, plugin, command] = /^plugin:([^|]+)\|(.+)$/.exec(route) ?? [];
      expect(COMMAND_PLUGINS[command as keyof typeof COMMAND_PLUGINS], `${name} → ${route}`).toBe(plugin);
    }
  });

  it('names the model in model commands', async () => {
    await modelApi.setActiveChatModel('herb-9b');
    expect(lastCall()[1]).toEqual({ modelId: 'herb-9b' });
    await modelApi.getModelVariants('org/herb');
    expect(lastCall()[1]).toEqual({ repoId: 'org/herb' });
    await modelApi.searchModelCatalog({ query: 'herb', limit: 5 } as never);
    expect(lastCall()[1]).toEqual({ request: { query: 'herb', limit: 5 } });
  });

  it('keeps the file unless asked when deleting a downloaded model, and deletes it by default via deleteModel', async () => {
    await modelApi.deleteDownloadedModel('herb-9b');
    expect(lastCall()[1]).toEqual({ modelId: 'herb-9b', deleteFile: false });
    await modelApi.deleteModel('herb-9b');
    expect(lastCall()[1]).toEqual({ modelId: 'herb-9b', deleteFile: true });
    await modelApi.deleteModel('herb-9b', false);
    expect(lastCall()[1]).toEqual({ modelId: 'herb-9b', deleteFile: false });
  });

  it('sends a category only when one is given', async () => {
    await modelApi.getCompatibleModels('embedding');
    expect(lastCall()[1]).toEqual({ category: 'embedding' });
    await modelApi.getCompatibleModels();
    expect(lastCall()[1]).toEqual({});
  });

  it('wraps replies and failures as results', async () => {
    mockIPC.mockResolvedValueOnce(true);
    await expect(modelApi.isModelDownloaded('herb-9b')).resolves.toEqual({ ok: true, data: true });
    mockIPC.mockRejectedValueOnce(new Error('not found'));
    await expect(modelApi.getActiveChatModel()).resolves.toMatchObject({ ok: false, error: 'not found' });
  });
});
