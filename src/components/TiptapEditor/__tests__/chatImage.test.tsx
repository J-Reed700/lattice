import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { Editor } from '@tiptap/core';
import { Markdown } from 'tiptap-markdown';
import { describe, expect, it } from 'vitest';

import { createExtensions } from '../extensions';
import { TiptapViewer } from '../TiptapViewer';

import type { ChatFigure } from '../extensions/chatImage';

const FIGURE: ChatFigure = {
  ref: 'img-1',
  url: 'lattice-image://localhost/0123456789abcdef0123456789abcdef.jpg',
  caption: 'Plane trees over a narrow street',
  width: 640,
  height: 480,
  sourceUrl: 'https://commons.wikimedia.org/wiki/File:Canopy.jpg',
  sourceTitle: 'File:Canopy.jpg',
  license: 'CC BY-SA 4.0',
  attribution: 'A. Photographer',
  citationNumber: 3,
};

const ANSWER = 'Here is the street.\n\n![Plane trees](img-1)\n\nShade covers most of it.';

describe('pictures in a chat answer', () => {
  it('draws a ref the answer holds, sized before it loads, with caption, chip and credit', () => {
    const view = render(<TiptapViewer content={ANSWER} images={[FIGURE]} />);
    const image = view.container.querySelector('.chat-figure img')!;
    expect(image).toHaveAttribute('src', FIGURE.url);
    expect(image).toHaveAttribute('width', '640');
    expect(image).toHaveAttribute('height', '480');
    expect(image).toHaveAttribute('alt', FIGURE.caption);
    const caption = view.container.querySelector('.chat-figure-caption')!;
    expect(caption).toHaveTextContent('Plane trees over a narrow street [3]');
    expect(caption.querySelector('.cite-chip')).toHaveAttribute('data-cite', '3');
    expect(caption.querySelector('.chat-figure-credit')).toHaveTextContent('CC BY-SA 4.0 · A. Photographer');
  });

  it('links the source page instead of a chip while citations are hidden or not in yet', () => {
    const view = render(<TiptapViewer content={ANSWER} images={[{ ...FIGURE, citationNumber: null }]} />);
    const link = view.container.querySelector('.chat-figure-source')!;
    expect(link).toHaveAttribute('href', FIGURE.sourceUrl);
    expect(link).toHaveTextContent('File:Canopy.jpg');
    view.rerender(<TiptapViewer content={ANSWER} images={[FIGURE]} showEvidence={false} />);
    expect(view.container.querySelector('.chat-figure .cite-chip')).toBeNull();
  });

  it('loads nothing for an address the model wrote or a ref it does not hold', () => {
    const content = [
      '![remote](https://example.com/cat.png)',
      '![inline](data:image/png;base64,iVBORw0KGgo=)',
      '![missing](img-2)',
    ].join('\n\n');
    const view = render(<TiptapViewer content={content} images={[FIGURE]} />);
    // ProseMirror's own separator after an inline node is the only <img>, with no src.
    expect(view.container.querySelector('img:not(.ProseMirror-separator)')).toBeNull();
    const placeholders = [...view.container.querySelectorAll('.chat-figure-placeholder')].map((box) => box.textContent);
    expect(placeholders).toEqual([
      'External images are not shown hereremote',
      'External images are not shown hereinline',
      'Picture not availablemissing',
    ]);
  });

  it('waits for a picture the stream has not delivered, then draws it', async () => {
    const view = render(<TiptapViewer content={ANSWER} images={[]} streaming />);
    expect(view.container.querySelector('.chat-figure-placeholder')).toHaveTextContent('Fetching the picture…');
    view.rerender(<TiptapViewer content={ANSWER} images={[FIGURE]} streaming />);
    await waitFor(() => expect(view.container.querySelector('.chat-figure img')).not.toBeNull());
  });

  it('says the picture is unavailable when the file fails to load', () => {
    const view = render(<TiptapViewer content={ANSWER} images={[FIGURE]} />);
    fireEvent.error(view.container.querySelector('.chat-figure img')!);
    expect(view.container.querySelector('.chat-figure img')).toBeNull();
    expect(view.container.querySelector('.chat-figure-placeholder')).toHaveTextContent('Picture not available');
  });

  it('opens the picture in the lightbox with its source page', async () => {
    const view = render(<TiptapViewer content={ANSWER} images={[FIGURE]} />);
    fireEvent.click(view.container.querySelector('.chat-figure-open')!);
    const dialog = await screen.findByRole('dialog', { name: 'File:Canopy.jpg' });
    expect(dialog.querySelector('img')).toHaveAttribute('src', FIGURE.url);
    expect(screen.getByRole('link', { name: 'Open source page' })).toHaveAttribute('href', FIGURE.sourceUrl);
  });

  it('leaves images alone outside chat and in an editable editor', () => {
    const view = render(<TiptapViewer content="![logo](https://example.com/logo.png)" />);
    expect(view.container.querySelector('img')).toHaveAttribute('src', 'https://example.com/logo.png');

    const element = document.createElement('div');
    const editor = new Editor({
      element,
      extensions: [
        ...createExtensions({
          chatImages: { figures: () => [], pending: () => false, citationsVisible: () => true, onOpen: () => {} },
        }),
        Markdown.configure({ html: false }),
      ],
      content: '![logo](https://example.com/logo.png)',
      editable: true,
    });
    expect(element.querySelector('img')).toHaveAttribute('src', 'https://example.com/logo.png');
    editor.destroy();
  });
});
