import { queryOptions } from '@tanstack/react-query';

import VaultAPI, { type ApiClient } from '@/lib/api';
import { assetUrl } from '@/utils/assetUrl';

/** Paths inside `~/.lattice/files` match `assetProtocol.scope` in `tauri.conf.json`. */
export function isScopedAssetPath(path: string): boolean {
  return /(?:^|[\\/])\.lattice[\\/]files[\\/]/i.test(path);
}

/** True only when Tauri's real asset protocol bridge (`convertFileSrc`) is available. */
export function hasTauriAssetProtocol(): boolean {
  if (typeof window === 'undefined') return false;
  const internals = (window as { __TAURI_INTERNALS__?: { convertFileSrc?: unknown } }).__TAURI_INTERNALS__;
  return typeof internals?.convertFileSrc === 'function';
}

/** A source file's text, for the readers that render text, code and saved pages. */
export const readSourceText = (path: string, api: ApiClient = VaultAPI) => api.readFileContent(path);

/** A source file's bytes, for the readers that parse a binary format (PDF, DOCX). */
export const readSourceBytes = async (path: string, api: ApiClient = VaultAPI) => {
  if (hasTauriAssetProtocol() && api === VaultAPI && path && isScopedAssetPath(path)) {
    try {
      const response = await fetch(assetUrl(path));
      if (response.ok) {
        return { ok: true as const, data: new Uint8Array(await response.arrayBuffer()) };
      }
    } catch {
      // Fall back to IPC when the asset protocol is unavailable or rejects the path.
    }
  }
  return api.readFileBytes(path);
};

/** The saved web article the archive holds for a file, or null when it holds none. */
export const readSavedWebArticle = (path: string, api: ApiClient = VaultAPI) => api.savedWebArticle(path);

export const readingKeys = {
  all: ['reading'] as const,
  /** A file's text, by path. */
  fileText: (path: string) => ['reading', 'file-text', path] as const,
  /** Whether the web archive holds a file as a saved article, by path. */
  savedWebArticle: (path: string) => ['reading', 'saved-web-article', path] as const,
  /** A saved web page, or its markdown fallback. */
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
