import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CitationSourcesRow } from '@/features/reading/components/CitationSourcesRow';
import type { SourceWithMetadata } from '@/types/conversation';


const source = (n: number): SourceWithMetadata => ({
  documentId: `document-${n}`,
  chunkId: `chunk-${n}`,
  fileName: `Source ${n}.md`,
  filePath: `/vault/source-${n}.md`,
  mimeType: 'text/markdown',
  category: 'note',
  content: `Passage ${n}`,
  score: 0.5,
  fileSizeBytes: 1024,
  modifiedAt: '2026-09-19T00:00:00.000Z',
  citationId: n,
});

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

describe('CitationSourcesRow', () => {
  it('labels the row "Sources" by default and takes another label', () => {
    const { rerender } = render(<CitationSourcesRow entries={[{ number: 1, source: source(1), onView: vi.fn() }]} />);
    expect(screen.getByText('Sources')).toBeInTheDocument();

    rerender(<CitationSourcesRow label="Saved sources" entries={[{ number: 1, source: source(1), onView: vi.fn() }]} />);
    expect(screen.getByText('Saved sources')).toBeInTheDocument();
  });

  it('draws one chip per entry, showing its number', () => {
    render(
      <CitationSourcesRow
        entries={[
          { number: 2, source: source(2), onView: vi.fn() },
          { number: 5, source: source(5), onView: vi.fn() },
        ]}
      />,
    );

    expect(screen.getAllByRole('button')).toHaveLength(2);
    expect(screen.getByRole('button', { name: /Citation 2/ })).toHaveTextContent('2');
    expect(screen.getByRole('button', { name: /Citation 5/ })).toHaveTextContent('5');
  });

  it('shows a title only for an entry that has one', () => {
    render(
      <CitationSourcesRow
        entries={[
          { number: 1, source: source(1), onView: vi.fn(), title: 'Tides explained' },
          { number: 2, source: source(2), onView: vi.fn() },
        ]}
      />,
    );

    expect(screen.getByText('Tides explained')).toBeInTheDocument();
    expect(screen.queryAllByText(/Source \d\.md/)).toHaveLength(0);
  });

  it('opens the entry whose "View source" is clicked', async () => {
    const first = vi.fn();
    const second = vi.fn();
    render(
      <CitationSourcesRow
        entries={[
          { number: 1, source: source(1), onView: first },
          { number: 2, source: source(2), onView: second },
        ]}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: /Citation 2/ }));
    fireEvent.click(await screen.findByRole('button', { name: 'View source' }));

    expect(second).toHaveBeenCalledTimes(1);
    expect(first).not.toHaveBeenCalled();
  });

  it('passes an entry\'s disabled reason to its popover', async () => {
    render(
      <CitationSourcesRow
        entries={[{ number: 1, source: source(1), onView: vi.fn(), disabledReason: 'Opens once submitted' }]}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: /Citation 1/ }));

    expect(await screen.findByText('Opens once submitted')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'View source' })).not.toBeInTheDocument();
  });
});
