import type { MouseEvent as ReactMouseEvent } from 'react';

import { act, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';

import { useCitationInteractions } from '@/features/reading/hooks/useCitationInteractions';
import type { SourceWithMetadata } from '@/types/conversation';

const source: SourceWithMetadata = {
  documentId: 'course:v1',
  chunkId: 'v1#0',
  fileName: 'The Quillet Guide',
  filePath: '',
  mimeType: 'text/html',
  category: 'web',
  content: 'A saved passage.',
  score: 1,
  fileSizeBytes: 0,
  modifiedAt: '2026-09-20T00:00:00.000Z',
  citationId: 2,
};

function chipAt(occurrence: number): HTMLElement {
  const chip = document.createElement('span');
  chip.dataset.cite = '2';
  chip.dataset.citeAt = String(occurrence);
  document.body.append(chip);
  return chip;
}

const over = (target: HTMLElement) =>
  ({ target, currentTarget: document.body, clientY: 0 }) as unknown as ReactMouseEvent<HTMLElement>;

afterEach(() => {
  document.body.replaceChildren();
});

describe('useCitationInteractions hover', () => {
  it('carries the occurrence of the chip under the pointer', () => {
    const { result } = renderHook(() =>
      useCitationInteractions({ sources: [source], ownerKey: 'answer', numbered: true, showCitations: true, claims: [] }),
    );

    act(() => result.current.handleBodyMouseOver(over(chipAt(1))));

    expect(result.current.citationHover).toMatchObject({ number: 2, occurrence: 1 });
  });

  it('moves to another chip of the same source when the pointer does', () => {
    const { result } = renderHook(() =>
      useCitationInteractions({ sources: [source], ownerKey: 'answer', numbered: true, showCitations: true, claims: [] }),
    );

    act(() => result.current.handleBodyMouseOver(over(chipAt(0))));
    act(() => result.current.handleBodyMouseOver(over(chipAt(3))));

    expect(result.current.citationHover).toMatchObject({ number: 2, occurrence: 3 });
  });
});
