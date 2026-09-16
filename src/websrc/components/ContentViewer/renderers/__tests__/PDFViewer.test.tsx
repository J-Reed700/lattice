import { render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { PDFViewer } from '../PDFViewer';

const { items, proxy } = vi.hoisted(() => {
  const items = ['Unrelated heading', 'This cited passage', 'continues on another line', 'and must be highlighted.', 'Unrelated footer'];
  return { items, proxy: { numPages: 2, getPage: vi.fn(async () => ({
    getTextContent: async () => ({ items: items.map(str => ({ str })) }),
  })) } };
});
vi.mock('../../../../lib/api', () => ({ default: {
  readFileBytes: async () => ({ ok: true, data: new Uint8Array([1]) }),
} }));
vi.mock('react-pdf', async () => {
  const { useEffect } = await import('react');
  return {
    pdfjs: { GlobalWorkerOptions: {} },
    Document: ({ onLoadSuccess, children }: { onLoadSuccess: (_proxy: typeof proxy) => void; children: React.ReactNode }) => {
      useEffect(() => { onLoadSuccess(proxy); }, [onLoadSuccess]);
      return children;
    },
    Page: ({ customTextRenderer, pageNumber }: { customTextRenderer?: (_item: { str: string; itemIndex: number }) => string; pageNumber: number }) => (
      <div data-testid="pdf-text" data-page={pageNumber}>
        {items.map((str, itemIndex) => <span key={itemIndex} dangerouslySetInnerHTML={{ __html: customTextRenderer?.({ str, itemIndex }) ?? str }} />)}
      </div>
    ),
  };
});

describe('PDF citation navigation', () => {
  it('highlights across lines even with a known page, then clears marks for an unmatched citation', async () => {
    const onMatch = vi.fn();
    const { rerender } = render(<PDFViewer filePath="/test.pdf" highlight={{ text: items.slice(1, 4).join(' '), page: 2 }} onMatch={onMatch} />);
    await waitFor(() => expect(screen.getByTestId('pdf-text').querySelectorAll('mark')).toHaveLength(3));
    expect(screen.getByTestId('pdf-text')).toHaveAttribute('data-page', '2');
    expect(onMatch).toHaveBeenLastCalledWith('exact');
    rerender(<PDFViewer filePath="/test.pdf" highlight={{ text: 'A completely different passage absent from this PDF', page: 1 }} onMatch={onMatch} />);
    await waitFor(() => expect(onMatch).toHaveBeenLastCalledWith('none'));
    expect(screen.getByTestId('pdf-text').querySelectorAll('mark')).toHaveLength(0);
  });
});
