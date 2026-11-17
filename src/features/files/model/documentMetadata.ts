import { type DocumentMetadata } from '@/types/fileBrowser';
import { formatRelativeTime } from '@/utils/dateUtils';

const normalizeToken = (value: string): string => value.trim().toLowerCase();

/** Short, plain type word for the meta line — never a badge. */
export const typeLabel = (doc: DocumentMetadata): string => {
  if (doc.web) return 'Web';
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

export const pathBasename = (value: string): string => {
  const segments = value.replace(/\\/g, '/').split('/').filter(Boolean);
  return segments[segments.length - 1] ?? value;
};
