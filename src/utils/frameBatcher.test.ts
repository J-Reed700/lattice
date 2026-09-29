import { afterEach, describe, expect, it, vi } from 'vitest';

import { createFrameBatcher } from './frameBatcher';

describe('createFrameBatcher', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  it('flushes the final queued update and cancels its fallback timer', () => {
    vi.useFakeTimers();
    let pending = '';
    let rendered = '';
    const batcher = createFrameBatcher(() => {
      rendered += pending;
      pending = '';
    }, 100);
    pending += 'final ';
    batcher.schedule();
    pending += 'answer';
    batcher.schedule();
    batcher.flush();
    batcher.cancel();
    expect(rendered).toBe('final answer');
    vi.advanceTimersByTime(200);
    expect(rendered).toBe('final answer');
  });

  it('cancels a pending frame when its owner unmounts', () => {
    vi.useFakeTimers();
    const update = vi.fn();
    const batcher = createFrameBatcher(update, 100);
    batcher.schedule();
    batcher.cancel();
    vi.advanceTimersByTime(200);
    expect(update).not.toHaveBeenCalled();
  });

  it('uses the timeout when animation frames are unavailable', () => {
    vi.useFakeTimers();
    vi.stubGlobal('requestAnimationFrame', undefined);
    const update = vi.fn();
    const batcher = createFrameBatcher(update, 100);
    batcher.schedule();
    vi.advanceTimersByTime(99);
    expect(update).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(update).toHaveBeenCalledTimes(1);
  });
});
