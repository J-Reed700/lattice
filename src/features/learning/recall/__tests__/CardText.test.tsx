import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { CardText, cardPreview } from '@/features/learning/recall/CardText';

vi.mock('@/features/reading/components/viewers/MarkdownViewer', () => ({ MarkdownViewer: ({ content }: { content: string }) => <div data-testid="markdown">{content}</div> }));

describe('review card text', () => {
  it('previews a card’s first line of prose without Markdown syntax', () => {
    expect(cardPreview('## What does **this** print?\n\n```python\nprint(sorted([3, 1]))\n```')).toBe('What does this print?');
    expect(cardPreview('- Use `items()` to read [pairs](https://example.org) in _order_')).toBe('Use items() to read pairs in order');
    expect(cardPreview('> A quoted premise.\nMore.')).toBe('A quoted premise.');
    expect(cardPreview('Plain text stays as it is.')).toBe('Plain text stays as it is.');
  });

  it('keeps identifiers and arithmetic that only look like emphasis', () => {
    expect(cardPreview('What does **`get_item_by_id`** return from `__init__`?')).toBe('What does get_item_by_id return from __init__?');
    expect(cardPreview('Call my_var_name twice; is _this_ cached?')).toBe('Call my_var_name twice; is this cached?');
    expect(cardPreview('Is 2 * 3 * 4 the same as `2*3*4`?')).toBe('Is 2 * 3 * 4 the same as 2*3*4?');
  });

  it('previews a card that opens with code by its first line of code', () => {
    expect(cardPreview('```python\nvalues = [3, 1, 2]\nprint(values)\n```')).toBe('values = [3, 1, 2]');
  });

  it('renders a card’s full text as Markdown, like the lesson it came from', () => {
    const text = 'What prints?\n\n```python\nprint(1)\n```';
    render(<CardText text={text} />);
    expect(screen.getByTestId('markdown').textContent).toBe(text);
  });
});
