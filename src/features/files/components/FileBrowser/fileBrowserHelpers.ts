import { isHttpUrl, typeBucket } from '@/features/files/model/documentMetadata';
import { type DocumentMetadata } from '@/types/fileBrowser';

export const isAbsolutePath = (path: string): boolean =>
  path.startsWith('/') || /^[A-Za-z]:[\\/]/.test(path);

const normalizeCategory = (category: string): string =>
  category.toLowerCase().replace(/[_-]+/g, ' ').trim();

export const normalizeSearchToken = (value: string): string => value.trim().toLowerCase();

const NON_TEXT_FILE_TYPES = new Set([
  'png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'ico',
  'mp3', 'wav', 'flac', 'ogg', 'm4a',
  'mp4', 'mkv', 'mov', 'avi', 'webm',
  'zip', 'rar', '7z', 'tar', 'gz',
]);

/**
 * Which documents the content search reads. Must stay in step with
 * `filterLibraryDocuments`'s `matchesType`, or a facet would narrow the list
 * while the content search kept reading a different set.
 */
export const matchesTypeFilter = (doc: DocumentMetadata, activeFilter: string | null): boolean => {
  if (!activeFilter) {
    return true;
  }

  const normalizedCategory = normalizeCategory(doc.category);
  const normalizedFileType = (doc.fileType ?? '').toLowerCase();

  return (
    typeBucket(doc).toLowerCase() === activeFilter ||
    normalizedCategory.includes(activeFilter) ||
    normalizedFileType === activeFilter ||
    (activeFilter === 'web' && normalizedCategory.includes('web article'))
  );
};

export const canSearchByContent = (doc: DocumentMetadata): boolean => {
  const normalizedFileType = (doc.fileType ?? '').toLowerCase().trim();
  if (isHttpUrl(doc.filePath)) {
    return false;
  }
  return !NON_TEXT_FILE_TYPES.has(normalizedFileType);
};

export const errorMessage = (error: unknown): string => (error instanceof Error ? error.message : String(error));

export const plural = (count: number, noun: string): string =>
  `${count.toLocaleString()} ${noun}${count === 1 ? '' : 's'}`;
