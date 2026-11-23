import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import VaultAPI from '@/lib/api';
import { toast, useToastStore } from '@/stores/toastStore';

import { useVaultConflictListener } from './useVaultConflictListener';

const events = vi.hoisted(() => ({
  listen: vi.fn(),
  unlisten: vi.fn(),
  handlers: new Map<string, (event: { payload: unknown }) => void>(),
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: events.listen }));

beforeEach(() => {
  events.handlers.clear();
  events.unlisten.mockReset();
  events.listen.mockReset().mockImplementation(async (name: string, handler: (event: { payload: unknown }) => void) => {
    events.handlers.set(name, handler);
    return events.unlisten;
  });
  vi.mocked(VaultAPI.listVaultConflicts).mockReset().mockResolvedValue({ ok: true, data: [] });
  vi.mocked(VaultAPI.dismissVaultConflicts).mockReset().mockResolvedValue({ ok: true, data: null });
  useToastStore.getState().dismissAll();
  vi.spyOn(toast, 'warning');
});
afterEach(() => vi.restoreAllMocks());

describe('useVaultConflictListener', () => {
  it('says both versions were kept, naming the copy, until dismissed', async () => {
    const { unmount } = renderHook(() => useVaultConflictListener());
    await waitFor(() => expect(events.handlers.has('vault:note-conflict')).toBe(true));
    act(() => events.handlers.get('vault:note-conflict')!({
      payload: { id: 'c1', kind: 'keptCopy', noteId: 'n1', noteTitle: 'Bread', copyId: 'n2', copyTitle: 'Bread (conflicted copy 2026-11-22)' },
    }));
    expect(toast.warning).toHaveBeenCalledWith(
      'Kept both versions of "Bread"',
      {
        message: 'It was changed in Lattice and in its file at the same time. The note now has the file\'s version. Lattice\'s version is in "Bread (conflicted copy 2026-11-22)".',
        duration: 0,
      },
    );
    const toastId = useToastStore.getState().toasts[0]?.id;
    expect(toastId).toBeDefined();
    act(() => toast.dismiss(toastId!));
    expect(VaultAPI.dismissVaultConflicts).toHaveBeenCalledWith(['c1']);
    unmount();
    expect(events.unlisten).toHaveBeenCalledTimes(1);
  });

  it('says where a replaced file was kept', async () => {
    renderHook(() => useVaultConflictListener());
    await waitFor(() => expect(events.handlers.has('vault:note-conflict')).toBe(true));
    act(() => events.handlers.get('vault:note-conflict')!({
      payload: { id: 'c2', kind: 'setAside', noteId: 'n1', noteTitle: 'Bread', setAsidePath: '/vault/notes/n1.replaced-20261122-101500.md' },
    }));
    expect(toast.warning).toHaveBeenCalledWith(
      'Kept the file that replaced "Bread"',
      {
        message: "Its file was replaced by one that isn't this note. That file is kept at /vault/notes/n1.replaced-20261122-101500.md, and the note was saved back to its own file.",
        duration: 0,
      },
    );
  });

  it('surfaces startup conflicts recorded before the listener attached and deduplicates live events', async () => {
    const startupConflict = {
      kind: 'keptCopy' as const,
      id: 'startup-1',
      noteId: 'n1',
      noteTitle: 'Startup Note',
      copyId: 'c1',
      copyTitle: 'Startup Note (conflicted copy 2026-11-22)',
    };
    vi.mocked(VaultAPI.listVaultConflicts).mockResolvedValueOnce({
      ok: true,
      data: [startupConflict],
    });

    renderHook(() => useVaultConflictListener());
    await waitFor(() => expect(useToastStore.getState().toasts).toHaveLength(1));
    expect(useToastStore.getState().toasts[0]?.title).toBe('Kept both versions of "Startup Note"');

    // Re-receiving the same conflict id via live event does not duplicate the toast.
    act(() => events.handlers.get('vault:note-conflict')!({ payload: startupConflict }));
    expect(useToastStore.getState().toasts).toHaveLength(1);
  });

  it('preserves all conflicts across >5 conflicts in one pass and across transient info toasts', async () => {
    renderHook(() => useVaultConflictListener());
    await waitFor(() => expect(events.handlers.has('vault:note-conflict')).toBe(true));

    act(() => {
      for (let i = 1; i <= 8; i++) {
        events.handlers.get('vault:note-conflict')!({
          payload: {
            id: `c-${i}`,
            kind: 'keptCopy',
            noteId: `n-${i}`,
            noteTitle: `Note ${i}`,
            copyId: `copy-${i}`,
            copyTitle: `Note ${i} (conflicted copy 2026-11-22)`,
          },
        });
      }
      for (let i = 1; i <= 6; i++) {
        toast.info(`Info ${i}`, { duration: 4000 });
      }
    });

    const warnings = useToastStore.getState().toasts.filter((t) => t.type === 'warning');
    expect(warnings).toHaveLength(4);
    const summary = warnings.find((t) => t.title === 'Kept both versions for 5 more notes');
    expect(summary).toBeDefined();
    for (let i = 4; i <= 8; i++) {
      expect(summary?.message).toContain(`"Note ${i}" -> "Note ${i} (conflicted copy 2026-11-22)"`);
    }

    act(() => toast.dismiss(summary!.id));
    expect(VaultAPI.dismissVaultConflicts).toHaveBeenCalledWith([
      'c-4',
      'c-5',
      'c-6',
      'c-7',
      'c-8',
    ]);
  });

  it('ignores a malformed payload', async () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    renderHook(() => useVaultConflictListener());
    await waitFor(() => expect(events.handlers.has('vault:note-conflict')).toBe(true));
    act(() => events.handlers.get('vault:note-conflict')!({ payload: { id: 'c3', kind: 'keptCopy', noteId: 3 } }));
    expect(toast.warning).not.toHaveBeenCalled();
    expect(error).toHaveBeenCalledWith('[useVaultConflictListener] payload validation failed:', expect.anything());
  });

  it('drops a subscription that lands after unmount, and logs one that fails', async () => {
    let attach!: (unlisten: () => void) => void;
    events.listen.mockReturnValueOnce(new Promise((resolve) => { attach = resolve; }));
    const { unmount } = renderHook(() => useVaultConflictListener());
    await waitFor(() => expect(events.listen).toHaveBeenCalledTimes(1));
    unmount();
    const late = vi.fn();
    await act(async () => { attach(late); });
    expect(late).toHaveBeenCalledTimes(1);

    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    events.listen.mockRejectedValueOnce(new Error('event bus closed'));
    renderHook(() => useVaultConflictListener());
    await waitFor(() => expect(error).toHaveBeenCalledWith('[useVaultConflictListener] failed to subscribe:', expect.any(Error)));
    expect(VaultAPI.listVaultConflicts).not.toHaveBeenCalled();
  });
});
