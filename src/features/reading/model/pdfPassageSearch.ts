import { buildNeedles, normalizeForMatch } from '@/shared/sources/passageLocator';

const DEFAULT_MAX_PAGES = 400;

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

/** Escapes HTML special characters for safe injection into react-pdf customTextRenderer. */
export function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

const escapeRegExp = (value: string): string => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/** Wraps occurrences of `needle` in `<mark class="lattice-pdf-mark">`. */
export function buildPassageTextRenderer(
  needle: string
): (_item: { str: string }) => string {
  const trimmed = needle.trim();
  if (!trimmed) return (item) => escapeHtml(item.str);

  const pattern = new RegExp(`(${escapeRegExp(trimmed)})`, 'gi');
  return (item) => {
    const text = item.str ?? '';
    if (!text) return '';
    pattern.lastIndex = 0;
    if (!pattern.test(text)) return escapeHtml(text);
    pattern.lastIndex = 0;
    return text
      .split(pattern)
      .map((part, index) =>
        index % 2 === 1
          ? `<mark class="lattice-pdf-mark">${escapeHtml(part)}</mark>`
          : escapeHtml(part)
      )
      .join('');
  };
}

/** Matches the page as a whole, then maps the matched span back to PDF text items. */
export function passageItemMarks(items: PdfTextItem[], needle: string, compact = false): Record<number, [number, number]> {
  const raw = items.map(item => item.str ?? '').join(' ');
  const tokens = Array.from(raw.matchAll(/[\p{L}\p{N}'"\u2018-\u201f\u2010-\u2015-]+/gu))
    .map(match => ({ text: normalizeForMatch(match[0]), start: match.index!, end: match.index! + match[0].length }))
    .filter(token => token.text);
  const fold = (text: string) => compact ? text.replace(/[^\p{L}\p{N}]/gu, '') : text;
  const separator = compact ? '' : ' ';
  const normalized = tokens.map(token => fold(token.text)).join(separator);
  const wanted = fold(normalizeForMatch(needle));
  const start = wanted ? normalized.indexOf(wanted) : -1;
  if (start < 0) return {};
  const end = start + wanted.length;
  let offset = 0;
  const matched = tokens.filter(token => {
    const intersects = offset < end && offset + fold(token.text).length > start;
    offset += fold(token.text).length + separator.length;
    return intersects;
  });
  if (!matched.length) return {};
  const rawStart = matched[0].start;
  const rawEnd = matched[matched.length - 1].end;
  const marks: Record<number, [number, number]> = {};
  offset = 0;
  items.forEach((item, index) => {
    const length = (item.str ?? '').length;
    if (offset < rawEnd && offset + length > rawStart) {
      marks[index] = [Math.max(0, rawStart - offset), Math.min(length, rawEnd - offset)];
    }
    offset += length + 1;
  });
  return marks;
}

export function buildItemTextRenderer(marks: Record<number, [number, number]>) {
  return ({ str, itemIndex }: { str: string; itemIndex: number }): string => {
    const range = marks[itemIndex];
    if (!range) return escapeHtml(str);
    const [start, end] = range;
    return `${escapeHtml(str.slice(0, start))}<mark class="lattice-pdf-mark">${escapeHtml(str.slice(start, end))}</mark>${escapeHtml(str.slice(end))}`;
  };
}

/** Scans PDF pages for the passage, returning 1-based page number and matched substring. */
export async function findPassagePage(
  pdf: PdfDocumentLike,
  text: string,
  options: { maxPages?: number; preferredPage?: number; signal?: { aborted: boolean } } = {}
): Promise<{ page: number; needle: string; marks: Record<number, [number, number]> } | null> {
  const needles = buildNeedles(text);
  if (needles.length) needles.unshift(normalizeForMatch(text));
  if (needles.length === 0) return null;

  const maxPages = Math.min(pdf.numPages, options.maxPages ?? DEFAULT_MAX_PAGES);

  const pages = Array.from({ length: maxPages }, (_, index) => index + 1);
  if (options.preferredPage && options.preferredPage <= pdf.numPages && options.preferredPage > 0) {
    const index = pages.indexOf(options.preferredPage);
    if (index >= 0) pages.splice(index, 1);
    pages.unshift(options.preferredPage);
  }
  for (const pageNumber of pages) {
    if (options.signal?.aborted) return null;

    let pageText: string;
    let items: PdfTextItem[];
    try {
      const page = await pdf.getPage(pageNumber);
      const content = await page.getTextContent();
      items = content.items;
      pageText = items.map((item) => item.str ?? '').join(' ');
    } catch {
      continue;
    }

    const normalized = normalizeForMatch(pageText);
    if (!normalized) continue;

    for (const needle of needles) {
      if (normalized.includes(needle)) {
        return { page: pageNumber, needle, marks: passageItemMarks(items, needle) };
      }
      // Try extraction-tolerant matching before shortening the passage.
      if (needle.replace(/[^\p{L}\p{N}]/gu, '').length < 40) continue;
      const marks = passageItemMarks(items, needle, true);
      if (Object.keys(marks).length) return { page: pageNumber, needle, marks };
    }
  }

  return null;
}
