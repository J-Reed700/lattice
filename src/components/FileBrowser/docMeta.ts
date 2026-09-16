import { type DocumentMetadata } from '../../types/fileBrowser';
import { formatRelativeTime } from '../../utils/dateUtils';

const normalizeToken = (value: string): string => value.trim().toLowerCase();

export const isHttpUrl = (value: string): boolean => /^https?:\/\//i.test(value);

export const isWebDocument = (doc: DocumentMetadata): boolean => {
  const normalizedCategory = normalizeToken(doc.category ?? '').replace(/[_-]+/g, ' ');
  return (
    normalizedCategory.includes('web article') ||
    normalizedCategory === 'web' ||
    isHttpUrl(doc.filePath) ||
    doc.filePath.includes('/.lattice/web-archive/')
  );
};

/**
 * The closed set of type labels the app uses for a document. Keep in sync with
 * `src-tauri/src/features/stats/type_label.rs::type_label` — same words, same order.
 */
export const TYPE_BUCKETS = [
  'PDF',
  'Markdown',
  'Text',
  'Word',
  'Spreadsheet',
  'HTML',
  'Code',
  'Web',
  'Other',
] as const;

export type TypeBucketLabel = (typeof TYPE_BUCKETS)[number] | string;

const TYPE_LABELS: Record<string, string> = {
  pdf: 'PDF',
  md: 'Markdown',
  markdown: 'Markdown',
  txt: 'Text',
  rtf: 'Text',
  doc: 'Word',
  docx: 'Word',
  xlsx: 'Spreadsheet',
  xls: 'Spreadsheet',
  csv: 'Spreadsheet',
  html: 'HTML',
  htm: 'HTML',
  json: 'Code',
  js: 'Code',
  jsx: 'Code',
  ts: 'Code',
  tsx: 'Code',
  py: 'Code',
  rs: 'Code',
  go: 'Code',
  java: 'Code',
};

/**
 * One document → one bucket label. The single rule behind the facet counts and
 * the facet filter, so the two can never disagree.
 */
export const typeBucket = (doc: DocumentMetadata): TypeBucketLabel => {
  if (isWebDocument(doc)) return 'Web';
  const fileType = normalizeToken(doc.fileType ?? '');
  return TYPE_LABELS[fileType] ?? (fileType ? fileType.toUpperCase() : 'Other');
};

/** Short, plain type word for the meta line — never a badge. */
export const typeLabel = (doc: DocumentMetadata): string => {
  if (isWebDocument(doc)) return 'Web';
  const fileType = normalizeToken(doc.fileType ?? '');
  if (!fileType) return 'File';
  if (fileType === 'md' || fileType === 'markdown') return 'Markdown';
  return fileType.toUpperCase();
};

/** "9,800 words · 1h ago · PDF" */
export const metaLine = (doc: DocumentMetadata): string =>
  [
    doc.wordCount > 0 ? `${doc.wordCount.toLocaleString()} words` : null,
    formatRelativeTime(doc.modifiedAt),
    typeLabel(doc),
  ]
    .filter(Boolean)
    .join(' · ');

/** The folder a document sits in, as a basename. */
export const folderName = (filePath: string): string => {
  if (isHttpUrl(filePath)) return 'Web';
  const segments = filePath.replace(/\\/g, '/').split('/').filter(Boolean);
  return segments.length > 1 ? segments[segments.length - 2] : '';
};

export const pathBasename = (value: string): string => {
  const segments = value.replace(/\\/g, '/').split('/').filter(Boolean);
  return segments[segments.length - 1] ?? value;
};
