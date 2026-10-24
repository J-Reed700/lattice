import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useTextSelection } from '../useTextSelection';

let reader: HTMLDivElement;
let elsewhere: HTMLDivElement;
let frames: Map<number, FrameRequestCallback>;
let nextFrame: number;

function select(node: Node, start: number, end: number) {
  const range = document.createRange();
  range.setStart(node, start);
  range.setEnd(node, end);
  window.getSelection()!.removeAllRanges();
  window.getSelection()!.addRange(range);
  document.dispatchEvent(new Event('selectionchange'));
}

function flushFrames() {
  act(() => {
    const pending = [...frames.values()];
    frames.clear();
    for (const callback of pending) callback(0);
  });
}

beforeEach(() => {
  reader = document.createElement('div');
  reader.textContent = 'The Treaty of Westphalia ended the Thirty Years War.';
  elsewhere = document.createElement('div');
  elsewhere.textContent = 'Sidebar';
  document.body.append(reader, elsewhere);
  frames = new Map();
  nextFrame = 1;
  vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => { frames.set(nextFrame, callback); return nextFrame++; });
  vi.spyOn(window, 'cancelAnimationFrame').mockImplementation((id) => { frames.delete(id); });
  vi.spyOn(Range.prototype, 'getBoundingClientRect').mockReturnValue(new DOMRect(10, 20, 80, 16));
});
afterEach(() => {
  window.getSelection()?.removeAllRanges();
  reader.remove();
  elsewhere.remove();
  vi.restoreAllMocks();
});

describe('useTextSelection', () => {
  it('reports the selected text and its rect once per frame', () => {
    const { result } = renderHook(() => useTextSelection({ current: reader }));
    expect(result.current).toEqual({ text: '', rect: null });
    select(reader.firstChild!, 4, 24);
    select(reader.firstChild!, 4, 30);
    expect(frames.size).toBe(1);
    flushFrames();
    expect(result.current.text).toBe('Treaty of Westphalia ended');
    expect(result.current.rect).toEqual(new DOMRect(10, 20, 80, 16));
  });

  it('clears on a collapsed selection, one outside the reader, or one with no size', () => {
    const { result } = renderHook(() => useTextSelection({ current: reader }));
    const expectCleared = (arrange: () => void) => {
      select(reader.firstChild!, 4, 10);
      flushFrames();
      expect(result.current.text).toBe('Treaty');
      arrange();
      flushFrames();
      expect(result.current).toEqual({ text: '', rect: null });
    };
    expectCleared(() => select(reader.firstChild!, 4, 4));
    expectCleared(() => select(elsewhere.firstChild!, 0, 7));
    expectCleared(() => {
      vi.mocked(Range.prototype.getBoundingClientRect).mockReturnValueOnce(new DOMRect(0, 0, 0, 0));
      select(reader.firstChild!, 4, 10);
    });
    expectCleared(() => select(reader.firstChild!, 3, 4));
  });

  it('keeps an empty state as the same object when nothing was selected', () => {
    const { result } = renderHook(() => useTextSelection({ current: reader }));
    const before = result.current;
    select(reader.firstChild!, 2, 2);
    flushFrames();
    expect(result.current).toBe(before);
  });

  it('clears on any scroll and drops a pending read', () => {
    const { result } = renderHook(() => useTextSelection({ current: reader }));
    select(reader.firstChild!, 4, 10);
    flushFrames();
    select(reader.firstChild!, 4, 20);
    expect(frames.size).toBe(1);
    act(() => { reader.dispatchEvent(new Event('scroll')); });
    expect(frames.size).toBe(0);
    expect(result.current).toEqual({ text: '', rect: null });
    act(() => { document.dispatchEvent(new Event('scroll')); });
    expect(result.current).toEqual({ text: '', rect: null });
  });

  it('reports nothing without a reader element', () => {
    const { result } = renderHook(() => useTextSelection({ current: null }));
    select(reader.firstChild!, 4, 10);
    flushFrames();
    expect(result.current).toEqual({ text: '', rect: null });
  });

  it('stops listening and cancels a pending frame on unmount', () => {
    const { unmount } = renderHook(() => useTextSelection({ current: reader }));
    select(reader.firstChild!, 4, 10);
    expect(frames.size).toBe(1);
    unmount();
    expect(frames.size).toBe(0);
    select(reader.firstChild!, 4, 12);
    expect(frames.size).toBe(0);
  });
});
