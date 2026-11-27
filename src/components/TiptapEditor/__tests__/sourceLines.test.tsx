import { render, waitFor } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { TiptapViewer } from '../TiptapViewer';

const sampleMarkdown = [
  '# Quillet Heading',
  '',
  'First paragraph.',
  '',
  'Second paragraph.',
  '',
  '1. Item one',
  '2. Item two',
  '3. Item three',
  '',
  '> Quoted block',
  '',
  '```ts',
  'const count = 1;',
  '```',
  '',
  '| Term | Note |',
  '| --- | --- |',
  '| Quillet | Example |',
].join('\n');

describe('SourceLines', () => {
  it('annotates rendered blocks and list items only when sourceLines is enabled', async () => {
    const plain = render(<TiptapViewer content={sampleMarkdown} />);
    await waitFor(() => expect(plain.container).toHaveTextContent('Quillet Heading'));
    expect(plain.container.querySelector('[data-source-lines]')).toBeNull();

    const annotated = render(<TiptapViewer content={sampleMarkdown} sourceLines />);
    await waitFor(() => expect(annotated.container.querySelector('h1')).not.toBeNull());

    expect(annotated.container.querySelector('h1')).toHaveAttribute('data-source-lines', '0-1');

    const topParagraphs = Array.from(annotated.container.querySelectorAll('.ProseMirror > p'));
    expect(topParagraphs.map((el) => el.getAttribute('data-source-lines'))).toEqual(['2-3', '4-5']);

    const listItems = Array.from(annotated.container.querySelectorAll('ol > li'));
    expect(listItems.map((el) => el.getAttribute('data-source-lines'))).toEqual(['6-7', '7-8', '8-10']);

    expect(annotated.container.querySelector('blockquote')).toHaveAttribute('data-source-lines', '10-11');
    expect(annotated.container.querySelector('pre')).toHaveAttribute('data-source-lines', '12-15');

    const tableRows = Array.from(annotated.container.querySelectorAll('table tr'));
    expect(tableRows.map((el) => el.getAttribute('data-source-lines'))).toEqual(['16-17', '18-19']);
  });
});
