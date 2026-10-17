import Image, { type ImageOptions } from '@tiptap/extension-image';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import { Decoration, DecorationSet } from '@tiptap/pm/view';

import type { Node as ProseMirrorNode } from '@tiptap/pm/model';
import type { NodeView } from '@tiptap/pm/view';

/** What the viewer needs to draw one picture of a chat answer. */
export interface ChatFigure {
  /** `img-N`, as the answer's Markdown names it. */
  ref: string;
  /** Where the stored file is served from (the `lattice-image` scheme). */
  url: string;
  caption: string;
  width: number;
  height: number;
  sourceUrl: string;
  sourceTitle: string | null;
  license: string | null;
  attribution: string | null;
  /** The answer's citation number for the source page, once its sources are in. */
  citationNumber: number | null;
}

/** Read at draw time, so pictures and citations that arrive later light up. */
export interface ChatImageHost {
  figures: () => readonly ChatFigure[];
  /** The answer is still streaming: a ref it names may not have arrived yet. */
  pending: () => boolean;
  citationsVisible: () => boolean;
  onOpen: (figure: ChatFigure) => void;
}

export interface ChatImageOptions extends ImageOptions {
  /** Present in a chat answer. Elsewhere images render as before. */
  host: ChatImageHost | undefined;
}

export const chatImagesKey = new PluginKey<number>('chatImages');

const CHAT_IMAGE_REF = /^img-\d+$/;

function element<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function hostOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, '');
  } catch {
    return url;
  }
}

export function figureCredit(figure: Pick<ChatFigure, 'license' | 'attribution'>): string {
  return [figure.license, figure.attribution].filter((part): part is string => Boolean(part?.trim())).join(' · ');
}

/**
 * One picture in a read-only answer. Plain DOM, drawn synchronously like
 * ProseMirror's own nodes.
 *
 * Only an `img-N` ref this answer's own list holds becomes an `<img>`, and its
 * address is the local scheme. Anything else the model wrote (a web address,
 * a `data:` URI, a ref that never arrived) is a quiet placeholder and nothing
 * is fetched.
 */
class ChatImageView implements NodeView {
  dom: HTMLElement;
  private node: ProseMirrorNode;
  private signature = '';
  private failedUrl: string | null = null;

  constructor(node: ProseMirrorNode, private readonly host: ChatImageHost) {
    this.node = node;
    this.dom = element('span', 'chat-figure');
    this.draw();
  }

  update(node: ProseMirrorNode): boolean {
    if (node.type !== this.node.type) return false;
    this.node = node;
    this.draw();
    return true;
  }

  // Clicks on the picture are the view's own; the caption's citation chip is
  // left to the citation plugin and the message, as in the text.
  stopEvent(event: Event): boolean {
    return event.target instanceof Element && event.target.closest('.chat-figure-open') !== null;
  }

  ignoreMutation(): boolean {
    return true;
  }

  private draw() {
    const src = typeof this.node.attrs.src === 'string' ? this.node.attrs.src : '';
    const alt = typeof this.node.attrs.alt === 'string' ? this.node.attrs.alt : '';
    const figure = CHAT_IMAGE_REF.test(src) ? this.host.figures().find((candidate) => candidate.ref === src) : undefined;
    const visible = this.host.citationsVisible();
    const state = !CHAT_IMAGE_REF.test(src)
      ? 'external'
      : !figure
        ? (this.host.pending() ? 'pending' : 'missing')
        : this.failedUrl === figure.url ? 'missing' : 'shown';
    const signature = JSON.stringify([state, src, alt, state === 'shown' ? figure : null, visible]);
    if (signature === this.signature) return;
    this.signature = signature;
    this.dom.replaceChildren();
    this.dom.dataset.state = state;
    if (state !== 'shown' || !figure) {
      this.drawPlaceholder(state, alt);
      return;
    }
    this.drawFigure(figure, visible);
  }

  private drawPlaceholder(state: string, alt: string) {
    const message = state === 'external'
      ? 'External images are not shown here'
      : state === 'pending' ? 'Fetching the picture…' : 'Picture not available';
    const box = element('span', 'chat-figure-placeholder');
    box.append(element('span', 'chat-figure-placeholder-message', message));
    if (alt.trim()) box.append(element('span', 'chat-figure-placeholder-alt', alt));
    this.dom.append(box);
  }

  private drawFigure(figure: ChatFigure, citationsVisible: boolean) {
    const open = element('button', 'chat-figure-open');
    open.type = 'button';
    open.setAttribute('aria-label', `Enlarge picture: ${figure.caption}`);
    open.addEventListener('click', (event) => {
      event.preventDefault();
      this.host.onOpen(figure);
    });
    const image = element('img');
    // Width and height reserve the box before the bytes arrive, so the
    // answer does not jump when the picture loads.
    image.width = figure.width;
    image.height = figure.height;
    image.alt = figure.caption;
    image.loading = 'lazy';
    image.decoding = 'async';
    image.addEventListener('error', () => {
      // The scheme refuses a file whose hash no longer matches.
      this.failedUrl = figure.url;
      this.draw();
    });
    image.src = figure.url;
    open.append(image);

    const caption = element('span', 'chat-figure-caption');
    caption.append(element('span', 'chat-figure-text', figure.caption));
    if (citationsVisible && figure.citationNumber !== null) {
      // The same chip the text's `[n]` marks become, so hover and click open
      // the source page the way they do there.
      const chip = element('span', 'cite-chip', `[${figure.citationNumber}]`);
      chip.dataset.cite = String(figure.citationNumber);
      chip.setAttribute('role', 'button');
      chip.tabIndex = 0;
      chip.setAttribute('aria-label', `Citation ${figure.citationNumber}`);
      caption.append(' ', chip);
    } else {
      const link = element('a', 'chat-figure-source', figure.sourceTitle ?? hostOf(figure.sourceUrl));
      link.href = figure.sourceUrl;
      link.target = '_blank';
      link.rel = 'noopener noreferrer';
      caption.append(' ', link);
    }
    const credit = figureCredit(figure);
    if (credit) caption.append(element('span', 'chat-figure-credit', credit));
    this.dom.append(open, caption);
  }
}

/**
 * The image node, drawn by `ChatImageView` in a read-only chat answer. An
 * editor created editable, or a viewer outside chat, keeps the plain image.
 */
export const ChatImage = Image.extend<ChatImageOptions>({
  addOptions() {
    return { ...this.parent?.() as ImageOptions, host: undefined };
  },

  addNodeView() {
    const { host } = this.options;
    if (!host || this.editor.options.editable) return null;
    return ({ node }) => new ChatImageView(node, host);
  },

  addProseMirrorPlugins() {
    if (!this.options.host) return [];
    // The viewer bumps this when pictures, citations or streaming change. A
    // changed decoration is what makes ProseMirror hand each picture's view
    // an update, even when the text has not changed.
    return [
      new Plugin<number>({
        key: chatImagesKey,
        state: {
          init: () => 0,
          apply: (tr, epoch) => (tr.getMeta(chatImagesKey) === undefined ? epoch : epoch + 1),
        },
        props: {
          decorations(state) {
            const epoch = chatImagesKey.getState(state) ?? 0;
            const decorations: Decoration[] = [];
            state.doc.descendants((node, pos) => {
              if (node.type.name === 'image') decorations.push(Decoration.node(pos, pos + node.nodeSize, {}, { chatImageEpoch: epoch }));
            });
            return decorations.length ? DecorationSet.create(state.doc, decorations) : DecorationSet.empty;
          },
        },
      }),
    ];
  },
});
