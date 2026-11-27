import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { SourceCitations } from '@/features/reading/components/SourceCitations';
import type { SourceWithMetadata } from '@/types/conversation';

const source = (overrides: Partial<SourceWithMetadata>): SourceWithMetadata => ({
  documentId: 'doc-1',
  chunkId: 'chunk-1',
  fileName: 'paper.pdf',
  filePath: '/vault/paper.pdf',
  mimeType: 'application/pdf',
  category: 'PDF',
  content: 'The sample size was 42 participants.',
  excerpt: 'The sample size was 42 participants.',
  score: 0.83,
  fileSizeBytes: 1024,
  modifiedAt: '2026-09-01T00:00:00Z',
  ...overrides,
});

const expand = async () => {
  await userEvent.click(screen.getByRole('button', { expanded: false }));
};

describe('SourceCitations', () => {
  it('numbers a web result from one, not zero', async () => {
    render(
      <SourceCitations
        sources={[
          source({
            documentId: 'web:https://example.com/a',
            chunkId: 'web-chunk-1',
            fileName: 'example.com',
            category: 'Web article',
            chunkIndex: 0,
          }),
        ]}
        onViewSource={vi.fn()}
      />
    );
    await expand();

    expect(screen.getByText(/Rank 1/)).toBeInTheDocument();
    expect(screen.queryByText(/Rank #?0/)).not.toBeInTheDocument();
  });

  it('says nothing about a retrieval score', async () => {
    // Normalising against the best hit made the top source "100% relevance"
    // every single time — a percentage of nothing a reader can name.
    render(
      <SourceCitations
        sources={[
          source({ chunkId: 'chunk-1', score: 0.9 }),
          source({
            documentId: 'doc-2',
            chunkId: 'chunk-2',
            fileName: 'notes.md',
            score: 0.2,
          }),
        ]}
        onViewSource={vi.fn()}
      />
    );
    await expand();

    expect(screen.queryByText(/relevance/)).not.toBeInTheDocument();
    expect(screen.queryByText(/100%/)).not.toBeInTheDocument();
    expect(screen.queryByText(/No score/)).not.toBeInTheDocument();
    // The things a reader can actually use are still there.
    expect(screen.getByText('paper.pdf')).toBeInTheDocument();
    expect(screen.getByText('notes.md')).toBeInTheDocument();
  });

  it('treats non-positive citationId as unnumbered rather than rendering Citation 0', async () => {
    render(
      <SourceCitations
        sources={[source({ citationId: 0, fileName: 'legacy.md' })]}
        onViewSource={vi.fn()}
      />,
    );
    await expand();

    expect(screen.queryByRole('group', { name: /Source 0/ })).not.toBeInTheDocument();
    expect(screen.getByText('legacy.md')).toBeInTheDocument();
  });

  it('renders CitationCard with relation, excerpts, and heading trail for anchored sources', async () => {
    const text = '# Guide\n\n## Methods\n\nThe sample size was 42 participants.';
    const exact = 'The sample size was 42 participants.';
    const start = new TextEncoder().encode(text.slice(0, text.indexOf(exact))).length;
    const end = start + new TextEncoder().encode(exact).length;
    render(
      <SourceCitations
        sources={[
          source({
            citationId: 4,
            fileName: 'Guide.md',
            content: text,
            excerpt: exact,
            anchoredBy: [
              {
                occurrence: 0,
                relation: 'quotes',
                anchors: [{ exact, span: { start, end } }],
              },
            ],
            anchorBase: 0,
          }),
        ]}
        onViewSource={vi.fn()}
      />,
    );
    await expand();

    const card = screen.getByRole('group', { name: 'Source 4, quoted from Guide.md' });
    expect(card).toHaveTextContent('Quoted from');
    expect(card).toHaveTextContent('“The sample size was 42 participants.”');
    expect(card).toHaveTextContent('Guide › Methods');
  });
});

it('opens the selected passage within a shared PDF without marking search keywords', async () => {
  const first = source({ citationId: 1, highlights: ['patent'] });
  const second = source({ citationId: 2, chunkId: 'chunk-2', excerpt: 'A patent application must include a written description.', highlights: ['patent'] });
  const onViewSource = vi.fn();
  const { container } = render(<SourceCitations sources={[first, second]} onViewSource={onViewSource} />);
  await expand();
  await userEvent.click(screen.getByRole('button', { name: 'View passage [2]' }));
  expect(onViewSource).toHaveBeenCalledWith(second);
  expect(container.querySelectorAll('mark')).toHaveLength(0);
});
