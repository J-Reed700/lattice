import { act, renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { useThreadScroll } from '@/features/chat/hooks/useThreadScroll';

type Message = { id: string };
const key = (message: Message) => message.id;
const msgs = (...ids: string[]) => ids.map((id) => ({ id }));

type Props = { activeConversationId: string | null; messages: Message[]; prefersReducedMotion: boolean | null };

const setup = (initial: Props) => {
  const hook = renderHook((props: Props) => useThreadScroll({ ...props, getMessageKey: key }), { initialProps: initial });
  const scrollToIndex = vi.fn();
  (hook.result.current.scrollContainerRef as { current: HTMLDivElement | null }).current = document.createElement('div');
  (hook.result.current.messageListRef as { current: unknown }).current = { scrollToIndex };
  return { ...hook, scrollToIndex };
};

describe('useThreadScroll', () => {
  it('jumps to the end when a conversation opens, then glides as messages arrive', () => {
    const { rerender, scrollToIndex } = setup({ activeConversationId: null, messages: [], prefersReducedMotion: false });
    rerender({ activeConversationId: 'c1', messages: msgs('a', 'b'), prefersReducedMotion: false });
    expect(scrollToIndex).toHaveBeenLastCalledWith(1, { align: 'end', behavior: 'auto' });
    rerender({ activeConversationId: 'c1', messages: msgs('a', 'b', 'c'), prefersReducedMotion: false });
    expect(scrollToIndex).toHaveBeenLastCalledWith(2, { align: 'end', behavior: 'smooth' });
  });

  it('never glides when motion is reduced', () => {
    const { rerender, scrollToIndex } = setup({ activeConversationId: 'c1', messages: msgs('a'), prefersReducedMotion: true });
    rerender({ activeConversationId: 'c1', messages: msgs('a', 'b'), prefersReducedMotion: true });
    rerender({ activeConversationId: 'c1', messages: msgs('a', 'b', 'c'), prefersReducedMotion: true });
    expect(scrollToIndex).toHaveBeenLastCalledWith(2, { align: 'end', behavior: 'auto' });
  });

  it('treats the first batch after an empty conversation as a jump', () => {
    const { rerender, scrollToIndex } = setup({ activeConversationId: 'c1', messages: [], prefersReducedMotion: false });
    rerender({ activeConversationId: 'c1', messages: [], prefersReducedMotion: false });
    rerender({ activeConversationId: 'c1', messages: msgs('a', 'b'), prefersReducedMotion: false });
    expect(scrollToIndex).toHaveBeenLastCalledWith(1, { align: 'end', behavior: 'auto' });
  });

  it('stops following after a jump back, until the reader returns to the latest', () => {
    const { result, rerender, scrollToIndex } = setup({ activeConversationId: 'c1', messages: msgs('a', 'b'), prefersReducedMotion: false });
    rerender({ activeConversationId: 'c1', messages: msgs('a', 'b'), prefersReducedMotion: false });
    act(() => result.current.navigateToMessage(0));
    expect(scrollToIndex).toHaveBeenLastCalledWith(0, { align: 'start', behavior: 'auto' });
    scrollToIndex.mockClear();
    rerender({ activeConversationId: 'c1', messages: msgs('a', 'b', 'c'), prefersReducedMotion: false });
    expect(scrollToIndex).not.toHaveBeenCalled();
    act(() => result.current.navigateToLatest());
    expect(scrollToIndex).toHaveBeenLastCalledWith(2, { align: 'end', behavior: 'auto' });
    rerender({ activeConversationId: 'c1', messages: msgs('a', 'b', 'c', 'd'), prefersReducedMotion: false });
    expect(scrollToIndex).toHaveBeenLastCalledWith(3, { align: 'end', behavior: 'smooth' });
  });

  it('follows again on switching conversations', () => {
    const { result, rerender, scrollToIndex } = setup({ activeConversationId: 'c1', messages: msgs('a'), prefersReducedMotion: false });
    act(() => result.current.navigateToMessage(0));
    rerender({ activeConversationId: 'c2', messages: msgs('x', 'y'), prefersReducedMotion: false });
    expect(scrollToIndex).toHaveBeenLastCalledWith(1, { align: 'end', behavior: 'auto' });
  });

  it('marks only messages that arrived after the conversation opened as fresh', () => {
    const { result, rerender } = setup({ activeConversationId: 'c1', messages: msgs('a', 'b'), prefersReducedMotion: false });
    expect(result.current.freshMessageKeys.size).toBe(0);
    rerender({ activeConversationId: 'c1', messages: msgs('a', 'b', 'c'), prefersReducedMotion: false });
    expect([...result.current.freshMessageKeys]).toEqual(['c']);
    rerender({ activeConversationId: 'c1', messages: msgs('a', 'b', 'c'), prefersReducedMotion: true });
    expect(result.current.freshMessageKeys.size).toBe(0);
    rerender({ activeConversationId: null, messages: msgs('z'), prefersReducedMotion: true });
    expect(result.current.freshMessageKeys.size).toBe(0);
  });

  it('records which message is in view, for which conversation', () => {
    const { result } = setup({ activeConversationId: 'c1', messages: msgs('a', 'b'), prefersReducedMotion: false });
    expect(result.current.visibleMessage).toEqual({ conversationId: null, index: 0 });
    act(() => result.current.handleVisibleMessage(1));
    expect(result.current.visibleMessage).toEqual({ conversationId: 'c1', index: 1 });
  });

  it('does nothing before the thread is mounted', () => {
    const hook = renderHook((props: Props) => useThreadScroll({ ...props, getMessageKey: key }), {
      initialProps: { activeConversationId: 'c1', messages: msgs('a'), prefersReducedMotion: false },
    });
    hook.rerender({ activeConversationId: 'c1', messages: msgs('a', 'b'), prefersReducedMotion: false });
    act(() => hook.result.current.navigateToLatest());
    act(() => hook.result.current.navigateToMessage(0));
    expect(hook.result.current.shouldAutoScrollRef.current).toBe(false);
  });
});
