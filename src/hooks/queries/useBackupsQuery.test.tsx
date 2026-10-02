import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { migrateLegacyCustomCollections } from '@/hooks/queries/useCustomCollectionsQuery';
import VaultAPI from '@/lib/api';

import { useCreateBackupMutation } from './useBackupsQuery';

vi.mock('@/lib/api', () => ({ default: {
  createBackup: vi.fn(),
  listBackups: vi.fn().mockResolvedValue({ ok: true, data: [] }),
} }));
vi.mock('@/hooks/queries/useCustomCollectionsQuery', () => ({
  migrateLegacyCustomCollections: vi.fn(),
}));

function createWrapper() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return ({ children }: { children: React.ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}

describe('useCreateBackupMutation', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(migrateLegacyCustomCollections).mockResolvedValue();
    vi.mocked(VaultAPI.createBackup).mockResolvedValue({ ok: true, data: { backupPath: '/backup.db', size: 1, createdAt: 'now' } });
  });

  it('migrates legacy collections before taking the SQLite snapshot', async () => {
    const order: string[] = [];
    vi.mocked(migrateLegacyCustomCollections).mockImplementation(async () => { order.push('migrate'); });
    vi.mocked(VaultAPI.createBackup).mockImplementation(async () => {
      order.push('backup');
      return { ok: true, data: { backupPath: '/backup.db', size: 1, createdAt: 'now' } };
    });
    const { result } = renderHook(() => useCreateBackupMutation(), { wrapper: createWrapper() });
    await act(async () => { await result.current.mutateAsync(); });
    expect(order).toEqual(['migrate', 'backup']);
  });

  it('does not create an incomplete backup if the legacy migration fails', async () => {
    vi.mocked(migrateLegacyCustomCollections).mockRejectedValue(new Error('collection storage unavailable'));
    const { result } = renderHook(() => useCreateBackupMutation(), { wrapper: createWrapper() });
    await expect(result.current.mutateAsync()).rejects.toThrow('collection storage unavailable');
    expect(VaultAPI.createBackup).not.toHaveBeenCalled();
  });
});
