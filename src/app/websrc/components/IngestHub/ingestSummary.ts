import type { CorpusShapeDto } from '@/types';

export interface IngestSummary {
  title: string;
  message?: string;
}

/** "mostly X" only when X is at least 60 % of the library. */
export const MOSTLY_THRESHOLD = 0.6;

/** Below this, a share means nothing — nine documents is not a shape. */
const MIN_TOTAL_FOR_SHAPE = 10;

const PLURALS: Record<IngestNoun, string> = {
  file: 'files',
  URL: 'URLs',
  item: 'items',
};

export type IngestNoun = 'file' | 'URL' | 'item';

/**
 * How a type bucket reads inside "Your library is mostly ___."
 *
 * A bare `${label}s` gives "mostly Markdowns" and "mostly Codes", which is not
 * English. `Other` has no phrasing at all — "mostly Others" says nothing about
 * the library — so it suppresses the sentence.
 * Keep in sync with `TYPE_BUCKETS` in `components/FileBrowser/docMeta.ts`.
 */
const BUCKET_PHRASE: Record<string, string | null> = {
  PDF: 'PDFs',
  Markdown: 'Markdown notes',
  Text: 'text files',
  Word: 'Word documents',
  Spreadsheet: 'spreadsheets',
  HTML: 'HTML files',
  Code: 'code',
  Web: 'web pages',
  Other: null,
};

/** Unknown buckets are uppercased extensions ("EPUB"), which pluralise cleanly. */
const bucketPhrase = (label: string): string | null =>
  label in BUCKET_PHRASE ? BUCKET_PHRASE[label] ?? null : `${label}s`;

/**
 * What to say after an import lands.
 *
 * The count is always true. The second sentence only appears when the library
 * is big enough for a share to mean anything and one type actually dominates —
 * a made-up characterisation is worse than silence.
 */
export function composeIngestSummary(
  noun: IngestNoun,
  count: number,
  shape: CorpusShapeDto | null,
): IngestSummary {
  const title = `Added ${count.toLocaleString()} ${count === 1 ? noun : PLURALS[noun]}`;

  if (!shape || shape.total < MIN_TOTAL_FOR_SHAPE || shape.byType.length === 0) {
    return { title };
  }

  const top = shape.byType.reduce((best, bucket) => (bucket.count > best.count ? bucket : best));
  if (top.count / shape.total < MOSTLY_THRESHOLD) {
    return { title };
  }

  const phrase = bucketPhrase(top.type);
  if (!phrase) {
    return { title };
  }

  return { title, message: `Your library is mostly ${phrase}.` };
}
