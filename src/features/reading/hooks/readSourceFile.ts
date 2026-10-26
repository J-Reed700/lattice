import { queryOptions } from '@tanstack/react-query';

import VaultAPI, { type ApiClient } from '@/lib/api';

/** A source file's text, for the readers that render text, code and saved pages. */
export const readSourceText = (path: string, api: ApiClient = VaultAPI) => api.readFileContent(path);

/** A source file's bytes, for the readers that parse a binary format (PDF, DOCX). */
export const readSourceBytes = (path: string, api: ApiClient = VaultAPI) => api.readFileBytes(path);

export const readingKeys = {
  all: ['reading'] as const,
  /** A file's text, by path. */
  fileText: (path: string) => ['reading', 'file-text', path] as const,
  /** A saved web page (or its markdown fallback) with its archive metadata. */
  archivedPage: (path: string) => ['reading', 'archived-page', path] as const,
  /** A DOCX converted to sanitized HTML. */
  docxHtml: (path: string) => ['reading', 'docx-html', path] as const,
  /** Where a document lives now; the indexed path goes stale when it moves. */
  documentPath: (documentId: string) => ['reading', 'document-path', documentId] as const,
};

/**
 * A file's text. A failed read is the reader's error, shown at once rather
 * than after retries; a reopened file shows what was read last while it is
 * read again, so edits made elsewhere still come through.
 */
export const sourceTextQueryOptions = (path: string, api: ApiClient = VaultAPI) =>
  queryOptions({
    queryKey: readingKeys.fileText(path),
    queryFn: async () => {
      const result = await readSourceText(path, api);
      if (!result.ok) throw new Error(result.error || 'Failed to load file');
      return result.data;
    },
    retry: false,
    staleTime: 0,
  });
