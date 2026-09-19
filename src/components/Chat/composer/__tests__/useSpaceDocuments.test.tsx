import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useSpaceDocuments } from '../useSpaceDocuments';

import type { SpaceDocument } from '../../../../types';

const listSpaceDocuments = vi.hoisted(() => vi.fn());

vi.mock('../../../../lib/api', () => ({ VaultAPI: { listSpaceDocuments } }));

const halvorsen: SpaceDocument = {
  documentId: 'doc-canopy-meta',
  fileName: 'Halvorsen 2024.pdf',
  category: 'Research Paper',
  modifiedAt: '2026-09-18T00:00:00Z',
};

describe('what `@` is allowed to offer', () => {
  beforeEach(() => {
    listSpaceDocuments.mockReset();
    listSpaceDocuments.mockResolvedValue({ ok: true, data: [halvorsen] });
  });

  // Space isolation: the popup may only ever name documents this chat can read,
  // so it asks the one command that answers from the retrieval scope.
  it('asks for the documents of the space it was given', async () => {
    const { result } = renderHook(() => useSpaceDocuments('space-thesis', 'halv'));

    await waitFor(() => expect(result.current.documents).toEqual([halvorsen]));
    expect(listSpaceDocuments).toHaveBeenCalledWith('space-thesis', 'halv', 8);
  });

  it('asks nothing at all while nothing is being looked up', async () => {
    const { result } = renderHook(() => useSpaceDocuments('space-thesis', null));

    await waitFor(() => expect(result.current.isLoading).toBe(false));
    expect(listSpaceDocuments).not.toHaveBeenCalled();
    expect(result.current.documents).toEqual([]);
  });

  it('offers nothing when the lookup fails, rather than the last answer', async () => {
    const { result, rerender } = renderHook(
      ({ query }: { query: string }) => useSpaceDocuments('space-thesis', query),
      { initialProps: { query: 'halv' } }
    );
    await waitFor(() => expect(result.current.documents).toEqual([halvorsen]));

    listSpaceDocuments.mockResolvedValue({ ok: false, error: 'no such space' });
    rerender({ query: 'transect' });

    await waitFor(() => expect(result.current.documents).toEqual([]));
  });
});
