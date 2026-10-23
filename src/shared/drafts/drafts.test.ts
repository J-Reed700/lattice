import { act, renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { clearDraft, readDraft, setDraft, useDraft } from '@/shared/drafts/drafts';

describe('drafts', () => {
  it('are kept for a restart and dropped from storage once empty', () => {
    setDraft('conversation:a', 'unsent');
    expect(JSON.parse(localStorage.getItem('lattice.drafts') ?? '{}')).toEqual({ 'conversation:a': 'unsent' });

    clearDraft('conversation:a');
    expect(readDraft('conversation:a')).toBe('');
    expect(localStorage.getItem('lattice.drafts')).toBeNull();
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
