import { renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';

import { useReaderClosesWithOwner } from '@/features/reading/hooks/useReaderClosesWithOwner';
import { useReaderStore } from '@/features/reading/stores/readerStore';
import type { SourceWithMetadata } from '@/types/conversation';

const source: SourceWithMetadata = {
  documentId: 'doc-1', chunkId: 'chunk-1', fileName: 'Doc', filePath: '/doc', mimeType: 'text/plain',
  category: 'note', content: 'x', score: 1, fileSizeBytes: 1, modifiedAt: '2026-01-01T00:00:00.000Z',
};
const openFor = (owner: string) => useReaderStore.getState().open(owner, [source], 0);
const sessionOwner = () => useReaderStore.getState().session?.ownerKey ?? null;

beforeEach(() => {
  useReaderStore.setState({ session: null });
});

describe('useReaderClosesWithOwner', () => {
  it('releases its own owner session on unmount', () => {
    openFor('owner-a');
    const { unmount } = renderHook(() => useReaderClosesWithOwner('owner-a'));
    expect(sessionOwner()).toBe('owner-a');
    unmount();
    expect(sessionOwner()).toBeNull();
  });

  it('leaves another owner session open on unmount', () => {
    openFor('owner-b');
    const { unmount } = renderHook(() => useReaderClosesWithOwner('owner-a'));
    unmount();
    expect(sessionOwner()).toBe('owner-b');
  });

  it('does nothing for a null owner', () => {
    openFor('owner-a');
    const { unmount } = renderHook(() => useReaderClosesWithOwner(null));
    unmount();
    expect(sessionOwner()).toBe('owner-a');
  });

  it('releases the previous owner when the key changes', () => {
    openFor('owner-a');
    const { rerender } = renderHook(({ owner }) => useReaderClosesWithOwner(owner), { initialProps: { owner: 'owner-a' as string | null } });
    expect(sessionOwner()).toBe('owner-a');
    rerender({ owner: 'owner-b' });
    expect(sessionOwner()).toBeNull();
  });

  it('does not release anything when the key changes to null from an owner without a session', () => {
    openFor('owner-c');
    const { rerender } = renderHook(({ owner }) => useReaderClosesWithOwner(owner), { initialProps: { owner: 'owner-a' as string | null } });
    rerender({ owner: null });
    expect(sessionOwner()).toBe('owner-c');
  });
});
