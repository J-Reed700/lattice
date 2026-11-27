import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CitationFootnote } from '@/features/reading/components/CitationFootnote';
import type { SourceWithMetadata } from '@/types/conversation';


const source: SourceWithMetadata = {
  documentId: 'document-1',
  chunkId: 'chunk-1',
  fileName: 'Source 1.md',
  filePath: '/vault/source-1.md',
  mimeType: 'text/markdown',
  category: 'note',
  content: 'Passage one',
  score: 0.5,
  fileSizeBytes: 1024,
  modifiedAt: '2026-09-19T00:00:00.000Z',
  citationId: 1,
};

beforeEach(() => {
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    }
  );
});

afterEach(() => {
  vi.unstubAllGlobals();
});

const openPopover = () => fireEvent.click(screen.getByRole('button', { name: /Citation 1/ }));

describe('CitationFootnote', () => {
  it('shows the reason and no way to open when the source cannot be opened', async () => {
    render(<CitationFootnote number={1} source={source} onViewFile={vi.fn()} disabledReason="Not available offline" />);
    openPopover();

    expect(await screen.findByText('Not available offline')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'View source' })).not.toBeInTheDocument();
  });

  it('opens the source and closes the popover when "View source" is clicked', async () => {
    const onViewFile = vi.fn();
    render(<CitationFootnote number={1} source={source} onViewFile={onViewFile} />);
    openPopover();

    fireEvent.click(await screen.findByRole('button', { name: 'View source' }));

    expect(onViewFile).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(screen.queryByRole('button', { name: 'View source' })).not.toBeInTheDocument());
  });

  it('returns focus to the chip when the popover is closed with Escape', async () => {
    render(<CitationFootnote number={1} source={source} onViewFile={vi.fn()} />);
    const chip = screen.getByRole('button', { name: /Citation 1/ });
    fireEvent.click(chip);
    const viewSource = await screen.findByRole('button', { name: 'View source' });
    viewSource.focus();
    expect(chip).not.toHaveFocus();

    fireEvent.keyDown(viewSource, { key: 'Escape' });
    await waitFor(() => expect(screen.queryByRole('button', { name: 'View source' })).not.toBeInTheDocument());
    await waitFor(() => expect(chip).toHaveFocus());
  });

  it('returns focus to the chip on Escape after reopening following "View source"', async () => {
    render(<CitationFootnote number={1} source={source} onViewFile={vi.fn()} />);
    const chip = screen.getByRole('button', { name: /Citation 1/ });
    fireEvent.click(chip);
    fireEvent.click(await screen.findByRole('button', { name: 'View source' }));
    await waitFor(() => expect(screen.queryByRole('button', { name: 'View source' })).not.toBeInTheDocument());

    fireEvent.click(chip);
    const reopened = await screen.findByRole('button', { name: 'View source' });
    reopened.focus();
    expect(chip).not.toHaveFocus();
    fireEvent.keyDown(reopened, { key: 'Escape' });
    await waitFor(() => expect(screen.queryByRole('button', { name: 'View source' })).not.toBeInTheDocument());
    await waitFor(() => expect(chip).toHaveFocus());
  });

  it('renders CitationCard relation, curly-quoted excerpt, heading trail, and accessible name inside popover', async () => {
    const anchored: SourceWithMetadata = {
      ...source,
      webSnapshot: {
        url: 'https://example.org/source-1',
        title: 'Source 1.md',
        text: '# Manual\n\n## Rules\n\nExact quoted sentence from manual.',
        fetchedAt: null,
        truncated: false,
      },
      anchoredBy: [
        { occurrence: 0, relation: 'quotes', anchors: [{ exact: 'Exact quoted sentence from manual.' }] },
      ],
    };
    render(<CitationFootnote number={1} source={anchored} onViewFile={vi.fn()} />);
    openPopover();

    const group = await screen.findByRole('group', { name: 'Source 1, quoted from Source 1.md' });
    expect(group).toHaveTextContent('Quoted from');
    expect(group).toHaveTextContent('“Exact quoted sentence from manual.”');
    expect(group).toHaveTextContent('Manual › Rules');
  });

  it('renders relation, excerpts, and heading trail via anchoredCardDetails for whole-source and named occurrences', async () => {
    const text = '# Guide\n\n## Methods\n\nFirst passage here.\n\n## Results\n\nSecond passage here.';
    const spanFor = (exact: string) => {
      const start = new TextEncoder().encode(text.slice(0, text.indexOf(exact))).length;
      return { start, end: start + new TextEncoder().encode(exact).length };
    };
    const anchored: SourceWithMetadata = {
      ...source,
      fileName: 'Guide.md',
      content: text,
      anchoredBy: [
        { occurrence: 0, relation: 'quotes', anchors: [{ exact: 'First passage here.', span: spanFor('First passage here.') }] },
        { occurrence: 1, relation: 'cites', anchors: [{ exact: 'Second passage here.', span: spanFor('Second passage here.') }] },
      ],
      anchorBase: 0,
    };

    const view = render(<CitationFootnote number={1} source={anchored} onViewFile={vi.fn()} />);
    openPopover();
    const firstCard = await screen.findByRole('group', { name: 'Source 1, quoted from Guide.md' });
    expect(firstCard).toHaveTextContent('Quoted from');
    expect(firstCard).toHaveTextContent('“First passage here.”');
    expect(firstCard).toHaveTextContent('Guide › Methods');

    view.rerender(<CitationFootnote number={1} source={anchored} occurrence={1} onViewFile={vi.fn()} />);
    const secondCard = await screen.findByRole('group', { name: 'Source 1, cited (not checked) in Guide.md' });
    expect(secondCard).toHaveTextContent('Cited (not checked)');
    expect(secondCard).toHaveTextContent('Second passage here.');
    expect(secondCard).toHaveTextContent('Guide › Results');
  });
});
