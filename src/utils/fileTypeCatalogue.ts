import { FILE_TYPES } from '@/lib/bindings';

export type FileFormat = (typeof FILE_TYPES)[number];
export type FileCategory = FileFormat['category'];
export type ExtractionRoute = NonNullable<FileFormat['route']>;

const BY_EXTENSION = new Map<string, FileFormat>(FILE_TYPES.map((format): [string, FileFormat] => [format.extension, format]));

/** The catalogue entry for an extension without its dot, in any case. */
export function formatOfExtension(extension: string): FileFormat | undefined {
  return BY_EXTENSION.get(extension.toLowerCase());
}

/** A format is indexable exactly when the backend has a route to extract it. */
export function isIndexableFormat(format: FileFormat | undefined): boolean {
  return format !== undefined && format.route !== null;
}

export const INDEXABLE_EXTENSIONS: readonly string[] = FILE_TYPES.filter(format => format.route !== null).map(
  format => format.extension,
);
