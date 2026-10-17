import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { Editor } from '@tiptap/core';
import { Markdown } from 'tiptap-markdown';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { createExtensions } from '../extensions';
import { hslTripleToHex } from '../mermaid/render';
import { TiptapViewer } from '../TiptapViewer';

const mermaid = vi.hoisted(() => ({
  initialize: vi.fn(),
  parse: vi.fn(async (source: string) => {
    if (source.includes('BROKEN')) {
      throw new Error("Parse error on line 2:\n...A -->\n------^\nExpecting 'NODE_STRING', got 'EOF'");
    }
    return { diagramType: 'flowchart-v2' };
  }),
  render: vi.fn(async (id: string, _source: string) => ({
    svg: `<svg id="${id}" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" onload="alert(1)">`
      + '<script>alert(2)</script><g class="node"><text>A</text></g>'
      + '<foreignObject><div>html label</div></foreignObject></svg>',
  })),
}));
vi.mock('mermaid', () => ({ default: mermaid }));

function fence(language: string, body: string): string {
  return `\`\`\`${language}\n${body}\n\`\`\``;
}

function renderCallsFor(source: string): number {
  return mermaid.render.mock.calls.filter(([, text]) => text === source).length;
}

afterEach(() => {
  document.documentElement.removeAttribute('data-theme');
  document.documentElement.classList.remove('dark');
});

describe('code blocks in the read-only viewer', () => {
  it('keeps an ordinary fence a highlighted code block', async () => {
    const view = render(<TiptapViewer content={fence('js', 'const answer = 42;')} />);
    await waitFor(() => expect(view.container.querySelector('pre code.language-js')).not.toBeNull());
    expect(view.container.querySelector('pre .hljs-keyword')).not.toBeNull();
    expect(view.container.querySelector('.diagram-figure')).toBeNull();
  });

  it('draws a closed mermaid fence once, however often it mounts, and sanitises the SVG', async () => {
    const source = 'flowchart LR\n  A1 --> B1';
    const first = render(<TiptapViewer content={`Intro.\n\n${fence('mermaid', source)}\n\nAfter.`} />);
    await waitFor(() => expect(first.container.querySelector('.diagram-svg svg')).not.toBeNull());
    const second = render(<TiptapViewer content={fence('mermaid', source)} />);
    await waitFor(() => expect(second.container.querySelector('.diagram-svg svg')).not.toBeNull());

    expect(renderCallsFor(source)).toBe(1);
    const svg = first.container.querySelector('.diagram-svg svg')!;
    expect(svg.querySelector('script')).toBeNull();
    expect(svg.querySelector('foreignObject')).toBeNull();
    expect(svg.hasAttribute('onload')).toBe(false);
    expect(first.container.querySelector('.diagram-figure pre')).toHaveAttribute('hidden');
    // Sized from the viewBox: natural width, shrinking no further than 85%.
    expect(first.container.querySelector<HTMLElement>('.diagram-svg')!.style.getPropertyValue('--diagram-width')).toBe('10px');
  });

  it('holds back the fence a streaming answer has not closed, and draws it when the answer ends', async () => {
    const closed = 'flowchart LR\n  A2 --> B2';
    const open = 'flowchart LR\n  C2 --> D2';
    const content = `${fence('mermaid', closed)}\n\nThen:\n\n\`\`\`mermaid\n${open}`;
    const view = render(<TiptapViewer content={content} streaming />);
    await waitFor(() => expect(view.container.querySelectorAll('.diagram-svg svg')).toHaveLength(1));
    const blocks = view.container.querySelectorAll('pre');
    expect(blocks[blocks.length - 1]).toHaveTextContent('C2 --> D2');
    expect(blocks[blocks.length - 1].closest('.diagram-figure')).toBeNull();
    expect(renderCallsFor(open)).toBe(0);

    // The last chunk changes no text; the answer is just no longer pending.
    view.rerender(<TiptapViewer content={content} streaming={false} />);
    await waitFor(() => expect(view.container.querySelectorAll('.diagram-svg svg')).toHaveLength(2));
    expect(renderCallsFor(closed)).toBe(1);
  });

  it("shows the code and Mermaid's complaint when a fence cannot be drawn", async () => {
    const view = render(<TiptapViewer content={fence('mermaid', 'flowchart LR\n  BROKEN -->')} />);
    await waitFor(() => expect(view.container.querySelector('.diagram-error')).not.toBeNull());
    expect(view.container.querySelector('.diagram-error')).toHaveTextContent(
      "Diagram could not be drawn: Parse error on line 2: unexpected EOF",
    );
    const code = view.container.querySelector('.diagram-figure pre')!;
    expect(code).not.toHaveAttribute('hidden');
    expect(code).toHaveTextContent('BROKEN -->');
    expect(view.container.querySelector('.diagram-svg')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Source' })).toBeNull();
  });

  it('redraws with the new colours after a theme switch', async () => {
    const source = 'flowchart LR\n  A3 --> B3';
    const view = render(<TiptapViewer content={fence('mermaid', source)} />);
    await waitFor(() => expect(view.container.querySelector('.diagram-svg svg')).not.toBeNull());
    expect(renderCallsFor(source)).toBe(1);

    act(() => {
      document.documentElement.setAttribute('data-theme', 'dark');
      document.documentElement.classList.add('dark');
    });
    await waitFor(() => expect(renderCallsFor(source)).toBe(2));
    // The previous drawing stays up while the new one is made.
    expect(view.container.querySelector('.diagram-svg svg')).not.toBeNull();
  });

  it('flips to the source and enlarges the drawing', async () => {
    const view = render(<TiptapViewer content={fence('mermaid', 'flowchart LR\n  A4 --> B4')} />);
    await waitFor(() => expect(view.container.querySelector('.diagram-svg svg')).not.toBeNull());

    fireEvent.click(screen.getByRole('button', { name: 'Source' }));
    expect(view.container.querySelector('.diagram-figure pre')).not.toHaveAttribute('hidden');
    expect(view.container.querySelector('.diagram-svg')).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'Enlarge' }));
    const dialog = await screen.findByRole('dialog', { name: 'Diagram' });
    expect(dialog.querySelector('.media-lightbox-diagram svg')).not.toBeNull();
  });

  it('keeps every fence as code when diagrams are off', async () => {
    const source = 'flowchart LR\n  A5 --> B5';
    const view = render(<TiptapViewer content={fence('mermaid', source)} diagrams={false} />);
    await waitFor(() => expect(view.container.querySelector('pre code')).not.toBeNull());
    expect(view.container.querySelector('.diagram-figure')).toBeNull();
    expect(renderCallsFor(source)).toBe(0);
  });

  it('leaves an editable editor its plain code block', () => {
    const element = document.createElement('div');
    const editor = new Editor({
      element,
      extensions: [...createExtensions(), Markdown.configure({ html: false })],
      content: fence('mermaid', 'flowchart LR\n  A6 --> B6'),
      editable: true,
    });
    expect(element.querySelector('pre > code')).toHaveTextContent('A6 --> B6');
    expect(element.querySelector('[data-node-view-wrapper], .diagram-figure')).toBeNull();
    editor.destroy();
  });
});

describe('hslTripleToHex', () => {
  it('converts the theme tokens Mermaid is given', () => {
    expect(hslTripleToHex('0 0% 100%')).toBe('#ffffff');
    expect(hslTripleToHex('12 52% 40%')).toBe('#9b4631');
    expect(hslTripleToHex('not a colour')).toBeNull();
  });

  it('lays a translucent token over the surface it sits on', () => {
    expect(hslTripleToHex('0 0% 0% / 0.5', '0 0% 100%')).toBe('#808080');
  });
});
