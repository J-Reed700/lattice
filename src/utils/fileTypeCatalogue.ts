import { FILE_TYPES } from '@/lib/bindings';

export type FileCategory = 'document' | 'code' | 'image' | 'video' | 'audio' | 'archive' | 'data' | 'binary';

export type ExtractionRoute =
  | 'plain_text'
  | 'pdf'
  | 'docx'
  | 'rtf'
  | 'odt'
  | 'xlsx'
  | 'pptx'
  | 'csv'
  | 'code'
  | 'audio';

export interface FileFormat {
  extension: string;
  mimeType: string;
  category: FileCategory;
  route: ExtractionRoute | null;
}

const FORMATS: readonly FileFormat[] = FILE_TYPES;

const BY_EXTENSION = new Map<string, FileFormat>(FORMATS.map(format => [format.extension, format]));

/** The catalogue entry for an extension without its dot, in any case. */
export function formatOfExtension(extension: string): FileFormat | undefined {
  return BY_EXTENSION.get(extension.toLowerCase());
}

/** A format is indexable exactly when the backend has a route to extract it. */
export function isIndexableFormat(format: FileFormat | undefined): boolean {
  return format !== undefined && format.route !== null;
}

export const INDEXABLE_EXTENSIONS: readonly string[] = FORMATS.filter(isIndexableFormat).map(
  format => format.extension,
);
