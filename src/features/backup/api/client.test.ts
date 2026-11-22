import { beforeEach, describe, expect, it } from 'vitest';

import { COMMAND_PLUGINS } from '@/shared/ipc/routes.generated';
import { mockIPC } from '@/tests/setup';

import { backupApi } from './client';
import { useRestoreLockoutStore } from '../restoreLockoutStore';

const lastCall = () => mockIPC.mock.calls.at(-1) as [string, Record<string, unknown>];

beforeEach(() => {
  mockIPC.mockReset();
  mockIPC.mockResolvedValue({});
  useRestoreLockoutStore.getState().resetRestoreLockout();
});

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

  it('sets restore lockout state only when restoreBackup or restoreArchive restores the database', async () => {
    mockIPC.mockRejectedValueOnce(new Error('corrupt backup'));
    await backupApi.restoreBackup('/backups/bad.db');
    expect(useRestoreLockoutStore.getState().isDatabaseRestoredRestartRequired).toBe(false);

    mockIPC.mockResolvedValueOnce({ success: true, restoredCount: 10, message: null });
    await backupApi.restoreBackup('/backups/good.db');
    expect(useRestoreLockoutStore.getState().isDatabaseRestoredRestartRequired).toBe(true);
    expect(useRestoreLockoutStore.getState().restoredFromArchive).toBe(false);

    useRestoreLockoutStore.getState().resetRestoreLockout();
    mockIPC.mockResolvedValueOnce({ outcome: 'needs_secret', restartRequired: false });
    await backupApi.restoreArchive(null);
    expect(useRestoreLockoutStore.getState().isDatabaseRestoredRestartRequired).toBe(false);

    mockIPC.mockResolvedValueOnce({ outcome: 'cancelled', restartRequired: false });
    await backupApi.restoreArchive(null);
    expect(useRestoreLockoutStore.getState().isDatabaseRestoredRestartRequired).toBe(false);

    mockIPC.mockResolvedValueOnce({ outcome: 'restored', restartRequired: true });
    await backupApi.restoreArchive('secret');
    expect(useRestoreLockoutStore.getState().isDatabaseRestoredRestartRequired).toBe(true);
    expect(useRestoreLockoutStore.getState().restoredFromArchive).toBe(true);
  });
});
