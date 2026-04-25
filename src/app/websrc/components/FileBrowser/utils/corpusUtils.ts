import { type DocumentMetadata, type SortField, type SortOrder } from '../../../types/fileBrowser';
import { getDateGroup } from '../../../utils/dateUtils';

export type GroupByMode = 'none' | 'date' | 'type' | 'folder' | 'source';

export interface CorpusGroup {
  key: string;
  label: string;
  count: number;
  documents: DocumentMetadata[];
}

export type CorpusListItem =
  | { type: 'header'; groupKey: string; groupLabel: string; groupCount: number }
  | { type: 'doc'; groupKey: string; doc: DocumentMetadata };

export const normalizeToken = (value: string): string => value.trim().toLowerCase();
export const isHttpUrl = (value: string): boolean => /^https?:\/\//i.test(value);

const NON_TEXT_FILE_TYPES = new Set([
  'png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'ico',
  'mp3', 'wav', 'flac', 'ogg', 'm4a',
  'mp4', 'mkv', 'mov', 'avi', 'webm',
  'zip', 'rar', '7z', 'tar', 'gz',
]);

export const isWebDocument = (doc: DocumentMetadata): boolean => {
  if (isHttpUrl(doc.filePath)) return true;
  if (doc.filePath.includes('/.recall/web-archive/')) return true;
  const normalizedCategory = normalizeToken(doc.category).replace(/[_-]+/g, ' ');
  return normalizedCategory.includes('web article') || normalizedCategory === 'web';
};

export const matchesTypeFilter = (doc: DocumentMetadata, activeFilter: string | null): boolean => {
  if (!activeFilter) return true;
  const normalizedCategory = normalizeToken(doc.category).replace(/[_-]+/g, ' ');
  const normalizedFileType = normalizeToken(doc.fileType ?? '');
  return (
    normalizedCategory.includes(activeFilter) ||
    normalizedFileType === activeFilter ||
    (activeFilter === 'web' && normalizedCategory.includes('web article'))
  );
};

export const canSearchByContent = (doc: DocumentMetadata): boolean => {
  const normalizedFileType = normalizeToken(doc.fileType ?? '');
  if (isHttpUrl(doc.filePath)) return false;
  return !NON_TEXT_FILE_TYPES.has(normalizedFileType);
};

const normalizePath = (value: string): string => value.replace(/\\/g, '/');

// Lifted from legacy TreeView.tsx:174-197 so Folder group-by behaves
// identically to the old Collections-mode project clusters.
export const extractProjectName = (path: string): string => {
  if (isHttpUrl(path) || path.includes('/.recall/web-archive/')) {
    return 'Web Imports';
  }
  const normalized = normalizePath(path);
  const segments = normalized.split('/').filter(Boolean);

  if (segments.length === 0) return 'Library';

  if (segments[0] === 'Users' && segments.length >= 3) {
    if (['Documents', 'Desktop', 'Downloads', 'Code'].includes(segments[2]) && segments.length >= 4) {
      return segments[3];
    }
    return segments[2];
  }

  if (segments[0] === 'home' && segments.length >= 3) {
    if (['documents', 'desktop', 'downloads', 'code'].includes(segments[2].toLowerCase()) && segments.length >= 4) {
      return segments[3];
    }
    return segments[2];
  }

  return segments[0];
};

export const getTypeGroupLabel = (doc: DocumentMetadata): string => {
  const normalizedCategory = normalizeToken(doc.category).replace(/[_-]+/g, ' ');
  const normalizedFileType = normalizeToken(doc.fileType ?? '');

  if (isWebDocument(doc)) return 'Web';
  if (normalizedCategory.includes('pdf') || normalizedFileType === 'pdf') return 'PDFs';
  if (['md', 'markdown', 'txt', 'rtf', 'doc', 'docx'].includes(normalizedFileType)) return 'Notes';
  if (['js', 'jsx', 'ts', 'tsx', 'py', 'rs', 'go', 'java', 'cpp', 'c', 'h', 'html', 'css', 'json', 'xml', 'yml', 'yaml'].includes(normalizedFileType)) return 'Code';
  if (['mp3', 'wav', 'flac', 'ogg', 'm4a', 'mp4', 'mkv', 'mov', 'avi', 'webm', 'jpg', 'jpeg', 'png', 'gif', 'webp', 'svg'].includes(normalizedFileType)) return 'Media';
  return 'Other';
};

export const sortDocuments = (
  documents: DocumentMetadata[],
  sortField: SortField,
  sortOrder: SortOrder,
): DocumentMetadata[] => {
  const direction = sortOrder === 'asc' ? 1 : -1;
  const toComparable = (doc: DocumentMetadata): string | number => {
    if (sortField === 'name') return doc.fileName.toLowerCase();
    if (sortField === 'size') return doc.wordCount;
    if (sortField === 'modified') return new Date(doc.modifiedAt).getTime();
    return doc.fileType.toLowerCase();
  };

  return [...documents]
    .map((doc, index) => ({ doc, index, value: toComparable(doc) }))
    .sort((a, b) => {
      if (a.value < b.value) return -1 * direction;
      if (a.value > b.value) return 1 * direction;
      return a.index - b.index;
    })
    .map(({ doc }) => doc);
};

const DATE_GROUP_ORDER = ['Today', 'Yesterday', 'This Week', 'Last Week', 'This Month', 'Older'] as const;

export const groupDocuments = (
  docs: DocumentMetadata[],
  mode: GroupByMode,
): CorpusGroup[] => {
  if (mode === 'none' || docs.length === 0) {
    return [{ key: '__all', label: '', count: docs.length, documents: docs }];
  }

  const buckets = new Map<string, DocumentMetadata[]>();
  for (const doc of docs) {
    let key: string;
    if (mode === 'date') {
      key = getDateGroup(doc.modifiedAt);
    } else if (mode === 'type') {
      key = getTypeGroupLabel(doc);
    } else if (mode === 'folder') {
      key = extractProjectName(doc.filePath);
    } else {
      key = isWebDocument(doc) ? 'Web Imports' : 'Unsourced';
    }
    if (!buckets.has(key)) buckets.set(key, []);
    const bucket = buckets.get(key);
    if (bucket) bucket.push(doc);
  }

  if (mode === 'date') {
    const out: CorpusGroup[] = [];
    for (const label of DATE_GROUP_ORDER) {
      const bucket = buckets.get(label);
      if (bucket && bucket.length > 0) {
        out.push({ key: label, label, count: bucket.length, documents: bucket });
      }
    }
    return out;
  }

  return Array.from(buckets.entries())
    .sort(([a], [b]) => a.localeCompare(b, undefined, { sensitivity: 'base' }))
    .map(([key, bucket]) => ({ key, label: key, count: bucket.length, documents: bucket }));
};

export const flattenToListItems = (groups: CorpusGroup[], showHeaders: boolean): CorpusListItem[] => {
  const items: CorpusListItem[] = [];
  for (const group of groups) {
    if (showHeaders && group.label) {
      items.push({
        type: 'header',
        groupKey: group.key,
        groupLabel: group.label,
        groupCount: group.count,
      });
    }
    for (const doc of group.documents) {
      items.push({ type: 'doc', groupKey: group.key, doc });
    }
  }
  return items;
};
