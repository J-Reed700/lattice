/**
 * Finding a cited passage inside an open PDF (BRIEF rank 1).
 *
 * No extractor writes a page number into a chunk, so the page is resolved
 * client-side by scanning the already-loaded document. That is why a citation
 * row shows `p. 12` only *after* the file has been opened once: the page is
 * resolved, never guessed.
 */

import { buildNeedles, normalizeForMatch } from './passageLocator';

/** Pages scanned before giving up. A very long PDF is not worth a stall. */
const DEFAULT_MAX_PAGES = 400;

/** Words in the shortest run we are willing to mark on the page. */
const MIN_MARK_WORDS = 6;

interface PdfTextItem {
  str: string;
}

interface PdfPageLike {
  getTextContent(): Promise<{ items: PdfTextItem[] }>;
}

interface PdfDocumentLike {
  numPages: number;
  getPage(_pageNumber: number): Promise<PdfPageLike>;
}

/**
 * Escape `&`, `<`, `>` and `"` for safe innerHTML injection.
 *
 * react-pdf's `customTextRenderer` returns a **string that is assigned to
 * innerHTML**. Every fragment we do not control has to come back through here
 * or a PDF containing `<script>` would execute it.
 */
export function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

const escapeRegExp = (value: string): string => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/**
 * A react-pdf `customTextRenderer` that wraps occurrences of `needle` in
 * `<mark class="lattice-pdf-mark">`. Every non-matching fragment is escaped.
 */
export function buildPassageTextRenderer(
  needle: string
): (_item: { str: string }) => string {
  const trimmed = needle.trim();
  if (!trimmed) return (item) => escapeHtml(item.str);

  const pattern = new RegExp(`(${escapeRegExp(trimmed)})`, 'gi');
  return (item) => {
    const text = item.str ?? '';
    if (!text) return '';
    // Reset between items: the regex is global and stateful.
    pattern.lastIndex = 0;
    if (!pattern.test(text)) return escapeHtml(text);
    pattern.lastIndex = 0;
    return text
      .split(pattern)
      .map((part, index) =>
        // split() with one capture group puts matches at odd indices.
        index % 2 === 1
          ? `<mark class="lattice-pdf-mark">${escapeHtml(part)}</mark>`
          : escapeHtml(part)
      )
      .join('');
  };
}

/**
 * The literal on-page substring to mark for a normalized needle.
 *
 * Matching happens on normalized text, but the renderer sees the *raw* page
 * strings, so we look for the longest run of needle words that occurs verbatim
 * in the raw page. Returns null when nothing long enough survives — landing on
 * the right page is the feature; the mark is a nicety, and a mark drawn in the
 * wrong place would be worse than none.
 */
function literalMarkFor(rawPageText: string, needle: string): string | null {
  const words = needle.split(' ').filter(Boolean);
  const haystack = rawPageText.toLowerCase();

  for (let length = words.length; length >= MIN_MARK_WORDS; length -= 1) {
    for (let start = 0; start + length <= words.length; start += 1) {
      const run = words.slice(start, start + length).join(' ');
      if (haystack.includes(run)) return run;
    }
  }
  return null;
}

/**
 * Scan pages for the passage, longest needle first.
 *
 * Returns the 1-based page and a literal substring that occurs on it, or null.
 * Aborts as soon as `signal.aborted` flips — the caller unmounts freely.
 */
export async function findPassagePage(
  pdf: PdfDocumentLike,
  text: string,
  options: { maxPages?: number; signal?: { aborted: boolean } } = {}
): Promise<{ page: number; needle: string } | null> {
  const needles = buildNeedles(text);
  if (needles.length === 0) return null;

  const maxPages = Math.min(pdf.numPages, options.maxPages ?? DEFAULT_MAX_PAGES);

  for (let pageNumber = 1; pageNumber <= maxPages; pageNumber += 1) {
    if (options.signal?.aborted) return null;

    let pageText: string;
    try {
      const page = await pdf.getPage(pageNumber);
      const content = await page.getTextContent();
      pageText = content.items.map((item) => item.str).join(' ');
    } catch {
      // A page that will not render is a page we cannot search. Keep going.
      continue;
    }

    const normalized = normalizeForMatch(pageText);
    if (!normalized) continue;

    for (const needle of needles) {
      if (!normalized.includes(needle)) continue;
      // Marked with the raw run when one exists, unmarked otherwise.
      return { page: pageNumber, needle: literalMarkFor(pageText, needle) ?? '' };
    }
  }

  return null;
}
