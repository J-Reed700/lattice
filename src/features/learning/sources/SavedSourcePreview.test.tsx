import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { SavedSourcePreview } from '@/features/learning/sources/SavedSourcePreview';

describe('SavedSourcePreview', () => {
  it('draws the opening as formatted text and keeps the capture exact', () => {
    const fullText = '# Title\n\nIntro **paragraph** with `code`.\n\n```ts\nconst x = 1;\n```\n\n## Next\n\nMore.';
    const { container } = render(<SavedSourcePreview fullText={fullText} extractionVersion="reader-1" wordCount={10} />);

    expect(screen.getByRole('heading', { name: 'Title' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Next' })).toBeInTheDocument();
    expect(screen.getByText('paragraph').tagName).toBe('STRONG');
    expect(screen.getByText('code').tagName).toBe('CODE');
    expect(container.querySelector('pre > code')?.textContent).toBe('const x = 1;');
    const section = screen.getByRole('region', { name: 'Saved source' });
    const opening = section.querySelector('details')!.previousElementSibling!;
    expect(opening.textContent).not.toMatch(/#|\*\*|```/);
    expect(container.querySelector('details pre')?.textContent).toBe(fullText);
  });

  it('draws tables and lists in the opening, and keeps markup-like text as written', () => {
    const fullText = '| A | B |\n|---|---|\n| 1 | 2 |\n\n- one\n- two\n\nSee target/<triple>/debug/.';
    render(<SavedSourcePreview fullText={fullText} extractionVersion="reader-1" wordCount={10} />);

    expect(screen.getByRole('table')).toBeInTheDocument();
    expect(screen.getAllByRole('listitem')).toHaveLength(2);
    expect(screen.getByText('See target/<triple>/debug/.')).toBeInTheDocument();
  });

  it('shows whole blocks until about 900 characters, and fades when blocks were left out', () => {
    const paragraphs = Array.from({ length: 5 }, (_, index) => `${String(index)}${'a'.repeat(399)}`);
    const fullText = paragraphs.join('\n\n');
    const { container } = render(<SavedSourcePreview fullText={fullText} extractionVersion="reader-1" wordCount={1} />);

    // 400 + 400 is under 900, so a third block is shown; the fourth is not.
    expect(container.querySelectorAll('p.font-serif')).toHaveLength(3);
    expect(container.querySelector('[aria-hidden="true"].bg-linear-to-t')).not.toBeNull();
    expect(container.querySelector('details pre')?.textContent).toBe(fullText);
  });

  it('fades when the shown blocks are taller than the box, even if none were left out', () => {
    const scroll = vi.spyOn(HTMLElement.prototype, 'scrollHeight', 'get').mockReturnValue(500);
    const client = vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(288);
    try {
      const { container } = render(<SavedSourcePreview fullText={'a'.repeat(500)} extractionVersion="reader-1" wordCount={1} />);
      expect(container.querySelector('.bg-linear-to-t')).not.toBeNull();
    } finally {
      scroll.mockRestore();
      client.mockRestore();
    }
  });

  it('shows one long block whole, and no fade when everything fits', () => {
    const long = 'a'.repeat(1200);
    const { container, rerender } = render(<SavedSourcePreview fullText={long} extractionVersion="reader-1" wordCount={1} />);
    expect(container.querySelector('p.font-serif')?.textContent).toBe(long);
    expect(container.querySelector('.bg-linear-to-t')).toBeNull();

    rerender(<SavedSourcePreview fullText={'b'.repeat(900)} extractionVersion="reader-1" wordCount={1} />);
    expect(container.querySelector('p.font-serif')?.textContent).toBe('b'.repeat(900));
    expect(container.querySelector('.bg-linear-to-t')).toBeNull();
  });

  it('shows the word count and reading time at 225 words per minute', () => {
    const { rerender } = render(<SavedSourcePreview fullText="text" extractionVersion="reader-1" wordCount={1000} />);
    expect(screen.getByText('1,000 words · about 5 min')).toBeInTheDocument();
    rerender(<SavedSourcePreview fullText="text" extractionVersion="reader-1" wordCount={226} />);
    expect(screen.getByText('226 words · about 2 min')).toBeInTheDocument();
    rerender(<SavedSourcePreview fullText="text" extractionVersion="reader-1" wordCount={225} />);
    expect(screen.getByText('225 words · about 1 min')).toBeInTheDocument();
  });

  it('shows at least one minute', () => {
    render(<SavedSourcePreview fullText="text" extractionVersion="reader-1" wordCount={0} />);
    expect(screen.getByText('0 words · about 1 min')).toBeInTheDocument();
  });

  it('holds the raw text in an Exact capture details element', () => {
    const { container } = render(<SavedSourcePreview fullText={'# Raw\n\n\n\nkept'} extractionVersion="reader-1" wordCount={2} />);
    expect(container.querySelector('details summary')).toHaveTextContent('Exact capture');
    expect(container.querySelector('details pre')?.textContent).toBe('# Raw\n\n\n\nkept');
  });

  it('says so when the saved version has no text', () => {
    render(<SavedSourcePreview fullText="   " extractionVersion="reader-1" wordCount={0} />);
    expect(screen.getByText('This saved version has no text.')).toBeInTheDocument();
  });
});
