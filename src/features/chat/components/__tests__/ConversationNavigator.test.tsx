import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ConversationNavigator } from '@/features/chat/components/ConversationNavigator';
import { useChatNavigationStore } from '@/features/chat/stores/chatNavigationStore';
import type { ConversationMessage, OptimisticMessage } from '@/types/conversation';



const messages: ConversationMessage[] = Array.from({ length: 40 }, (_, index) => ({
  id: `m-${index}`, conversationId: 'c1', role: index % 2 === 0 ? 'user' : 'assistant',
  content: `Topic ${index + 1}`, status: 'completed', tokens: 5, createdAt: '2026-10-03',
}));
const getKey = (message: ConversationMessage | OptimisticMessage) => 'id' in message ? message.id : message.tempId;

const ROW_HEIGHT = 60;
const VIEWPORT = 300;
/** @tanstack/virtual-core's default `isScrollingResetDelay`. */
const VIRTUALIZER_SCROLL_END_MS = 150;

/**
 * A list that scrolls as in a browser: `scrollTo` moves it at once, and the scroll event comes with the next frame,
 * ahead of that frame's callbacks. Returns a function that runs that many frames.
 */
function scrollLikeABrowser(rows: number) {
  const pending: FrameRequestCallback[] = [];
  vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => pending.push(callback));
  vi.spyOn(Element.prototype, 'clientHeight', 'get').mockReturnValue(VIEWPORT);
  vi.spyOn(Element.prototype, 'scrollHeight', 'get').mockReturnValue(rows * ROW_HEIGHT);
  HTMLElement.prototype.scrollTo = function (this: HTMLElement, options?: ScrollToOptions | number) {
    const top = typeof options === 'object' ? options.top ?? 0 : 0;
    this.scrollTop = Math.max(0, Math.min(top, rows * ROW_HEIGHT - VIEWPORT));
    window.requestAnimationFrame(() => { this.dispatchEvent(new Event('scroll')); });
  } as HTMLElement['scrollTo'];
  return (frames: number) => {
    for (let frame = 0; frame < frames; frame++) {
      const due = pending.splice(0);
      act(() => { due.forEach((callback) => callback(performance.now())); });
    }
  };
}

describe('ConversationNavigator', () => {
  beforeEach(() => {
    useChatNavigationStore.setState({ expanded: false });
    localStorage.clear();
    vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(300);
    vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(240);
    HTMLElement.prototype.scrollTo = vi.fn();
  });
  // A scroll leaves the virtualizer's 150 ms scroll-end timer running past unmount; it has to fire while jsdom's
  // window is still there, not after the file's last test.
  afterEach(async () => {
    cleanup();
    await new Promise((resolve) => setTimeout(resolve, VIRTUALIZER_SCROLL_END_MS));
    vi.restoreAllMocks();
  });

  it('steps between messages and offers a separate jump to the end', () => {
    const onNavigate = vi.fn();
    const onLatest = vi.fn();
    render(<ConversationNavigator messages={messages} activeIndex={12} getKey={getKey} onNavigate={onNavigate} onLatest={onLatest} />);
    fireEvent.click(screen.getByRole('button', { name: 'Previous message' }));
    expect(onNavigate).toHaveBeenLastCalledWith(11);
    fireEvent.click(screen.getByRole('button', { name: 'Next message' }));
    expect(onNavigate).toHaveBeenLastCalledWith(13);
    fireEvent.click(screen.getByRole('button', { name: 'Jump to latest message' }));
    expect(onLatest).toHaveBeenCalledOnce();
  });

  it('opens clickable previews, marks the current message, and collapses with Escape', async () => {
    const onNavigate = vi.fn();
    const props = { messages, activeIndex: 2, getKey, onNavigate, onLatest: vi.fn() };
    const view = render(<ConversationNavigator {...props} />);
    fireEvent.click(screen.getByRole('button', { name: 'Expand conversation outline' }));
    expect(localStorage.getItem('chat.navigation.expanded')).toBe('true');
    const checkpoint = await screen.findByRole('button', { name: 'Message 3, You: Topic 3' });
    expect(checkpoint).toHaveAttribute('aria-current', 'location');
    fireEvent.click(screen.getByRole('button', { name: 'Message 2, Assistant: Topic 2' }));
    expect(onNavigate).toHaveBeenCalledWith(1);
    view.rerender(<ConversationNavigator {...props} activeIndex={3} />);
    expect(screen.getByRole('button', { name: 'Message 4, Assistant: Topic 4' })).toHaveAttribute('aria-current', 'location');
    checkpoint.focus();
    fireEvent.keyDown(checkpoint, { key: 'Escape' });
    expect(screen.queryByLabelText('Message checkpoints')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Expand conversation outline' })).toHaveFocus();
    expect(localStorage.getItem('chat.navigation.expanded')).toBe('false');
  });

  it('bounds the outline DOM for thousands of messages and reflects deletion', async () => {
    useChatNavigationStore.setState({ expanded: true });
    const many = Array.from({ length: 10_000 }, (_, index) => ({ ...messages[0], id: `m-${index}`, content: `Topic ${index + 1}` }));
    const props = { activeIndex: 0, getKey, onNavigate: vi.fn(), onLatest: vi.fn() };
    const view = render(<ConversationNavigator {...props} messages={many} />);
    await waitFor(() => expect(screen.getByRole('button', { name: 'Message 1, You: Topic 1' })).toHaveAttribute('aria-current', 'location'));
    expect(screen.getAllByRole('button').length).toBeLessThan(20);
    view.rerender(<ConversationNavigator {...props} messages={many.slice(1)} />);
    expect(screen.queryByRole('button', { name: 'Message 1, You: Topic 1' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Message 1, You: Topic 2' })).toHaveAttribute('aria-current', 'location');
    view.rerender(<ConversationNavigator {...props} messages={[]} />);
    expect(screen.queryByRole('navigation')).not.toBeInTheDocument();
  });

  it('leaves a scroll the reader makes right after the outline opens where they put it', () => {
    const runFrames = scrollLikeABrowser(messages.length);
    useChatNavigationStore.setState({ expanded: true });
    render(<ConversationNavigator messages={messages} activeIndex={messages.length - 1} getKey={getKey} onNavigate={vi.fn()} onLatest={vi.fn()} />);
    const list = screen.getByLabelText('Message checkpoints');
    expect(list.scrollTop).toBe(messages.length * ROW_HEIGHT - VIEWPORT);

    list.scrollTop = 11 * ROW_HEIGHT;
    fireEvent.scroll(list);
    runFrames(5);

    expect(list.scrollTop).toBe(11 * ROW_HEIGHT);
    expect(screen.getByRole('button', { name: /^Message 12, / })).toBeInTheDocument();
  });

  it('follows the reading position, and leaves a scroll the reader makes right after it moves', () => {
    const runFrames = scrollLikeABrowser(messages.length);
    useChatNavigationStore.setState({ expanded: true });
    const props = { messages, getKey, onNavigate: vi.fn(), onLatest: vi.fn() };
    const view = render(<ConversationNavigator {...props} activeIndex={0} />);
    const list = screen.getByLabelText('Message checkpoints');
    runFrames(3);
    expect(list.scrollTop).toBe(0);

    view.rerender(<ConversationNavigator {...props} activeIndex={30} />);
    expect(list.scrollTop).toBe(31 * ROW_HEIGHT - VIEWPORT);
    runFrames(3);
    view.rerender(<ConversationNavigator {...props} activeIndex={29} />);
    expect(list.scrollTop).toBe(31 * ROW_HEIGHT - VIEWPORT);
    runFrames(3);
    view.rerender(<ConversationNavigator {...props} activeIndex={10} />);
    expect(list.scrollTop).toBe(10 * ROW_HEIGHT);

    list.scrollTop = 900;
    fireEvent.scroll(list);
    runFrames(5);

    expect(list.scrollTop).toBe(900);
  });
});
