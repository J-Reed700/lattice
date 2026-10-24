import { beforeEach, describe, expect, it } from 'vitest';

import { COMMAND_PLUGINS } from '@/shared/ipc/routes.generated';
import { mockIPC } from '@/tests/setup';

import { backupApi } from './client';

const lastCall = () => mockIPC.mock.calls.at(-1) as [string, Record<string, unknown>];

beforeEach(() => { mockIPC.mockReset(); mockIPC.mockResolvedValue({}); });

describe('backupApi', () => {
  it('sends every method to a registered command through the plugin that owns it', async () => {
    for (const [name, call] of Object.entries(backupApi) as [string, (...args: unknown[]) => Promise<unknown>][]) {
      mockIPC.mockClear();
      await call('arg-1', 'arg-2');
      const [route] = lastCall();
      const [, plugin, command] = /^plugin:([^|]+)\|(.+)$/.exec(route) ?? [];
      expect(COMMAND_PLUGINS[command as keyof typeof COMMAND_PLUGINS], `${name} → ${route}`).toBe(plugin);
    }
  });

  it('never sends a destination for backups and exports; the backend picks the folder', async () => {
    await backupApi.createBackup();
    expect(lastCall()[1]).toEqual({ request: { backupPath: null } });
    await backupApi.exportMarkdown();
    expect(lastCall()[1]).toEqual({ request: { outputDir: null } });
    await backupApi.exportJson();
    expect(lastCall()[1]).toEqual({ request: { outputPath: null, pretty: true } });
    await backupApi.restoreBackup('/backups/2026-10-01.db');
    expect(lastCall()[1]).toEqual({ request: { backupPath: '/backups/2026-10-01.db' } });
  });

  it('lists backups, treating a missing list as none', async () => {
    mockIPC.mockResolvedValueOnce({ backups: [{ path: '/backups/a.db' }] });
    await expect(backupApi.listBackups()).resolves.toEqual({ ok: true, data: [{ path: '/backups/a.db' }] });
    mockIPC.mockResolvedValueOnce({});
    await expect(backupApi.listBackups()).resolves.toEqual({ ok: true, data: [] });
    mockIPC.mockRejectedValueOnce(new Error('backups folder unreadable'));
    await expect(backupApi.listBackups()).resolves.toMatchObject({ ok: false, error: 'backups folder unreadable' });
  });
});
