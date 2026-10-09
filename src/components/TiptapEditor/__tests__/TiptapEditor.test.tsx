import { act, render, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { describe, expect, it, vi } from 'vitest';

import { TiptapEditor } from '../TiptapEditor';

import type { Editor } from '@tiptap/react';

const captured = vi.hoisted(() => ({ editor: null as Editor | null }));
vi.mock('@tiptap/react', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@tiptap/react')>();
  return {
    ...actual,
    useEditor: (...args: Parameters<typeof actual.useEditor>) => {
      const editor = actual.useEditor(...args);
      captured.editor = editor;
      return editor;
    },
  };
});

describe('TiptapEditor external content synchronization', () => {
  it('loads and clears external markdown without reporting an edit', async () => {
    const onChange = vi.fn();
    const view = render(<TiptapEditor value="First page." onChange={onChange} />, {
      wrapper: MemoryRouter,
    });
    await waitFor(() => expect(view.getByRole('textbox')).toHaveTextContent('First page.'));
    expect(onChange).not.toHaveBeenCalled();

    view.rerender(
      <TiptapEditor
        value={'## Loaded page\n\nA **new** claim [7].'}
        onChange={onChange}
        citationNumbers={[7]}
      />,
    );
    await waitFor(() => {
      expect(view.getByRole('heading', { name: 'Loaded page', level: 2 })).toBeInTheDocument();
      expect(view.container.querySelector('strong')).toHaveTextContent('new');
      expect(view.container.querySelector('[data-cite="7"]')).toHaveTextContent('[7]');
    });
    expect(onChange).not.toHaveBeenCalled();

    view.rerender(<TiptapEditor value="" onChange={onChange} />);
    await waitFor(() => expect(view.getByRole('textbox')).toHaveTextContent(/^$/));
    expect(onChange).not.toHaveBeenCalled();
  });

  it('reports subsequent edits once using the latest onChange callback', async () => {
    const previousOnChange = vi.fn();
    const onChange = vi.fn();
    const view = render(<TiptapEditor value="First page." onChange={previousOnChange} />, {
      wrapper: MemoryRouter,
    });
    await waitFor(() => expect(view.getByRole('textbox')).toHaveTextContent('First page.'));

    view.rerender(<TiptapEditor value="Loaded page." onChange={onChange} />);
    await waitFor(() => expect(view.getByRole('textbox')).toHaveTextContent('Loaded page.'));
    expect(onChange).not.toHaveBeenCalled();

    act(() => {
      const editor = captured.editor!;
      editor.chain().setTextSelection(editor.state.doc.content.size - 1).insertContent(' Edited.').run();
    });
    expect(previousOnChange).not.toHaveBeenCalled();
    expect(onChange).toHaveBeenCalledExactlyOnceWith('Loaded page. Edited.');

    // A controlled parent echo must neither report another edit nor reset the cursor.
    const selection = captured.editor!.state.selection.from;
    view.rerender(<TiptapEditor value="Loaded page. Edited." onChange={onChange} />);
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(captured.editor!.state.selection.from).toBe(selection);
  });

  it('waits for a replacement editor when content changes after teardown', async () => {
    const onChange = vi.fn();
    const view = render(
      <TiptapEditor value="First page [4]." onChange={onChange} citationNumbers={[4]} />,
      { wrapper: MemoryRouter },
    );
    await waitFor(() => expect(view.getByRole('textbox')).toHaveTextContent('First page [4].'));
    const previous = captured.editor!;
    act(() => previous.destroy());
    expect(previous.isDestroyed).toBe(true);

    view.rerender(
      <TiptapEditor value="Next page [7]." onChange={onChange} citationNumbers={[7]} />,
    );
    await waitFor(() => {
      expect(captured.editor).not.toBe(previous);
      expect(captured.editor!.isDestroyed).toBe(false);
      expect(view.getByRole('textbox')).toHaveTextContent('Next page [7].');
      expect(view.container.querySelector('[data-cite="7"]')).toHaveTextContent('[7]');
    });
    expect(onChange).not.toHaveBeenCalled();

    act(() => {
      const editor = captured.editor!;
      editor.chain().setTextSelection(editor.state.doc.content.size - 1).insertContent(' Edited.').run();
    });
    expect(onChange).toHaveBeenCalledExactlyOnceWith('Next page \\[7\\]. Edited.');
  });
});
