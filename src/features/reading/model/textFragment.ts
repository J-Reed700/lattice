const ANCHOR_WORDS = 5;
const SHORT_PASSAGE_WORDS = 12;

function encodeDirective(text: string): string {
  return encodeURIComponent(text).replace(/-/g, '%2D');
}

/** Generates a URL with a text fragment directive (`#:~:text=...`) pointing to `passage`. */
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
