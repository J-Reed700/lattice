/**
 * The same highlight, in whatever browser the reader opens the page in.
 *
 * A URL text fragment (`#:~:text=start,end`) asks the browser to find that text
 * on the page, scroll to it and mark it — so leaving Lattice for the real
 * article does not lose the place the answer was drawn from.
 */

/** Enough words to be unambiguous without pinning the whole paragraph. */
const ANCHOR_WORDS = 5;
/** Below this, quoting the passage whole is shorter and more exact. */
const SHORT_PASSAGE_WORDS = 12;

/**
 * Percent-encode for a text directive.
 *
 * `-` and `,` are the directive's own syntax, so they have to be escaped even
 * though a URL would otherwise carry them as they are.
 */
function encodeDirective(text: string): string {
  return encodeURIComponent(text).replace(/-/g, '%2D');
}

/**
 * `url` with a text fragment pointing at `passage`.
 *
 * Any fragment the URL already carries is dropped: a page cannot be scrolled to
 * two places, and the passage is the one the reader asked for. An empty passage
 * leaves the URL exactly as it was.
 */
export function textFragmentUrl(url: string, passage: string): string {
  const words = passage.trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) return url;

  const hashAt = url.indexOf('#');
  const base = hashAt >= 0 ? url.slice(0, hashAt) : url;

  if (words.length <= SHORT_PASSAGE_WORDS) {
    return `${base}#:~:text=${encodeDirective(words.join(' '))}`;
  }

  const start = encodeDirective(words.slice(0, ANCHOR_WORDS).join(' '));
  const end = encodeDirective(words.slice(-ANCHOR_WORDS).join(' '));
  return `${base}#:~:text=${start},${end}`;
}
