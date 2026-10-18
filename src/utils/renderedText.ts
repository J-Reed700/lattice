/**
 * Rendered message text as one string, with every character's DOM point kept,
 * so text found in the string can be turned back into a `Range`.
 *
 * Text read from a selection or typed into a search crosses inline formatting
 * (bold, code, links) and block breaks freely, while the DOM splits it into
 * many text nodes. Matching against this flattened form finds it anyway.
 */

const BLOCK_TAGS = new Set([
  'P', 'LI', 'UL', 'OL', 'PRE', 'BLOCKQUOTE', 'H1', 'H2', 'H3', 'H4', 'H5', 'H6',
  'TABLE', 'TR', 'TD', 'TH', 'DIV', 'DT', 'DD', 'FIGCAPTION',
]);

interface TextPoint { node: Text; offset: number }

export interface RenderedText {
  text: string;
  /** The DOM point each character of `text` came from. */
  points: TextPoint[];
}

function blockOf(node: Node, root: HTMLElement): Element {
  for (let element = node.parentElement; element && element !== root; element = element.parentElement) {
    if (BLOCK_TAGS.has(element.tagName)) return element;
  }
  return root;
}

/**
 * `spaced` collapses each whitespace run to one space and counts a boundary
 * between blocks as a space (a selection's text has a line break there even
 * when the DOM has no whitespace node); otherwise whitespace is dropped.
 */
export function flattenRenderedText(root: HTMLElement, spaced: boolean): RenderedText {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  let text = '';
  const points: TextPoint[] = [];
  let previous: { node: Text; block: Element } | null = null;
  for (let node = walker.nextNode() as Text | null; node; node = walker.nextNode() as Text | null) {
    const block = blockOf(node, root);
    if (spaced && previous && previous.block !== block && text && !text.endsWith(' ')) {
      text += ' ';
      points.push({ node: previous.node, offset: previous.node.data.length });
    }
    for (let offset = 0; offset < node.data.length; offset += 1) {
      const char = node.data[offset];
      if (/\s/.test(char)) {
        if (!spaced || !text || text.endsWith(' ')) continue;
        text += ' ';
      } else {
        text += char;
      }
      points.push({ node, offset });
    }
    previous = { node, block };
  }
  return { text, points };
}

/** The DOM range of `length` characters of `flat.text` from `start`; neither end may be a space. */
export function renderedRange(flat: RenderedText, start: number, length: number): Range | null {
  const first = flat.points[start];
  const last = flat.points[start + length - 1];
  if (!first || !last || length <= 0) return null;
  const range = document.createRange();
  range.setStart(first.node, first.offset);
  range.setEnd(last.node, Math.min(last.offset + 1, last.node.data.length));
  return range;
}
