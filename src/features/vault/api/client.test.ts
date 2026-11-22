import { beforeEach, expect, it } from 'vitest';

import { COMMAND_PLUGINS } from '@/shared/ipc/routes.generated';
import { mockIPC } from '@/tests/setup';

import { vaultApi } from './client';

beforeEach(() => { mockIPC.mockReset(); });

it('asks the vault plugin for a rescan and returns its counts', async () => {
  mockIPC.mockResolvedValue({ scanned: 12, imported: 1, deleted: 0 });
  await expect(vaultApi.rescanVault()).resolves.toEqual({ ok: true, data: { scanned: 12, imported: 1, deleted: 0 } });
  expect(mockIPC).toHaveBeenCalledWith(`plugin:${COMMAND_PLUGINS.rescan_vault}|rescan_vault`, {});
});

it('lists and dismisses vault conflicts via the vault plugin', async () => {
  const conflicts = [
    { kind: 'keptCopy', id: 'c1', noteId: 'n1', noteTitle: 'T', copyId: 'c2', copyTitle: 'T (conflicted copy 2026-11-22)' },
  ];
  mockIPC.mockResolvedValueOnce(conflicts);
  await expect(vaultApi.listVaultConflicts()).resolves.toEqual({ ok: true, data: conflicts });
  expect(mockIPC).toHaveBeenCalledWith(
    `plugin:${COMMAND_PLUGINS.list_vault_conflicts}|list_vault_conflicts`,
    {},
  );

  mockIPC.mockResolvedValueOnce(null);
  await expect(vaultApi.dismissVaultConflicts(['c1'])).resolves.toEqual({ ok: true, data: null });
  expect(mockIPC).toHaveBeenCalledWith(
    `plugin:${COMMAND_PLUGINS.dismiss_vault_conflicts}|dismiss_vault_conflicts`,
    { ids: ['c1'] },
  );
});
