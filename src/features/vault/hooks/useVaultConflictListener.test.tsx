import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { toast } from '@/stores/toastStore';

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
  vi.spyOn(toast, 'warning');
});
afterEach(() => vi.restoreAllMocks());

describe('useVaultConflictListener', () => {
  it('says both versions were kept, naming the copy, until dismissed', async () => {
    const { unmount } = renderHook(() => useVaultConflictListener());
    await waitFor(() => expect(events.handlers.has('vault:note-conflict')).toBe(true));
    act(() => events.handlers.get('vault:note-conflict')!({
      payload: { kind: 'keptCopy', noteId: 'n1', noteTitle: 'Bread', copyId: 'n2', copyTitle: 'Bread (conflicted copy 2026-11-22)' },
    }));
    expect(toast.warning).toHaveBeenCalledWith(
      'Kept both versions of "Bread"',
      {
        message: 'It was changed in Lattice and in its file at the same time. The note now has the file\'s version. Lattice\'s version is in "Bread (conflicted copy 2026-11-22)".',
        duration: 0,
      },
    );
    unmount();
    expect(events.unlisten).toHaveBeenCalledTimes(1);
  });

  it('says where a replaced file was kept', async () => {
    renderHook(() => useVaultConflictListener());
    await waitFor(() => expect(events.handlers.has('vault:note-conflict')).toBe(true));
    act(() => events.handlers.get('vault:note-conflict')!({
      payload: { kind: 'setAside', noteId: 'n1', noteTitle: 'Bread', setAsidePath: '/vault/notes/n1.replaced-20261122-101500.md' },
    }));
    expect(toast.warning).toHaveBeenCalledWith(
      'Kept the file that replaced "Bread"',
      {
        message: "Its file was replaced by one that isn't this note. That file is kept at /vault/notes/n1.replaced-20261122-101500.md, and the note was saved back to its own file.",
        duration: 0,
      },
    );
  });

  it('ignores a malformed payload', async () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    renderHook(() => useVaultConflictListener());
    await waitFor(() => expect(events.handlers.has('vault:note-conflict')).toBe(true));
    act(() => events.handlers.get('vault:note-conflict')!({ payload: { kind: 'keptCopy', noteId: 3 } }));
    expect(toast.warning).not.toHaveBeenCalled();
    expect(error).toHaveBeenCalledWith('[useVaultConflictListener] payload validation failed:', expect.anything());
  });

  it('drops a subscription that lands after unmount, and logs one that fails', async () => {
    let attach!: (unlisten: () => void) => void;
    events.listen.mockReturnValueOnce(new Promise((resolve) => { attach = resolve; }));
    const { unmount } = renderHook(() => useVaultConflictListener());
    unmount();
    const late = vi.fn();
    await act(async () => { attach(late); });
    expect(late).toHaveBeenCalledTimes(1);

    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    events.listen.mockRejectedValueOnce(new Error('event bus closed'));
    renderHook(() => useVaultConflictListener());
    await waitFor(() => expect(error).toHaveBeenCalledWith('[useVaultConflictListener] failed to subscribe:', expect.any(Error)));
  });
});
