import { beforeEach, describe, expect, it } from 'vitest';

import { COMMAND_PLUGINS } from '@/shared/ipc/routes.generated';
import { mockIPC } from '@/tests/setup';

import { explorerApi } from './client';

const lastCall = () => mockIPC.mock.calls.at(-1) as [string, Record<string, unknown>];

beforeEach(() => { mockIPC.mockReset(); mockIPC.mockResolvedValue(null); });

describe('explorerApi', () => {
  it('sends every method to a registered command through the plugin that owns it', async () => {
    for (const [name, call] of Object.entries(explorerApi) as [string, (...args: unknown[]) => Promise<unknown>][]) {
      mockIPC.mockClear();
      await call('arg-1', 'arg-2', 'arg-3');
      const [route] = lastCall();
      const [, plugin, command] = /^plugin:([^|]+)\|(.+)$/.exec(route) ?? [];
      expect(COMMAND_PLUGINS[command as keyof typeof COMMAND_PLUGINS], `${name} → ${route}`).toBe(plugin);
    }
  });

  it('searches literally across the whole folder unless told otherwise', async () => {
    await explorerApi.explorerSearch('/notes', 'sourdough');
    expect(lastCall()[1]).toEqual({ root: '/notes', query: 'sourdough', regex: false, pathPrefix: null, maxResults: null });
    await explorerApi.explorerSearch('/notes', 'rye|spelt', { regex: true, pathPrefix: 'breads/', maxResults: 25 });
    expect(lastCall()[1]).toEqual({ root: '/notes', query: 'rye|spelt', regex: true, pathPrefix: 'breads/', maxResults: 25 });
  });

  it('returns a failed command as an error result', async () => {
    mockIPC.mockRejectedValueOnce(new Error('folder is gone'));
    await expect(explorerApi.explorerFoldersList()).resolves.toMatchObject({ ok: false, error: 'folder is gone' });
  });
});
