import { fireEvent, renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { useCitationTravel } from '@/features/reading/hooks/useCitationTravel';
import type { SourceWithMetadata } from '@/types/conversation';

const cited = (id: number, anchored: boolean): SourceWithMetadata => ({
  documentId: `course:${id}`,
  chunkId: `${id}#0`,
  fileName: `Source ${id}`,
  filePath: '',
  mimeType: 'text/html',
  category: 'web',
  content: '',
  score: 1,
  fileSizeBytes: 0,
  modifiedAt: '2026-09-20T00:00:00.000Z',
  citationId: id,
  anchoredBy: anchored ? [{ occurrence: 0, relation: 'restates', anchors: [] }] : [],
});

describe('useCitationTravel over anchored sources', () => {
  const sources = [cited(1, true), cited(2, false), cited(3, true), cited(4, true)];

  it('steps over the sources that have a chip with support', () => {
    const change = vi.fn();
    const { result } = renderHook(() => useCitationTravel(sources, 0, change));

    result.current.stepCitation(1);
    expect(change).toHaveBeenLastCalledWith(2);
    result.current.stepCitation(-1);
    expect(change).toHaveBeenLastCalledWith(0);
  });

  it('answers the "[" and "]" keys the same way', () => {
    const change = vi.fn();
    renderHook(() => useCitationTravel(sources, 2, change));

    fireEvent.keyDown(document, { key: ']' });
    expect(change).toHaveBeenLastCalledWith(3);
    fireEvent.keyDown(document, { key: '[' });
    expect(change).toHaveBeenLastCalledWith(0);
  });

  it('counts its position among the sources it steps over', () => {
    const { result } = renderHook(() => useCitationTravel(sources, 2, vi.fn()));

    expect(result.current.position).toBe(1);
    expect(result.current.total).toBe(3);
  });

  it('cannot travel when only one source has support', () => {
    const { result } = renderHook(() => useCitationTravel([cited(1, true), cited(2, false)], 0, vi.fn()));

    expect(result.current.canTravel).toBe(false);
  });

  it('leaves sources without any anchoring to be stepped over one by one', () => {
    const plain = [1, 2, 3].map((id) => ({ ...cited(id, false), anchoredBy: undefined }));
    const change = vi.fn();
    const { result } = renderHook(() => useCitationTravel(plain, 0, change));

    result.current.stepCitation(1);
    expect(change).toHaveBeenLastCalledWith(1);
    expect(result.current.total).toBe(3);
  });
});
