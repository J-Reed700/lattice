/**
 * Bringing a chat message into view — the one implementation of it.
 *
 * Three near-copies of this existed (the sidebar reference list, the
 * conversation spotlight, and the deep-link effect in `ChatView`), each with its
 * own retry budget and its own idea of how long the highlight lasts. They had
 * already drifted apart, and memory evidence would have made a fourth. Every
 * caller that jumps to a message goes through `scrollToMessage`.
 *
 * The retry loop is not defensive padding: the target is routinely absent when
 * the click lands, because the caller has just switched conversations or closed
 * a panel and React has not committed the thread yet. A caller that must react
 * to a message which never appears passes `onSettled`.
 */

/** Matches the `chat-message-highlight` keyframes in `index.css`. */
const MESSAGE_HIGHLIGHT_MS = 1500;
const MESSAGE_HIGHLIGHT_CLASS = 'chat-message-highlighted';
const FIRST_ATTEMPT_DELAY_MS = 80;
const RETRY_INTERVAL_MS = 120;
const MAX_ATTEMPTS = 16;

/** The DOM id `Message` renders; the only contract between jumper and thread. */
export const messageElementId = (messageId: string): string => `message-${messageId}`;

export const findMessageElement = (messageId: string): HTMLElement | null =>
  document.getElementById(messageElementId(messageId));

/** A span of a message identified the way the backend identifies it. */
export interface ValidatedSpan {
  /** The message's stored content, byte for byte as the backend holds it. */
  content: string;
  /** UTF-8 byte offsets into `content` — not string indices. */
  startByte: number;
  endByte: number;
}

export interface DecodedSpan {
  /** The text the byte range names. */
  text: string;
  /** Where that text starts in UTF-16 code units, used to pick an occurrence. */
  charStart: number;
}

/**
 * Turn a UTF-8 byte range into the text it actually names.
 *
 * `content.slice(startByte, endByte)` is wrong and fails quietly: JavaScript
 * string indices count UTF-16 code units while these offsets count UTF-8 bytes.
 * A single earlier `é` (2 bytes) or `—` (3 bytes) shifts every later offset, so
 * a plain slice returns a window that drifts further the more non-ASCII the
 * message is — the highlight lands on neighbouring words and nothing reports an
 * error. Encoding the content and decoding the byte window back is the only
 * conversion that agrees with the Rust side, which slices `&content[a..b]` on
 * bytes.
 *
 * Returns `null` for a range this message cannot support, so callers can scroll
 * without a highlight instead of marking the wrong words.
 */
export function decodeValidatedSpan(span: ValidatedSpan): DecodedSpan | null {
  const { content, startByte, endByte } = span;
  if (!Number.isInteger(startByte) || !Number.isInteger(endByte)) return null;
  if (startByte < 0 || endByte <= startByte) return null;

  const bytes = new TextEncoder().encode(content);
  if (endByte > bytes.length) return null;

  try {
    // `fatal` refuses a range that begins or ends inside a character rather
    // than decoding the broken edge to U+FFFD, which would highlight half a
    // letter and read as a rendering bug instead of a bad offset.
    const decoder = new TextDecoder('utf-8', { fatal: true });
    return {
      text: decoder.decode(bytes.subarray(startByte, endByte)),
      charStart: decoder.decode(bytes.subarray(0, startByte)).length,
    };
  } catch {
    return null;
  }
}

interface FlattenedText {
  text: string;
  nodes: { node: Text; start: number }[];
}

function flattenText(root: HTMLElement): FlattenedText {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const nodes: { node: Text; start: number }[] = [];
  let text = '';

  let node = walker.nextNode() as Text | null;
  while (node) {
    nodes.push({ node, start: text.length });
    text += node.data;
    node = walker.nextNode() as Text | null;
  }

  return { text, nodes };
}

function rangeAt(flat: FlattenedText, start: number, length: number): Range | null {
  const end = start + length;
  const startNode = flat.nodes.find(({ node, start: at }) => start >= at && start < at + node.data.length);
  const endNode = flat.nodes.find(({ node, start: at }) => end > at && end <= at + node.data.length);
  if (!startNode || !endNode) return null;

  const range = document.createRange();
  range.setStart(startNode.node, start - startNode.start);
  range.setEnd(endNode.node, end - endNode.start);
  return range;
}

/**
 * Mark the span inside the message using the browser's own selection.
 *
 * Deliberately not a spliced-in `<mark>`: message bodies are React-rendered
 * markdown that re-renders while a reply streams, and inserting an element
 * between React and a text node it owns makes React's next removal of that node
 * throw `NotFoundError`. A selection sits on top of the DOM rather than in it,
 * is styled by the `::selection` rule, and leaves the span copyable.
 *
 * Returns `false` when the text is not in the rendered message — rendered
 * markdown is not the raw content, so a quotation spanning markup may simply
 * not be there. The caller then leaves the viewport on the message with no
 * span highlight; a nearest-guess would assert evidence the user never wrote.
 */
export function highlightSpanWithin(element: HTMLElement, span: DecodedSpan): boolean {
  if (!span.text) return false;
  const selection = window.getSelection?.();
  if (!selection) return false;

  const flat = flattenText(element);
  // The same words can appear twice in one message, so the byte offsets decide
  // which occurrence is the evidence: take the one nearest where the span
  // starts in the raw content.
  let best = -1;
  for (let at = flat.text.indexOf(span.text); at !== -1; at = flat.text.indexOf(span.text, at + 1)) {
    if (best === -1 || Math.abs(at - span.charStart) < Math.abs(best - span.charStart)) best = at;
  }
  if (best === -1) return false;

  const range = rangeAt(flat, best, span.text.length);
  if (!range) return false;

  selection.removeAllRanges();
  selection.addRange(range);
  return true;
}

export interface ScrollToMessageOptions {
  /** A validated span to mark inside the message, already byte-decoded. */
  span?: DecodedSpan | null;
  /**
   * Called once the jump resolves: `found` after the message was reached and
   * its highlight faded, `missing` when it never appeared. Callers use it to
   * clean up deep-link state or to tell the user nothing was found.
   */
  onSettled?: (outcome: 'found' | 'missing') => void;
}

/**
 * Scroll the message into view and flash it. Returns a cancel function so an
 * effect can drop a pending jump without leaving a highlight class behind.
 */
export function scrollToMessage(
  messageId: string,
  options: ScrollToMessageOptions = {}
): () => void {
  const { span, onSettled } = options;
  let attempts = 0;
  let retryTimeout: number | undefined;
  let highlightTimeout: number | undefined;
  let highlighted: HTMLElement | null = null;

  const clearHighlight = () => {
    highlighted?.classList.remove(MESSAGE_HIGHLIGHT_CLASS);
    highlighted = null;
  };

  const tick = () => {
    const element = findMessageElement(messageId);
    if (element) {
      element.scrollIntoView({ behavior: 'smooth', block: 'center' });
      element.classList.add(MESSAGE_HIGHLIGHT_CLASS);
      highlighted = element;
      if (span) highlightSpanWithin(element, span);
      highlightTimeout = window.setTimeout(() => {
        clearHighlight();
        onSettled?.('found');
      }, MESSAGE_HIGHLIGHT_MS);
      return;
    }

    attempts += 1;
    if (attempts < MAX_ATTEMPTS) {
      retryTimeout = window.setTimeout(tick, RETRY_INTERVAL_MS);
    } else {
      onSettled?.('missing');
    }
  };

  retryTimeout = window.setTimeout(tick, FIRST_ATTEMPT_DELAY_MS);

  return () => {
    if (retryTimeout !== undefined) window.clearTimeout(retryTimeout);
    if (highlightTimeout !== undefined) window.clearTimeout(highlightTimeout);
    clearHighlight();
  };
}
