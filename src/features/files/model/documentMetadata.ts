import { type DocumentMetadata } from '@/types/fileBrowser';
import { formatRelativeTime } from '@/utils/dateUtils';

const normalizeToken = (value: string): string => value.trim().toLowerCase();

const INTERNAL_FILES_PATTERN = /(?:^|\/)\.lattice\/files\/[^/]+(?:\/(.+))?$/i;
const INTERNAL_WEB_ARCHIVE_PATTERN = /(?:^|\/)\.lattice\/web-archive\/([^/]+)(?:\/(.+))?$/i;

export const isInternalLatticePath = (filePath: string | undefined | null): boolean => {
  if (!filePath) return false;
  const normalized = filePath.replace(/\\/g, '/');
  return INTERNAL_FILES_PATTERN.test(normalized) || INTERNAL_WEB_ARCHIVE_PATTERN.test(normalized);
};

/** Short, plain type word for the meta line — never a badge. */
export const typeLabel = (doc: DocumentMetadata): string => {
  if (doc.web) return 'Web';
  const fileType = normalizeToken(doc.fileType ?? '');
  if (!fileType) return 'File';
  if (fileType === 'md' || fileType === 'markdown') return 'Markdown';
  return fileType.toUpperCase();
};

/**
 * Turn raw internal `.lattice/files/<id>/...` or deep filesystem paths into a
 * concise human-readable relative/source label. Callers should keep the raw
 * path on a `title` tooltip for power users.
 */
export const formatDisplayPath = (filePath: string | undefined | null): string | null => {
  if (!filePath) return null;
  const trimmed = filePath.trim();
  if (!trimmed) return null;

  if (/^https?:\/\//i.test(trimmed)) {
    try {
      return new URL(trimmed).hostname.replace(/^www\./, '');
    } catch {
      return trimmed;
    }
  }

  const normalized = trimmed.replace(/\\/g, '/');

  const webArchiveMatch = INTERNAL_WEB_ARCHIVE_PATTERN.exec(normalized);
  if (webArchiveMatch) {
    const domain = webArchiveMatch[1]?.replace(/^www\./, '');
    return domain ? `Web archive · ${domain}` : 'Web archive';
  }

  const internalFilesMatch = INTERNAL_FILES_PATTERN.exec(normalized);
  if (internalFilesMatch) {
    const rest = internalFilesMatch[1]?.split('/').filter(Boolean) ?? [];
    if (rest.length > 1) {
      return `Imported · ${rest.slice(0, -1).join(' › ')}`;
    }
    return 'Imported';
  }

  const segments = normalized.split('/').filter(Boolean);
  if (segments.length <= 1) return null;
  const parents = segments.slice(Math.max(0, segments.length - 3), -1);
  return parents.join(' › ');
};

export const formatFolderDisplayName = (folderPath: string): string => {
  const normalized = folderPath.replace(/\\/g, '/').replace(/\/+$/, '');
  if (/(?:^|\/)\.lattice\/files(?:\/[^/]+)?$/i.test(normalized)) {
    return 'Imported files';
  }
  if (/(?:^|\/)\.lattice\/web-archive$/i.test(normalized)) {
    return 'Web archive';
  }
  return pathBasename(folderPath);
};

/** "9,800 words · 1h ago · PDF" */
export const metaLine = (
  doc: DocumentMetadata,
  options: { includeType?: boolean } = {},
): string => {
  const { includeType = true } = options;
  return [
    doc.wordCount > 0 ? `${doc.wordCount.toLocaleString()} words` : null,
    formatRelativeTime(doc.modifiedAt),
    includeType ? typeLabel(doc) : null,
  ]
    .filter(Boolean)
    .join(' · ');
};

export const pathBasename = (value: string): string => {
  const segments = value.replace(/\\/g, '/').split('/').filter(Boolean);
  return segments[segments.length - 1] ?? value;
};
