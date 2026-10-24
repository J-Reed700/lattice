import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import VaultAPI from '@/lib/api';

import {
  ARCHIVE_STATUS_QUERY_KEY,
  useArchiveStatusQuery,
  useBeginArchiveSetupMutation,
  useChooseArchiveDestinationMutation,
  useConfirmArchiveSetupMutation,
  useCreateArchiveNowMutation,
  useDisableArchiveMutation,
  useRestoreArchiveMutation,
  useRotateRecoveryCodeMutation,
  useSetArchiveKeepCountMutation,
  useSetArchivePassphraseMutation,
} from './useArchiveQuery';
import { BACKUPS_QUERY_KEY } from './useBackupsQuery';

const api = vi.mocked(VaultAPI);

let client: QueryClient;
const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;

const status = { configured: true, destination: '/Volumes/Backup', keepCount: 3 };
const refused = (error: string) => ({ ok: false as const, error });

/** Runs one mutation and returns its outcome. */
async function run<T>(hook: () => { mutateAsync: (v: T) => Promise<unknown> }, variables: T) {
  const { result } = renderHook(hook, { wrapper });
  let value: unknown;
  let error: unknown;
  await act(async () => {
    try { value = await result.current.mutateAsync(variables); } catch (caught) { error = caught; }
  });
  return { value, error };
}

describe('useArchiveQuery', () => {
  beforeEach(() => {
    client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
    vi.spyOn(client, 'invalidateQueries');
  });

  it('reads the archive status', async () => {
    api.getArchiveStatus.mockResolvedValueOnce({ ok: true, data: status } as never);
    const { result } = renderHook(() => useArchiveStatusQuery(), { wrapper });
    await waitFor(() => expect(result.current.data).toEqual(status));
  });

  it('turns a refused status read into an error', async () => {
    api.getArchiveStatus.mockResolvedValueOnce(refused('archive config unreadable') as never);
    const { result } = renderHook(() => useArchiveStatusQuery(), { wrapper });
    await waitFor(() => expect(result.current.error?.message).toBe('archive config unreadable'));
  });

  it.each([
    ['begin setup', () => useBeginArchiveSetupMutation(), undefined, 'beginArchiveSetup'],
    ['rotate the recovery code', () => useRotateRecoveryCodeMutation(), undefined, 'rotateRecoveryCode'],
  ] as const)('%s returns the words, or throws the refusal', async (_label, hook, variables, method) => {
    const setup = { recoveryWords: ['oak'], confirmIndices: [0] };
    (api[method] as ReturnType<typeof vi.fn>).mockResolvedValueOnce({ ok: true, data: setup });
    expect((await run(hook as never, variables)).value).toEqual(setup);
    (api[method] as ReturnType<typeof vi.fn>).mockResolvedValueOnce(refused('no entropy'));
    expect(((await run(hook as never, variables)).error as Error).message).toBe('no entropy');
  });

  it.each([
    ['confirm setup', () => useConfirmArchiveSetupMutation(), { confirmations: [{ index: 0, word: 'oak' }], passphrase: null }, 'confirmArchiveSetup', [[{ index: 0, word: 'oak' }], null]],
    ['choose a destination', () => useChooseArchiveDestinationMutation(), undefined, 'chooseArchiveDestination', []],
    ['set how many to keep', () => useSetArchiveKeepCountMutation(), 7, 'setArchiveKeepCount', [7]],
    ['set a passphrase', () => useSetArchivePassphraseMutation(), 'moss and stone', 'setArchivePassphrase', ['moss and stone']],
    ['disable the archive', () => useDisableArchiveMutation(), undefined, 'disableArchive', []],
  ] as const)('%s seeds the status cache, or throws the refusal', async (_label, hook, variables, method, args) => {
    (api[method] as ReturnType<typeof vi.fn>).mockResolvedValueOnce({ ok: true, data: status });
    expect((await run(hook as never, variables)).value).toEqual(status);
    expect(api[method]).toHaveBeenLastCalledWith(...args);
    expect(client.getQueryData(ARCHIVE_STATUS_QUERY_KEY)).toEqual(status);
    expect(client.invalidateQueries).toHaveBeenCalledWith({ queryKey: ARCHIVE_STATUS_QUERY_KEY });
    (api[method] as ReturnType<typeof vi.fn>).mockResolvedValueOnce(refused('folder vanished'));
    expect(((await run(hook as never, variables)).error as Error).message).toBe('folder vanished');
  });

  it('refreshes the status after a backup run either way', async () => {
    api.createArchiveNow.mockResolvedValueOnce({ ok: true, data: { path: '/a', size: 1 } } as never);
    expect((await run(() => useCreateArchiveNowMutation(), undefined)).value).toEqual({ path: '/a', size: 1 });
    api.createArchiveNow.mockResolvedValueOnce(refused('disk full') as never);
    expect(((await run(() => useCreateArchiveNowMutation(), undefined)).error as Error).message).toBe('disk full');
    expect(vi.mocked(client.invalidateQueries).mock.calls.filter(([filters]) => filters?.queryKey === ARCHIVE_STATUS_QUERY_KEY)).toHaveLength(2);
  });

  it('refreshes status and backups only after a real restore', async () => {
    api.restoreArchive.mockResolvedValueOnce({ ok: true, data: { outcome: 'needs_secret' } } as never);
    await run(() => useRestoreArchiveMutation(), null);
    expect(client.invalidateQueries).not.toHaveBeenCalled();
    api.restoreArchive.mockResolvedValueOnce({ ok: true, data: { outcome: 'restored' } } as never);
    await run(() => useRestoreArchiveMutation(), 'moss and stone');
    expect(api.restoreArchive).toHaveBeenLastCalledWith('moss and stone');
    expect(client.invalidateQueries).toHaveBeenCalledWith({ queryKey: ARCHIVE_STATUS_QUERY_KEY });
    expect(client.invalidateQueries).toHaveBeenCalledWith({ queryKey: BACKUPS_QUERY_KEY });
    api.restoreArchive.mockResolvedValueOnce(refused('wrong passphrase') as never);
    expect(((await run(() => useRestoreArchiveMutation(), 'x')).error as Error).message).toBe('wrong passphrase');
  });
});
