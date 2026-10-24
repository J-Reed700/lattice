import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useChatCompaction } from '@/features/chat/hooks/useChatCompaction';
import { useCompactionStore } from '@/features/chat/stores/compactionStore';
import { deferred } from '@/tests/deferred';

const h = vi.hoisted(() => ({ compactConversation: vi.fn(), toast: { success: vi.fn() } }));
vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: (select: (state: unknown) => unknown) => select({ compactConversation: h.compactConversation }),
}));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));

const record = { upToMessageId: 'm4', summary: 'Earlier talk about grain prices' };
const conversations = [{ id: 'c1', compaction: { upToMessageId: 'm2' } }, { id: 'c2' }] as never;

const setup = (activeConversationId: string | null = 'c1') => {
  const container = document.createElement('div');
  container.scrollTo = vi.fn();
  Object.defineProperty(container, 'scrollHeight', { value: 900 });
  const scrollContainerRef = { current: container };
  const shouldAutoScrollRef = { current: false };
  const hook = renderHook(({ id }) => useChatCompaction({ activeConversationId: id, conversations, scrollContainerRef, shouldAutoScrollRef }), {
    initialProps: { id: activeConversationId },
  });
  return { ...hook, container, shouldAutoScrollRef };
};

describe('useChatCompaction', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useCompactionStore.setState({ runs: {} });
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => { callback(0); return 0; });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('reads the saved compaction for the open chat, and nothing without one', () => {
    const { result, rerender } = setup();
    expect(result.current.compactionRecord).toEqual({ upToMessageId: 'm2' });
    expect(result.current.isCompacting).toBe(false);
    rerender({ id: 'c2' });
    expect(result.current.compactionRecord).toBeNull();
    rerender({ id: null });
    expect(result.current.compactionRecord).toBeNull();
    expect(result.current.compactionRun).toBeUndefined();
  });

  it('runs a compaction, scrolls to its status, and keeps its record', async () => {
    const pending = deferred<{ ok: true; record: typeof record }>();
    h.compactConversation.mockReturnValueOnce(pending.promise);
    const { result, container, shouldAutoScrollRef } = setup();
    let run!: Promise<void>;
    act(() => { run = result.current.handleCompact('c1'); });
    expect(result.current.isCompacting).toBe(true);
    expect(shouldAutoScrollRef.current).toBe(true);
    expect(container.scrollTo).toHaveBeenCalledWith({ top: 900, behavior: 'smooth' });
    await act(async () => { pending.resolve({ ok: true, record }); await run; });
    expect(result.current.compactionRun?.state).toBe('done');
    expect(result.current.compactionRecord).toEqual(record);
    expect(h.toast.success).toHaveBeenCalledWith('Context compacted', expect.objectContaining({ message: expect.stringContaining('Active requirements preserved') }));
  });

  it('starts no second run while one is going', async () => {
    useCompactionStore.getState().start('c1');
    const { result } = setup();
    await act(() => result.current.handleCompact('c1'));
    expect(h.compactConversation).not.toHaveBeenCalled();
  });

  it('records why a compaction could not run', async () => {
    h.compactConversation.mockResolvedValueOnce({ ok: false, error: 'too short to compact' });
    const { result } = setup();
    await act(() => result.current.handleCompact('c1'));
    expect(result.current.compactionRun).toMatchObject({ state: 'failed', error: 'too short to compact' });
    expect(result.current.compactionRecord).toEqual({ upToMessageId: 'm2' });
  });

  it('records a compaction that threw', async () => {
    h.compactConversation.mockRejectedValueOnce(new Error('model crashed'));
    const { result } = setup();
    await act(() => result.current.handleCompact('c1'));
    expect(result.current.compactionRun).toMatchObject({ state: 'failed', error: 'model crashed' });
    h.compactConversation.mockRejectedValueOnce('offline');
    await act(() => result.current.handleCompact('c1'));
    expect(result.current.compactionRun).toMatchObject({ state: 'failed', error: 'offline' });
  });

  it('dismisses a finished run', async () => {
    h.compactConversation.mockResolvedValueOnce({ ok: false, error: 'x' });
    const { result } = setup();
    await act(() => result.current.handleCompact('c1'));
    act(() => result.current.dismissCompaction('c1'));
    expect(result.current.compactionRun).toBeUndefined();
  });
});
