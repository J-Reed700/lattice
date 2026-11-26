import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui';
import { CitationRail, type CitationRailProps } from '@/features/reading/components/CitationRail';
import type { AnchorDto } from '@/lib/bindings';
import type { SourceWithMetadata } from '@/types/conversation';

const TEXT = ['# The Quillet Guide', 'Intro words.', '## Brewing', 'Water boils. Tea is hot and sweet.', '## Storage', 'Keep leaves dry.'].join(
  '\n\n',
);

function anchor(exact: string, base = 0): AnchorDto {
  const at = TEXT.indexOf(exact);
  const bytes = (index: number) => new TextEncoder().encode(TEXT.slice(0, index)).length;
  return { exact, span: { start: base + bytes(at), end: base + bytes(at + exact.length) } };
}

const source: SourceWithMetadata = {
  documentId: 'course:v1',
  chunkId: 'v1#0',
  fileName: 'The Quillet Guide',
  filePath: '',
  mimeType: 'text/html',
  category: 'web',
  content: 'Tea is hot and sweet.',
  score: 1,
  fileSizeBytes: 0,
  modifiedAt: '2026-09-20T00:00:00.000Z',
  citationId: 1,
  webSnapshot: { url: '', title: 'The Quillet Guide', text: TEXT, fetchedAt: null, truncated: false },
  anchoredBy: [
    { occurrence: 0, relation: 'restates', anchors: [anchor('Water boils.'), anchor('Tea is hot and sweet.')] },
    { occurrence: 1, relation: 'quotes', anchors: [anchor('Keep leaves dry.')] },
  ],
};

function renderRail(props: Partial<CitationRailProps> = {}) {
  return render(
    <TooltipProvider>
      <CitationRail
        source={source}
        locator={{ text: 'Tea is hot and sweet.', chunkId: 'v1#0' }}
        occurrence={0}
        onReference={vi.fn()}
        onAddToJournal={vi.fn()}
        onAskAbout={vi.fn()}
        {...props}
      />
    </TooltipProvider>,
  );
}

describe('CitationRail over anchored chips', () => {
  it('shows the anchored sentences of the active chip as its excerpt', () => {
    const view = renderRail();

    expect(view.container.querySelector('blockquote')).toHaveTextContent('Water boils. … Tea is hot and sweet.');
  });

  it('follows the chip that was clicked', () => {
    const view = renderRail({ occurrence: 1 });

    expect(view.container.querySelector('blockquote')).toHaveTextContent('Keep leaves dry.');
    expect(view.container.querySelector('blockquote')).not.toHaveTextContent('Water boils.');
  });

  it('gives the heading trail above the active sentence as its location', () => {
    renderRail();
    expect(screen.getByText('The Quillet Guide › Brewing')).toBeInTheDocument();
  });

  it('gives the trail of the clicked chip, not the first', () => {
    renderRail({ occurrence: 1 });
    expect(screen.getByText('The Quillet Guide › Storage')).toBeInTheDocument();
  });

  it('says nothing about a location it does not know', () => {
    const flat = { ...source, webSnapshot: { ...source.webSnapshot!, text: 'Water boils. Tea is hot and sweet.' } };
    renderRail({ source: flat });

    expect(screen.queryByText('Location unknown')).toBeNull();
  });

  it('keeps the locator text as the excerpt for a source without anchors', () => {
    const plain: SourceWithMetadata = { ...source, anchoredBy: undefined };
    const view = renderRail({ source: plain });

    expect(view.container.querySelector('blockquote')).toHaveTextContent('Tea is hot and sweet.');
    expect(screen.queryByText('Location unknown')).toBeNull();
  });
});
