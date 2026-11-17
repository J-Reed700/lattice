import { type DocumentMetadata } from '@/types/fileBrowser';
import { isHttpUrl } from '@/utils/sourcePreview';

export const isAbsolutePath = (path: string): boolean =>
  path.startsWith('/') || /^[A-Za-z]:[\\/]/.test(path);

export const normalizeSearchToken = (value: string): string => value.trim().toLowerCase();

const NON_TEXT_FILE_TYPES = new Set([
  'png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'ico',
  'mp3', 'wav', 'flac', 'ogg', 'm4a',
  'mp4', 'mkv', 'mov', 'avi', 'webm',
  'zip', 'rar', '7z', 'tar', 'gz',
]);

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
