import { act, render, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { reply } from '@/tests/replyFixtures';
import { drawReply } from '@/utils/replyPieces';

import { TiptapViewer } from '../TiptapViewer';

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

const first = drawReply(reply('First page ', 4, '.'));
const next = drawReply(reply('Next page ', 7, '.'));

describe('TiptapViewer lifecycle', () => {
  it('waits for a replacement editor when content changes after teardown', async () => {
    const view = render(<TiptapViewer content={first.markdown} marks={first.numbers} citationNumbers={[4]} />);
    await waitFor(() => expect(view.container).toHaveTextContent('First page'));
    const previous = captured.editor!;
    act(() => previous.destroy());
    expect(previous.isDestroyed).toBe(true);

    view.rerender(<TiptapViewer content={next.markdown} marks={next.numbers} citationNumbers={[7]} showEvidence={false} />);
    await waitFor(() => {
      expect(captured.editor).not.toBe(previous);
      expect(view.container).toHaveTextContent('Next page');
      expect(view.container.querySelector('.citation-hidden')).not.toBeNull();
    });

    view.rerender(<TiptapViewer content={next.markdown} marks={next.numbers} citationNumbers={[7]} />);
    await waitFor(() => expect(view.container.querySelector('[data-cite="7"]')).not.toBeNull());
    view.unmount();
    const remounted = render(<TiptapViewer content="Returned to the lesson." />);
    await waitFor(() => expect(remounted.container).toHaveTextContent('Returned to the lesson.'));
  });
});
