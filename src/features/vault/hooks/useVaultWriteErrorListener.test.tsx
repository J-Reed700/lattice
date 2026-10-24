import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { toast } from '@/stores/toastStore';

import { useVaultWriteErrorListener } from './useVaultWriteErrorListener';

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
  vi.spyOn(toast, 'error');
});
afterEach(() => vi.restoreAllMocks());

describe('useVaultWriteErrorListener', () => {
  it('keeps a mirror failure on screen until dismissed, saying the note itself was saved', async () => {
    const { unmount } = renderHook(() => useVaultWriteErrorListener());
    await waitFor(() => expect(events.handlers.has('vault:write-error')).toBe(true));
    act(() => events.handlers.get('vault:write-error')!({ payload: { target: '/vault/bread.md', error: 'disk full' } }));
    expect(toast.error).toHaveBeenCalledWith('Note saved, but vault mirror failed', { message: 'disk full\n\nFile: /vault/bread.md', duration: 0 });
    unmount();
    expect(events.unlisten).toHaveBeenCalledTimes(1);
  });

  it('ignores a malformed payload', async () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    renderHook(() => useVaultWriteErrorListener());
    await waitFor(() => expect(events.handlers.has('vault:write-error')).toBe(true));
    act(() => events.handlers.get('vault:write-error')!({ payload: { target: 3 } }));
    expect(toast.error).not.toHaveBeenCalled();
    expect(error).toHaveBeenCalledWith('[useVaultWriteErrorListener] payload validation failed:', expect.anything());
  });

  it('drops a subscription that lands after unmount, and logs one that fails', async () => {
    let attach!: (unlisten: () => void) => void;
    events.listen.mockReturnValueOnce(new Promise((resolve) => { attach = resolve; }));
    const { unmount } = renderHook(() => useVaultWriteErrorListener());
    unmount();
    const late = vi.fn();
    await act(async () => { attach(late); });
    expect(late).toHaveBeenCalledTimes(1);

    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    events.listen.mockRejectedValueOnce(new Error('event bus closed'));
    renderHook(() => useVaultWriteErrorListener());
    await waitFor(() => expect(error).toHaveBeenCalledWith('[useVaultWriteErrorListener] failed to subscribe:', expect.any(Error)));
  });
});
