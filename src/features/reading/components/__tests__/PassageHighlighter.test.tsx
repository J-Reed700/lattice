import { render, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { PassageHighlighter } from '../PassageHighlighter';

const CONTENT = (
  <>
    <p data-testid="p1">An opening paragraph that is not the passage at all.</p>
    <p data-testid="p2">
      The specific passage we are looking for lives inside this paragraph and
      nowhere else in the document.
    </p>
    <p data-testid="p3">A closing paragraph that is also not the passage.</p>
  </>
);

const PASSAGE =
  'The specific passage we are looking for lives inside this paragraph';

describe('PassageHighlighter', () => {
  it('marks the paragraph containing the passage and reports an exact match', async () => {
    const onMatch = vi.fn();
    const { getByTestId } = render(
      <PassageHighlighter locator={{ text: PASSAGE }} onMatch={onMatch}>
        {CONTENT}
      </PassageHighlighter>
    );

    await waitFor(() => expect(onMatch).toHaveBeenCalledWith('exact'));
    expect(getByTestId('p2').classList.contains('lattice-passage-block')).toBe(true);
    expect(getByTestId('p1').classList.contains('lattice-passage-block')).toBe(false);
    expect(getByTestId('p3').classList.contains('lattice-passage-block')).toBe(false);
  });

  it('reports an approximate match when the text is gone but the ordinal is known', async () => {
    const onMatch = vi.fn();
    render(
      <PassageHighlighter
        locator={{ text: 'text that does not appear anywhere in here', chunkIndex: 6 }}
        onMatch={onMatch}
      >
        {CONTENT}
      </PassageHighlighter>
    );

    await waitFor(() => expect(onMatch).toHaveBeenCalledWith('approximate'));
  });

  it('reports no match when there is nothing to go on', async () => {
    const onMatch = vi.fn();
    render(
      <PassageHighlighter
        locator={{ text: 'text that does not appear anywhere in here' }}
        onMatch={onMatch}
      >
        {CONTENT}
      </PassageHighlighter>
    );

    await waitFor(() => expect(onMatch).toHaveBeenCalledWith('none'));
  });

  it('reports no match for an empty locator without touching the DOM', async () => {
    const onMatch = vi.fn();
    const { getByTestId } = render(
      <PassageHighlighter locator={{ text: '   ' }} onMatch={onMatch}>
        {CONTENT}
      </PassageHighlighter>
    );

    await waitFor(() => expect(onMatch).toHaveBeenCalledWith('none'));
    expect(getByTestId('p2').classList.contains('lattice-passage-block')).toBe(false);
  });

  it('highlights the passage without marking query keywords', async () => {
    const onMatch = vi.fn();
    const { container } = render(
      <PassageHighlighter
        locator={{ text: PASSAGE, highlights: ['passage'] }}
        onMatch={onMatch}
      >
        {CONTENT}
      </PassageHighlighter>
    );

    await waitFor(() => expect(onMatch).toHaveBeenCalledWith('exact'));
    expect(container.querySelectorAll('.lattice-passage-term')).toHaveLength(0);
    expect(container.querySelectorAll('.lattice-passage-block')).toHaveLength(1);
  });

  it('removes every class it added on unmount', async () => {
    const onMatch = vi.fn();
    const host = document.createElement('div');
    document.body.appendChild(host);

    const { unmount } = render(
      <PassageHighlighter
        locator={{ text: PASSAGE, highlights: ['passage'] }}
        onMatch={onMatch}
      >
        {CONTENT}
      </PassageHighlighter>,
      { container: host }
    );

    await waitFor(() => expect(onMatch).toHaveBeenCalledWith('exact'));
    expect(host.querySelectorAll('.lattice-passage-block').length).toBeGreaterThan(0);

    unmount();
    expect(host.querySelectorAll('.lattice-passage-block')).toHaveLength(0);
    expect(host.querySelectorAll('.lattice-passage-term')).toHaveLength(0);
    host.remove();
  });

  it('waits until loading finishes and highlights the passage once document content renders', async () => {
    const onMatch = vi.fn();
    const { rerender, getByTestId } = render(
      <PassageHighlighter locator={{ text: PASSAGE }} isLoading contentKey="" onMatch={onMatch}>
        <div data-testid="spinner">Loading...</div>
      </PassageHighlighter>
    );

    // Must not get stuck on 'none' while the spinner is visible.
    expect(onMatch).not.toHaveBeenCalled();

    rerender(
      <PassageHighlighter locator={{ text: PASSAGE }} isLoading={false} contentKey={PASSAGE} onMatch={onMatch}>
        {CONTENT}
      </PassageHighlighter>
    );

    await waitFor(() => expect(onMatch).toHaveBeenCalledWith('exact'));
    expect(getByTestId('p2').classList.contains('lattice-passage-block')).toBe(true);
  });

  it('re-locates via MutationObserver when child content mounts asynchronously after initial render', async () => {
    const onMatch = vi.fn();
    const { container } = render(
      <PassageHighlighter locator={{ text: PASSAGE }} isLoading={false} onMatch={onMatch}>
        <div data-testid="async-host" />
      </PassageHighlighter>
    );

    await waitFor(() => expect(onMatch).toHaveBeenCalledWith('none'));
    onMatch.mockClear();

    const host = container.querySelector('[data-testid="async-host"]')!;
    const p = document.createElement('p');
    p.textContent = PASSAGE;
    host.appendChild(p);

    await waitFor(() => expect(onMatch).toHaveBeenCalledWith('exact'));
    expect(p.classList.contains('lattice-passage-block')).toBe(true);
  });
});
