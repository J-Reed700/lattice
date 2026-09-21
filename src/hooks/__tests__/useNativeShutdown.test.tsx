import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { registerPendingSave } from '@/lib/pendingSaves';

import { useNativeShutdown } from '../useNativeShutdown';

const mocks = vi.hoisted(() => ({ listen: vi.fn(), emit: vi.fn(), stop: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ isTauri: () => true }));
vi.mock('@tauri-apps/api/event', () => ({ listen: mocks.listen, emit: mocks.emit }));
let request: (event: { payload: number }) => Promise<void>;
beforeEach(() => {
  vi.clearAllMocks();
  mocks.emit.mockResolvedValue(undefined);
  mocks.listen.mockImplementation(async (_name, handler) => { request = handler; return mocks.stop; });
});
describe('native shutdown', () => {
  it('commits the focused field before flushing repository saves', async () => {
    const field = document.createElement('input');
    document.body.appendChild(field);
    field.value = 'Unsaved page title';
    let committed = '';
    field.addEventListener('blur', () => { committed = field.value; });
    const unregister = registerPendingSave(async () => committed === field.value);
    const { result, unmount } = renderHook(useNativeShutdown);
    try {
      await waitFor(() => expect(result.current.ready).toBe(true));
      field.focus();
      await act(async () => { await request({ payload: 11 }); });
      expect(mocks.emit).toHaveBeenCalledWith('lattice:shutdown-response', { requestId: 11, saved: true });
    } finally { unregister(); unmount(); field.remove(); }
  });

  it('registers before reporting ready and waits for repository acknowledgement', async () => {
    let finish!: (saved: boolean) => void;
    const unregister = registerPendingSave(() => new Promise((resolve) => { finish = resolve; }));
    const { result, unmount } = renderHook(useNativeShutdown);
    try {
      expect(result.current.ready).toBe(false);
      await waitFor(() => expect(result.current.ready).toBe(true));
      expect(mocks.emit).toHaveBeenCalledWith('lattice:renderer-ready');
      let closing!: Promise<void>;
      act(() => { closing = request({ payload: 7 }); });
      await waitFor(() => expect(result.current.isQuitting).toBe(true));
      expect(mocks.emit).not.toHaveBeenCalledWith('lattice:shutdown-response', expect.anything());
      await act(async () => { finish(true); await closing; });
      expect(mocks.emit).toHaveBeenCalledWith('lattice:shutdown-response', { requestId: 7, saved: true });
    } finally { unregister(); unmount(); }
    expect(mocks.stop).toHaveBeenCalledOnce();
  });
  it('keeps editing available after save failure and supports another quit attempt', async () => {
    const save = vi.fn().mockRejectedValueOnce(new Error('Disk full')).mockResolvedValue(true);
    const unregister = registerPendingSave(save);
    const { result, unmount } = renderHook(useNativeShutdown);
    try {
      await waitFor(() => expect(result.current.ready).toBe(true));
      await act(async () => { await request({ payload: 1 }); });
      expect(mocks.emit).toHaveBeenCalledWith('lattice:shutdown-response', { requestId: 1, saved: false });
      expect(result.current.isQuitting).toBe(false);
      expect(result.current.error).toContain('still open');
      await act(async () => { await request({ payload: 2 }); });
      expect(mocks.emit).toHaveBeenCalledWith('lattice:shutdown-response', { requestId: 2, saved: true });
    } finally { unregister(); unmount(); }
  });
  it('cancels a slow quit without allowing its late save to close the app', async () => {
    let finish!: (saved: boolean) => void;
    const unregister = registerPendingSave(() => new Promise((resolve) => { finish = resolve; }));
    const { result, unmount } = renderHook(useNativeShutdown);
    try {
      await waitFor(() => expect(result.current.ready).toBe(true));
      let closing!: Promise<void>;
      act(() => { closing = request({ payload: 9 }); });
      await waitFor(() => expect(result.current.isQuitting).toBe(true));
      await act(async () => { await result.current.cancelQuit(); });
      expect(result.current.isQuitting).toBe(false);
      await act(async () => { finish(true); await closing; });
      expect(mocks.emit).toHaveBeenCalledWith('lattice:shutdown-response', { requestId: 9, saved: false });
      expect(mocks.emit).not.toHaveBeenCalledWith('lattice:shutdown-response', { requestId: 9, saved: true });
    } finally { unregister(); unmount(); }
  });
  it('ignores an old completion when a failed cancellation reuses the native request ID', async () => {
    let first!: (saved: boolean) => void;
    let second!: (saved: boolean) => void;
    const save = vi.fn()
      .mockImplementationOnce(() => new Promise((resolve) => { first = resolve; }))
      .mockImplementationOnce(() => new Promise((resolve) => { second = resolve; }));
    const unregister = registerPendingSave(save);
    const { result, unmount } = renderHook(useNativeShutdown);
    try {
      await waitFor(() => expect(result.current.ready).toBe(true));
      let closing!: Promise<void>;
      act(() => { closing = request({ payload: 9 }); });
      await waitFor(() => expect(save).toHaveBeenCalledTimes(1));
      mocks.emit.mockRejectedValueOnce(new Error('IPC unavailable'));
      await act(async () => { await result.current.cancelQuit(); });
      let retry!: Promise<void>;
      act(() => { retry = request({ payload: 9 }); });
      await waitFor(() => expect(save).toHaveBeenCalledTimes(2));
      await act(async () => { first(true); await closing; });
      expect(mocks.emit).not.toHaveBeenCalledWith('lattice:shutdown-response', { requestId: 9, saved: true });
      await act(async () => { second(true); await retry; });
      expect(mocks.emit).toHaveBeenCalledWith('lattice:shutdown-response', { requestId: 9, saved: true });
    } finally { unregister(); unmount(); }
  });
  it('does not enable editing if shutdown registration fails', async () => {
    mocks.listen.mockRejectedValue(new Error('IPC unavailable'));
    const { result } = renderHook(useNativeShutdown);
    await waitFor(() => expect(result.current.error).toContain('Restart Lattice'));
    expect(result.current.ready).toBe(false);
  });
});
