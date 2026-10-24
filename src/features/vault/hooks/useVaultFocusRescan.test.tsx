import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { toast } from '@/stores/toastStore';

import { useVaultFocusRescan } from './useVaultFocusRescan';
import { useVaultImportStore } from '../stores/vaultImportStore';


const mocks = vi.hoisted(() => ({
  rescanVault: vi.fn(),
  onFocusChanged: vi.fn(),
  unlisten: vi.fn(),
  focusHandler: null as null | ((event: { payload: boolean }) => void),
}));
vi.mock('@/lib/api', () => ({ VaultAPI: { rescanVault: mocks.rescanVault }, default: { rescanVault: mocks.rescanVault } }));
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ onFocusChanged: mocks.onFocusChanged }) }));

const focus = (focused = true) => act(async () => { mocks.focusHandler!({ payload: focused }); });

beforeEach(() => {
  vi.useFakeTimers({ toFake: ['Date'] });
  vi.setSystemTime(new Date(2026, 9, 24, 10, 0, 0));
  mocks.rescanVault.mockReset().mockResolvedValue({ ok: true, data: { imported: 0, deleted: 0 } });
  mocks.unlisten.mockReset();
  mocks.onFocusChanged.mockReset().mockImplementation(async (handler: (event: { payload: boolean }) => void) => {
    mocks.focusHandler = handler;
    return mocks.unlisten;
  });
  useVaultImportStore.setState({ importTick: 0, lastImportedId: null });
  vi.spyOn(toast, 'info');
});
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

async function mount() {
  const view = renderHook(() => useVaultFocusRescan());
  await waitFor(() => expect(mocks.focusHandler).not.toBeNull());
  return view;
}

describe('useVaultFocusRescan', () => {
  it('rescans on focus, announces the window, and ignores blur', async () => {
    const focused = vi.fn();
    window.addEventListener('lattice:window-focused', focused);
    await mount();
    await focus(false);
    expect(mocks.rescanVault).not.toHaveBeenCalled();
    await focus();
    expect(mocks.rescanVault).toHaveBeenCalledTimes(1);
    expect(focused).toHaveBeenCalledTimes(1);
    expect(toast.info).not.toHaveBeenCalled();
    expect(useVaultImportStore.getState().importTick).toBe(0);
    window.removeEventListener('lattice:window-focused', focused);
  });

  it('rescans at most once every two seconds', async () => {
    await mount();
    await focus();
    vi.setSystemTime(new Date(2026, 9, 24, 10, 0, 1));
    await focus();
    expect(mocks.rescanVault).toHaveBeenCalledTimes(1);
    vi.setSystemTime(new Date(2026, 9, 24, 10, 0, 2));
    await focus();
    expect(mocks.rescanVault).toHaveBeenCalledTimes(2);
  });

  it.each([
    [{ imported: 2, deleted: 0 }, 'Vault sync: re-imported 2 notes'],
    [{ imported: 0, deleted: 1 }, 'Vault sync: synced 1 deletion note'],
    [{ imported: 1, deleted: 3 }, 'Vault sync: re-imported 1, synced 3 deletions notes'],
  ])('reports what a rescan changed (%j) and refreshes listeners', async (data, message) => {
    mocks.rescanVault.mockResolvedValue({ ok: true, data });
    await mount();
    await focus();
    expect(toast.info).toHaveBeenCalledWith(message);
    expect(useVaultImportStore.getState()).toMatchObject({ importTick: 1, lastImportedId: '' });
  });

  it('logs a failed rescan without a toast', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    mocks.rescanVault.mockResolvedValue({ ok: false, error: 'vault folder missing' });
    await mount();
    await focus();
    expect(warn).toHaveBeenCalledWith('[useVaultFocusRescan] rescan failed:', 'vault folder missing');
    expect(toast.info).not.toHaveBeenCalled();
  });

  it('unsubscribes on unmount, even when the subscription lands afterwards', async () => {
    const { unmount } = await mount();
    unmount();
    expect(mocks.unlisten).toHaveBeenCalledTimes(1);

    let attach!: (unlisten: () => void) => void;
    mocks.onFocusChanged.mockReturnValue(new Promise((resolve) => { attach = resolve; }));
    const late = renderHook(() => useVaultFocusRescan());
    late.unmount();
    const lateUnlisten = vi.fn();
    await act(async () => { attach(lateUnlisten); });
    expect(lateUnlisten).toHaveBeenCalledTimes(1);
  });

  it('logs when focus events are unavailable', async () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    mocks.onFocusChanged.mockRejectedValue(new Error('no window'));
    renderHook(() => useVaultFocusRescan());
    await waitFor(() => expect(error).toHaveBeenCalledWith('[useVaultFocusRescan] failed to subscribe to focus events:', expect.any(Error)));
  });
});
