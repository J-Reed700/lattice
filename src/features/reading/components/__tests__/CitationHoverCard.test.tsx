import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { CitationHoverCard } from '@/features/reading/components/CitationHoverCard';
import type { SourceWithMetadata } from '@/types/conversation';

const rect = { top: 400, bottom: 420, left: 100, width: 20, height: 20, right: 120, x: 100, y: 400 } as DOMRect;

const source: SourceWithMetadata = {
  documentId: 'course:v1',
  chunkId: 'v1#0',
  fileName: 'The Quillet Guide',
  filePath: '',
  mimeType: 'text/html',
  category: 'web',
  content: 'A saved passage about tea and kettles.',
  score: 1,
  fileSizeBytes: 0,
  modifiedAt: '2026-09-20T00:00:00.000Z',
  citationId: 2,
  anchoredBy: [
    { occurrence: 0, relation: 'restates', anchors: [{ exact: 'Tea is hot.' }, { exact: 'Kettles boil.' }] },
    { occurrence: 2, relation: 'quotes', anchors: [{ exact: 'Leaves stay dry.' }] },
  ],
};

describe('CitationHoverCard', () => {
  it('says how the hovered chip uses the source and shows its sentences', () => {
    render(<CitationHoverCard hover={{ number: 2, occurrence: 0, rect }} source={source} />);

    expect(screen.getByText('Paraphrased from')).toBeInTheDocument();
    expect(screen.getByRole('tooltip')).toHaveTextContent('Tea is hot.…Kettles boil.');
    expect(screen.queryByText('A saved passage about tea and kettles.')).toBeNull();
  });

  it('picks the entry by the chip occurrence, not by position', () => {
    render(<CitationHoverCard hover={{ number: 2, occurrence: 2, rect }} source={source} />);

    expect(screen.getByText('Quoted from')).toBeInTheDocument();
    expect(screen.getByText('“Leaves stay dry.”')).toBeInTheDocument();
  });

  it('includes the heading trail above an anchored excerpt in web snapshot text', () => {
    const withHeadings: SourceWithMetadata = {
      ...source,
      webSnapshot: {
        url: 'https://example.org/guide',
        title: 'The Quillet Guide',
        text: '# The Quillet Guide\n\n## Brewing\n\nTea is hot.\n\n## Storage\n\nLeaves stay dry.',
        fetchedAt: null,
        truncated: false,
      },
    };
    render(<CitationHoverCard hover={{ number: 2, occurrence: 2, rect }} source={withHeadings} />);

    expect(screen.getByText('The Quillet Guide › Storage')).toBeInTheDocument();
  });
});
