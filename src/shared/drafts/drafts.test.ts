import { act, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { flushPendingSaves } from '@/lib/pendingSaves';
import { clearDraft, flushDrafts, readDraft, setDraft, useDraft } from '@/shared/drafts/drafts';

describe('drafts', () => {
  afterEach(() => {
    flushDrafts();
    vi.useRealTimers();
  });

  it('debounces storage writes and clears immediately when emptied', () => {
    vi.useFakeTimers();
    setDraft('conversation:a', 'unsent');
    expect(readDraft('conversation:a')).toBe('unsent');
    expect(localStorage.getItem('lattice.drafts')).toBeNull();

    vi.advanceTimersByTime(250);
    expect(JSON.parse(localStorage.getItem('lattice.drafts') ?? '{}')).toEqual({ 'conversation:a': 'unsent' });

    clearDraft('conversation:a');
    expect(readDraft('conversation:a')).toBe('');
    expect(localStorage.getItem('lattice.drafts')).toBeNull();
  });

  it('flushes pending writes on pagehide and flushPendingSaves', async () => {
    setDraft('conversation:b', 'before hide');
    expect(localStorage.getItem('lattice.drafts')).toBeNull();
    window.dispatchEvent(new Event('pagehide'));
    expect(JSON.parse(localStorage.getItem('lattice.drafts') ?? '{}')).toEqual({ 'conversation:b': 'before hide' });

    setDraft('conversation:c', 'before quit');
    expect(JSON.parse(localStorage.getItem('lattice.drafts') ?? '{}')).toEqual({ 'conversation:b': 'before hide' });
    expect(await flushPendingSaves()).toBe(true);
    expect(JSON.parse(localStorage.getItem('lattice.drafts') ?? '{}')).toEqual({
      'conversation:b': 'before hide',
      'conversation:c': 'before quit',
    });
  });

  it('follow the key, not the component', () => {
    const { result, rerender, unmount } = renderHook(({ key }) => useDraft(key), { initialProps: { key: 'a' } });
    act(() => result.current[1]('first'));
    rerender({ key: 'b' });
    expect(result.current[0]).toBe('');
    unmount();

    expect(renderHook(() => useDraft('a')).result.current[0]).toBe('first');
  });

  it('stay in the component when there is nowhere to file them', () => {
    const { result } = renderHook(() => useDraft(null));
    act(() => result.current[1]('typed before a chat exists'));

    expect(result.current[0]).toBe('typed before a chat exists');
    expect(localStorage.getItem('lattice.drafts')).toBeNull();
  });
});
