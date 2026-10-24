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
