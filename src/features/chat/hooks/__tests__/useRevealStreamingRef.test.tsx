import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useRevealStreamingRef } from '@/features/chat/hooks/useRevealStreamingRef';

const h = vi.hoisted(() => ({ reveal: vi.fn() }));
vi.mock('@/features/chat/model/folderThreadHost', () => ({ revealInFolder: h.reveal }));

describe('useRevealStreamingRef', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    h.reveal.mockClear();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('shows the newest line reference as it streams, once each', () => {
    const { rerender } = renderHook(({ content, active }) => useRevealStreamingRef(content, active), {
      initialProps: { content: 'See notes/rome.md:12 for', active: true },
    });
    act(() => { vi.advanceTimersByTime(400); });
    expect(h.reveal).toHaveBeenCalledWith('notes/rome.md', { startLine: 12, endLine: 12 });
    act(() => { vi.advanceTimersByTime(400); });
    expect(h.reveal).toHaveBeenCalledTimes(1);
    rerender({ content: 'See notes/rome.md:12 for dates and notes/carthage.md:3-8 for', active: true });
    act(() => { vi.advanceTimersByTime(400); });
    expect(h.reveal).toHaveBeenLastCalledWith('notes/carthage.md', { startLine: 3, endLine: 8 });
  });

  it('waits for a reference at the very end until the answer is done', () => {
    const { rerender } = renderHook(({ content, active }) => useRevealStreamingRef(content, active), {
      initialProps: { content: 'Answer: notes/rome.md:1', active: true },
    });
    act(() => { vi.advanceTimersByTime(400); });
    expect(h.reveal).not.toHaveBeenCalled();
    rerender({ content: 'Answer: notes/rome.md:12', active: true });
    rerender({ content: 'Answer: notes/rome.md:12', active: false });
    expect(h.reveal).toHaveBeenCalledWith('notes/rome.md', { startLine: 12, endLine: 12 });
  });

  it('takes the last tokens when they arrive with the end of streaming', () => {
    const { rerender } = renderHook(({ content, active }) => useRevealStreamingRef(content, active), {
      initialProps: { content: 'Answer: notes/rome.md:1', active: true },
    });
    rerender({ content: 'Answer: notes/rome.md:12', active: false });
    expect(h.reveal).toHaveBeenCalledWith('notes/rome.md', { startLine: 12, endLine: 12 });
  });

  it('does nothing while inactive or without references', () => {
    const { rerender, unmount } = renderHook(({ content, active }) => useRevealStreamingRef(content, active), {
      initialProps: { content: 'notes/rome.md:12 is here', active: false },
    });
    act(() => { vi.advanceTimersByTime(1200); });
    rerender({ content: 'No references at all.', active: true });
    act(() => { vi.advanceTimersByTime(400); });
    unmount();
    expect(h.reveal).not.toHaveBeenCalled();
  });
});
