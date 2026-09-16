/**
 * Locating a cited passage inside the file it came from (BRIEF rank 1).
 *
 * Chunk boundaries are byte offsets from an indexing run that may predate the
 * file's current contents, so nothing here pretends to be exact. It matches
 * fuzzily, degrades to an ordinal-derived scroll position, and hands the caller
 * a tier so the UI can say which of the three happened.
 */

import type { PassageLocator, SourceWithMetadata } from '../../types/conversation';

/**
 * Collapse whitespace, fold case, drop punctuation that OCR/markdown mangles.
 *
 * Apostrophes, quotes and hyphens survive *inside* words (`well-known`,
 * `don't`) because dropping them would split a word into two false tokens, but
 * a token made only of them is noise — a stray em dash between sentences must
 * not stop a chunk matching its own source.
 */
export function normalizeForMatch(value: string): string {
  return value
    .toLowerCase()
    .replace(/[\u2018\u2019\u201a\u201b]/g, "'")
    .replace(/[\u201c\u201d\u201e\u201f]/g, '"')
    .replace(/[\u2010-\u2015]/g, '-')
    .replace(/[^\p{L}\p{N}'"-]+/gu, ' ')
    .split(' ')
    .filter((token) => /[\p{L}\p{N}]/u.test(token))
    .join(' ')
    .trim();
}

/**
 * Progressive needles from a chunk excerpt: 160, 80, then 40 normalized chars.
 *
 * Longest first, so a confident match wins before a short one that could hit
 * boilerplate. Needles shorter than the previous tier are dropped — a 40-char
 * needle from a 45-char chunk is the same needle twice.
 */
export function buildNeedles(text: string): string[] {
  const normalized = normalizeForMatch(text);
  if (!normalized) return [];

  const needles: string[] = [];
  for (const length of [160, 80, 40]) {
    const candidate = normalized.slice(0, length);
    if (candidate.length < 12) continue;
    if (!needles.includes(candidate)) needles.push(candidate);
  }
  return needles;
}

/** Build a locator from a citation payload. */
export function locatorFromSource(
  source: SourceWithMetadata,
  chunkId?: string
): PassageLocator {
  const matchedExcerpt = chunkId
    ? source.chunkExcerpts?.find((chunk) => chunk.chunkId === chunkId)
    : undefined;

  const text = matchedExcerpt?.excerpt ?? source.excerpt ?? source.content ?? '';
  const section = matchedExcerpt?.section ?? source.section;

  return {
    text,
    chunkIndex: matchedExcerpt?.chunkIndex ?? source.chunkIndex,
    highlights: matchedExcerpt?.highlights ?? source.highlights,
    label: formatSourceLocation({ section, chunkId: chunkId ?? source.chunkId }) ?? undefined,
    chunkId: chunkId ?? source.chunkId,
  };
}

const TIMESTAMP_SECTION = /^(\d{1,2}):(\d{2})(?::(\d{2}))?\s*[–-]\s*\d{1,2}:\d{2}/;

/** True when `section` reads as a transcript timestamp range. */
export function isTimestampSection(section?: string): boolean {
  if (!section) return false;
  return TIMESTAMP_SECTION.test(section.trim());
}

/**
 * Seconds for an AudioViewer seek, from a timestamp section.
 * Null when the section is not a timestamp.
 */
export function timestampSectionStartSeconds(section?: string): number | null {
  if (!section) return null;
  const match = TIMESTAMP_SECTION.exec(section.trim());
  if (!match) return null;

  const first = Number(match[1]);
  const second = Number(match[2]);
  const third = match[3] === undefined ? null : Number(match[3]);

  // "1:02:30" is h:mm:ss; "12:40" is mm:ss.
  return third === null ? first * 60 + second : first * 3600 + second * 60 + third;
}

/**
 * Human label for where a passage lives.
 *
 * Only ever reports what is actually known: a resolved page, a transcript
 * timestamp, or a heading. A chunk ordinal is not a location, so it returns
 * null rather than "Chunk 7".
 */
export function formatSourceLocation(
  source: Pick<SourceWithMetadata, 'section' | 'chunkId'>,
  resolvedPage?: number
): string | null {
  if (typeof resolvedPage === 'number' && Number.isFinite(resolvedPage) && resolvedPage > 0) {
    return `p. ${resolvedPage}`;
  }

  const section = source.section?.trim();
  if (section) {
    if (isTimestampSection(section)) return section;
    return `§ ${section}`;
  }

  if (source.chunkId) {
    const remembered = recallLocation(source.chunkId);
    if (remembered) return remembered;
  }

  return null;
}

/**
 * Fallback scroll ratio when the text cannot be found.
 *
 * Deliberately conservative — `chunkIndex / (chunkIndex + 4)` never reaches
 * the end of a document and never claims precision. The UI labels whatever it
 * lands on "approximate".
 */
export function approximateScrollRatio(chunkIndex: number): number {
  if (!Number.isFinite(chunkIndex) || chunkIndex <= 0) return 0;
  const ratio = chunkIndex / (chunkIndex + 4);
  return Math.min(0.95, Math.max(0, ratio));
}

/**
 * In-memory memo of locations a viewer resolved (PDF page numbers).
 *
 * Not persisted: a page number costs one scan to recompute and persisting it
 * would need a schema this track does not own.
 */
const resolvedLocations = new Map<string, string>();

export function rememberLocation(chunkId: string, label: string): void {
  if (!chunkId || !label) return;
  resolvedLocations.set(chunkId, label);
}

export function recallLocation(chunkId: string): string | undefined {
  return resolvedLocations.get(chunkId);
}

/** Test seam: drop every remembered location. */
export function forgetAllLocations(): void {
  resolvedLocations.clear();
}
