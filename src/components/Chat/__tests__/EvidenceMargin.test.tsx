import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { EvidenceMargin } from '../EvidenceMargin';

import type { ClaimVerdict, SourceWithMetadata } from '../../../types/conversation';

const source = (citationId: number, fileName: string, excerpt: string, pageNumber?: number) =>
  ({
    documentId: `doc-${fileName}`,
    chunkId: `chunk-${citationId}`,
    fileName,
    filePath: `/vault/${fileName}`,
    content: excerpt,
    excerpt,
    score: 0.9,
    citationId,
    ...(pageNumber ? { pageNumber } : {}),
  }) as unknown as SourceWithMetadata;

const verdict = (citationIds: number[], result: ClaimVerdict['verdict']): ClaimVerdict => ({
  sentence: 'A checked sentence of the answer.',
  citationIds,
  verdict: result,
  method: 'judge',
});

function renderMargin(overrides: Partial<Parameters<typeof EvidenceMargin>[0]> = {}) {
  const props = {
    citationMap: new Map([
      [2, source(2, 'Transect.md', 'Shaded segment averaged 31.4 °C.')],
      [1, source(1, 'Halvorsen.pdf', 'The pooled estimate was 1.2 °C.', 14)],
    ]),
    activeNumbers: [],
    onOpen: vi.fn(),
    onHoverNumber: vi.fn(),
    ...overrides,
  };
  render(<EvidenceMargin {...props} />);
  return props;
}

describe('EvidenceMargin', () => {
  it('sets out each cited passage in citation order, with where it is from', () => {
    renderMargin();

    const notes = screen.getAllByRole('button');
    expect(notes.map((note) => note.getAttribute('data-note'))).toEqual(['1', '2']);
    expect(within(notes[0]!).getByText('PDF p. 14')).toBeInTheDocument();
    expect(within(notes[0]!).getByText('The pooled estimate was 1.2 °C.')).toBeInTheDocument();
    expect(screen.getByText(/2 passages · 2 documents/)).toBeInTheDocument();
  });

  it('says what the checked sentences citing a passage came to', () => {
    renderMargin({
      claimVerdicts: [verdict([1], 'supported'), verdict([1], 'supported'), verdict([2], 'unsupported'), verdict([2], 'contradicted')],
    });

    expect(screen.getByText('backs 2 claims')).toBeInTheDocument();
    expect(screen.getByText('contradicts 1 · 1 not found here')).toBeInTheDocument();
  });

  it('lights the note for the citation under the pointer', () => {
    renderMargin({ activeNumbers: [2] });

    expect(screen.getByRole('button', { name: /Open passage 2/ })).toHaveAttribute('data-active', 'true');
    expect(screen.getByRole('button', { name: /Open passage 1/ })).not.toHaveAttribute('data-active');
  });

  it('opens a passage and reports the note under the pointer', async () => {
    const props = renderMargin();
    const note = screen.getByRole('button', { name: /Open passage 1/ });

    await userEvent.hover(note);
    expect(props.onHoverNumber).toHaveBeenLastCalledWith(1);
    await userEvent.click(note);
    expect(props.onOpen).toHaveBeenCalledWith(1);
  });

  it('offers to compare the documents the notes span', async () => {
    const onCompareDocuments = vi.fn();
    renderMargin({ onCompareDocuments });

    await userEvent.click(screen.getByRole('button', { name: 'Compare these 2 documents' }));
    expect(onCompareDocuments).toHaveBeenCalledWith(['doc-Halvorsen.pdf', 'doc-Transect.md']);
  });

  it('offers no comparison when every note comes from one document', () => {
    renderMargin({
      citationMap: new Map([
        [1, source(1, 'Halvorsen.pdf', 'The pooled estimate was 1.2 °C.', 14)],
        [2, source(2, 'Halvorsen.pdf', 'And a second passage from the same file.')],
      ]),
      onCompareDocuments: vi.fn(),
    });

    expect(screen.queryByText(/^Compare these/)).not.toBeInTheDocument();
  });

  it('does not count a cited web page as a document Compare could read', () => {
    renderMargin({
      citationMap: new Map([
        [1, source(1, 'Halvorsen.pdf', 'The pooled estimate was 1.2 °C.', 14)],
        [2, { ...source(2, 'A web piece', 'Something a page said.'), documentId: 'web:https://example.org/piece' }],
      ]),
      onCompareDocuments: vi.fn(),
    });

    expect(screen.queryByText(/^Compare these/)).not.toBeInTheDocument();
  });

  it('draws nothing for an answer that cites nothing', () => {
    const { container } = render(
      <EvidenceMargin citationMap={new Map()} activeNumbers={[]} onOpen={vi.fn()} onHoverNumber={vi.fn()} />
    );

    expect(container).toBeEmptyDOMElement();
  });
});
