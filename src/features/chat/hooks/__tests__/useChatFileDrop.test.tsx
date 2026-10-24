import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useChatFileDrop } from '@/features/chat/hooks/useChatFileDrop';
import { deferred } from '@/tests/deferred';

type Payload = { type: 'enter' | 'over' | 'drop' | 'leave'; paths?: string[]; position?: { x: number; y: number } };

const h = vi.hoisted(() => ({
  handler: null as null | ((event: { payload: Payload }) => void),
  onDragDropEvent: vi.fn(),
  unlisten: vi.fn(),
}));
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({ onDragDropEvent: h.onDragDropEvent }) }));

const panel = () => {
  const element = document.createElement('div');
  element.getBoundingClientRect = () => ({ left: 100, right: 500, top: 50, bottom: 450 } as DOMRect);
  return { current: element };
};

const setup = (stageable?: (path: string) => boolean, ref: { current: HTMLElement | null } = panel()) => {
  const onDrop = vi.fn();
  const hook = renderHook(() => useChatFileDrop(ref, onDrop, stageable));
  return { ...hook, onDrop };
};

const emit = (payload: Payload) => act(() => h.handler!({ payload }));

describe('useChatFileDrop', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.handler = null;
    h.onDragDropEvent.mockImplementation(async (handler: (event: { payload: Payload }) => void) => { h.handler = handler; return h.unlisten; });
    Object.defineProperty(window, 'devicePixelRatio', { configurable: true, value: 2 });
  });

  afterEach(() => {
    Object.defineProperty(window, 'devicePixelRatio', { configurable: true, value: 1 });
  });

  it('highlights only while a drag is over the panel', async () => {
    const { result } = setup();
    await waitFor(() => expect(h.handler).not.toBeNull());
    emit({ type: 'enter', position: { x: 400, y: 400 } });
    expect(result.current.isDragging).toBe(true);
    emit({ type: 'over', position: { x: 2000, y: 400 } });
    expect(result.current.isDragging).toBe(false);
    emit({ type: 'over' });
    expect(result.current.isDragging).toBe(true);
    emit({ type: 'leave' });
    expect(result.current.isDragging).toBe(false);
  });

  it('stages files dropped on the panel once each, and hands every path on', async () => {
    const { result, onDrop } = setup((path) => !path.endsWith('.png'));
    await waitFor(() => expect(h.handler).not.toBeNull());
    emit({ type: 'drop', position: { x: 400, y: 400 }, paths: ['/notes/rome.md', 'C:\\photos\\forum.png', ''] });
    expect(result.current.staged).toEqual([{ path: '/notes/rome.md', name: 'rome.md' }]);
    expect(onDrop).toHaveBeenCalledWith(['/notes/rome.md', 'C:\\photos\\forum.png']);
    emit({ type: 'drop', position: { x: 400, y: 400 }, paths: ['/notes/rome.md'] });
    expect(result.current.staged).toHaveLength(1);
  });

  it('ignores drops outside the panel and empty drops', async () => {
    const { result, onDrop } = setup();
    await waitFor(() => expect(h.handler).not.toBeNull());
    emit({ type: 'drop', position: { x: 10, y: 10 }, paths: ['/a.md'] });
    emit({ type: 'drop', position: { x: 400, y: 400 }, paths: [] });
    emit({ type: 'drop', position: { x: 400, y: 400 } });
    expect(result.current.staged).toEqual([]);
    expect(onDrop).not.toHaveBeenCalled();
  });

  it('ignores drops with no panel mounted', async () => {
    const { result, onDrop } = setup(undefined, { current: null });
    await waitFor(() => expect(h.handler).not.toBeNull());
    emit({ type: 'drop', paths: ['/a.md'] });
    expect(result.current.isDragging).toBe(false);
    expect(onDrop).not.toHaveBeenCalled();
  });

  it('stages picked files, removes one and clears all', () => {
    const { result } = setup((path) => path !== '/skip.exe');
    act(() => result.current.add(['/a/notes.md', '/skip.exe', '', '/b/']));
    expect(result.current.staged).toEqual([{ path: '/a/notes.md', name: 'notes.md' }, { path: '/b/', name: '/b/' }]);
    act(() => result.current.add(['/a/notes.md']));
    act(() => result.current.add([]));
    expect(result.current.staged).toHaveLength(2);
    act(() => result.current.remove('/b/'));
    expect(result.current.staged).toEqual([{ path: '/a/notes.md', name: 'notes.md' }]);
    act(() => result.current.clear());
    expect(result.current.staged).toEqual([]);
  });

  // Bug: useChatFileDrop.ts:76 only skips paths already staged, not repeats
  // within one batch, so the same file passed twice in one call is staged twice.
  it.fails('stages a file passed twice in one batch once', () => {
    const { result } = setup();
    act(() => result.current.add(['/a/notes.md', '/a/notes.md']));
    expect(result.current.staged).toHaveLength(1);
  });

  it('stops listening on unmount, even if the listener lands late', async () => {
    const first = setup();
    await waitFor(() => expect(h.handler).not.toBeNull());
    first.unmount();
    expect(h.unlisten).toHaveBeenCalledTimes(1);
    const late = deferred<() => void>();
    const lateUnlisten = vi.fn();
    h.onDragDropEvent.mockReturnValueOnce(late.promise);
    const second = setup();
    second.unmount();
    await act(async () => { late.resolve(lateUnlisten); await late.promise; });
    expect(lateUnlisten).toHaveBeenCalled();
  });

  it('works without the webview bridge', async () => {
    h.onDragDropEvent.mockRejectedValueOnce(new Error('not in Tauri'));
    const { result } = setup();
    await act(async () => {});
    expect(result.current.isDragging).toBe(false);
  });
});
