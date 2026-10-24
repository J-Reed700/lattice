import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useBackupAndExport } from './useBackupAndExport';

const h = vi.hoisted(() => ({
  api: { createBackup: vi.fn(), exportMarkdown: vi.fn(), exportJson: vi.fn(), showInFolder: vi.fn(), listBackups: vi.fn() },
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() },
}));
vi.mock('@/lib/api', () => ({ default: h.api, VaultAPI: h.api }));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));

let client: QueryClient;
const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
const ok = <T,>(data: T) => ({ ok: true as const, data });

describe('useBackupAndExport', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    client = new QueryClient({ defaultOptions: { mutations: { retry: false } } });
    h.api.showInFolder.mockResolvedValue(ok(undefined));
  });

  it('backs up and offers to show the file', async () => {
    h.api.createBackup.mockResolvedValueOnce(ok({ backupPath: '/backups/lattice-1.db' }));
    const { result } = renderHook(() => useBackupAndExport(), { wrapper });
    act(() => result.current.runBackup());
    await waitFor(() => expect(h.toast.success).toHaveBeenCalledWith('Backup created', expect.objectContaining({ message: 'Saved to your backups folder.' })));
    h.toast.success.mock.calls[0][1].action.onClick();
    expect(h.api.showInFolder).toHaveBeenCalledWith('/backups/lattice-1.db');
  });

  it('reports a backup that failed', async () => {
    h.api.createBackup.mockResolvedValueOnce({ ok: false, error: 'disk full' });
    const { result } = renderHook(() => useBackupAndExport(), { wrapper });
    act(() => result.current.runBackup());
    await waitFor(() => expect(h.toast.error).toHaveBeenCalledWith("Couldn't back up", { message: 'disk full' }));
  });

  it.each([
    ['markdown', 'exportMarkdown', 'Markdown files are in your exports folder.', 3, 'Exported 3 items'],
    ['json', 'exportJson', 'The JSON file is in your exports folder.', 1, 'Exported 1 item'],
  ] as const)('exports as %s and offers the folder', async (format, method, message, count, title) => {
    h.api[method].mockResolvedValueOnce(ok({ count, outputDir: '/exports/today' }));
    const { result } = renderHook(() => useBackupAndExport(), { wrapper });
    act(() => result.current.runExport(format));
    await waitFor(() => expect(h.toast.success).toHaveBeenCalledWith(title, expect.objectContaining({ message })));
    h.toast.success.mock.calls[0][1].action.onClick();
    expect(h.api.showInFolder).toHaveBeenCalledWith('/exports/today');
  });

  it('says when there is nothing to export, and reports a failure', async () => {
    h.api.exportMarkdown.mockResolvedValueOnce(ok({ count: 0, outputDir: '/x' })).mockResolvedValueOnce({ ok: false, error: 'locked' });
    const { result } = renderHook(() => useBackupAndExport(), { wrapper });
    act(() => result.current.runExport('markdown'));
    await waitFor(() => expect(h.toast.info).toHaveBeenCalledWith('Nothing to export yet.'));
    act(() => result.current.runExport('markdown'));
    await waitFor(() => expect(h.toast.error).toHaveBeenCalledWith("Couldn't export", { message: 'locked' }));
  });

  it('shows a path in its folder', async () => {
    const { result } = renderHook(() => useBackupAndExport(), { wrapper });
    await result.current.showInFolder('/backups');
    expect(h.api.showInFolder).toHaveBeenCalledWith('/backups');
  });
});
