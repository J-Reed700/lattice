import { useState } from 'react';

import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { describe, expect, it } from 'vitest';

import { TiptapEditor } from '@/components/TiptapEditor';
import {
  JournalFormattingPill,
  useDomTiptapEditor,
} from '@/features/journal/components/JournalFormattingPill';

function Harness({
  initial = 'Compost pile reached peak heat.',
  initialLoading = false,
}: {
  initial?: string;
  initialLoading?: boolean;
}) {
  const [markdown, setMarkdown] = useState(initial);
  const [isLoading, setIsLoading] = useState(initialLoading);
  const [container, setContainer] = useState<HTMLDivElement | null>(null);
  const editor = useDomTiptapEditor(container, 'note-1');

  return (
    <div>
      <button type="button" onClick={() => setIsLoading(false)}>
        Finish loading
      </button>
      <JournalFormattingPill editor={editor} />
      {isLoading ? (
        <p>Loading journal…</p>
      ) : (
        <div ref={setContainer}>
          <TiptapEditor value={markdown} onChange={setMarkdown} ariaLabel="Journal entry" />
        </div>
      )}
      <output data-testid="md">{markdown}</output>
    </div>
  );
}

describe('JournalFormattingPill', () => {
  it('wires inline formatting buttons to the mounted TipTap editor without losing selection', async () => {
    render(
      <MemoryRouter>
        <Harness />
      </MemoryRouter>,
    );

    const h1 = screen.getByRole('button', { name: 'Heading 1' });
    const h2 = screen.getByRole('button', { name: 'Heading 2' });
    const bold = screen.getByRole('button', { name: 'Bold' });
    const italic = screen.getByRole('button', { name: 'Italic' });
    const code = screen.getByRole('button', { name: 'Inline code' });
    const bulletList = screen.getByRole('button', { name: 'Bullet list' });
    const taskList = screen.getByRole('button', { name: 'Task list' });
    const blockquote = screen.getByRole('button', { name: 'Blockquote' });

    await waitFor(() => expect(h1).toBeEnabled());

    const mouseDown = new MouseEvent('mousedown', { bubbles: true, cancelable: true });
    h1.dispatchEvent(mouseDown);
    expect(mouseDown.defaultPrevented).toBe(true);

    fireEvent.click(h1);
    await waitFor(() => expect(h1).toHaveAttribute('aria-pressed', 'true'));
    expect(screen.getByTestId('md').textContent).toContain('# Compost pile reached peak heat.');

    fireEvent.click(h2);
    await waitFor(() => expect(h2).toHaveAttribute('aria-pressed', 'true'));
    expect(h1).toHaveAttribute('aria-pressed', 'false');

    fireEvent.click(h2);
    fireEvent.click(blockquote);
    await waitFor(() => expect(blockquote).toHaveAttribute('aria-pressed', 'true'));
    fireEvent.click(blockquote);

    fireEvent.click(bulletList);
    await waitFor(() => expect(bulletList).toHaveAttribute('aria-pressed', 'true'));
    fireEvent.click(bulletList);

    fireEvent.click(taskList);
    await waitFor(() => expect(taskList).toHaveAttribute('aria-pressed', 'true'));

    for (const button of [bold, italic, code]) {
      expect(button).toHaveAttribute('type', 'button');
      expect(button).toBeEnabled();
    }
  });

  it('enables formatting buttons when editor container mounts after loading finishes with constant noteId', async () => {
    render(
      <MemoryRouter>
        <Harness initialLoading />
      </MemoryRouter>,
    );

    const h1 = screen.getByRole('button', { name: 'Heading 1' });
    const bold = screen.getByRole('button', { name: 'Bold' });
    expect(h1).toBeDisabled();
    expect(bold).toBeDisabled();

    fireEvent.click(screen.getByRole('button', { name: 'Finish loading' }));

    await waitFor(() => expect(h1).toBeEnabled());
    expect(bold).toBeEnabled();

    fireEvent.click(h1);
    await waitFor(() => expect(h1).toHaveAttribute('aria-pressed', 'true'));
    expect(screen.getByTestId('md').textContent).toContain('# Compost pile reached peak heat.');
  });
});
