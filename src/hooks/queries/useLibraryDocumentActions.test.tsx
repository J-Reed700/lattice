import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { conversationKeys } from './conversationKeys';
import { THEMES_QUERY_KEY } from './useClustersQuery';
import { CUSTOM_COLLECTIONS_QUERY_KEY } from './useCustomCollectionsQuery';
import { INDEXING_STATUS_QUERY_KEY } from './useIndexingStatusQuery';
import { resolveDocumentPath, useLibraryDocumentActions } from './useLibraryDocumentActions';
import { LIBRARY_DOCUMENTS_QUERY_KEY } from './useLibraryDocumentsQuery';

const api = vi.hoisted(() => ({
  deleteDocument: vi.fn(),
  reindexFile: vi.fn(),
  removeIndexedFile: vi.fn(),
  getFilePathById: vi.fn(),
  setDocumentSpaceMembership: vi.fn(),
  setDocumentsSpaceMembership: vi.fn(),
}));
vi.mock('@/lib/api', () => ({ default: api, VaultAPI: api }));

const ok = <T,>(data: T) => ({ ok: true as const, data });
const doc = (id: string, filePath = `/vault/${id}.pdf`) => ({ id, filePath, fileName: `${id}.pdf` });

let client: QueryClient;
const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
const invalidated = (queryKey: readonly unknown[]) => client.getQueryState(queryKey)?.isInvalidated ?? false;

/** Seeds every key a library write may refresh, so each test can see which ones it did. */
function seed() {
  for (const key of [
    LIBRARY_DOCUMENTS_QUERY_KEY, CUSTOM_COLLECTIONS_QUERY_KEY, THEMES_QUERY_KEY, INDEXING_STATUS_QUERY_KEY,
    conversationKeys.memberships('a'), conversationKeys.memberships('b'), conversationKeys.lists,
  ]) client.setQueryData(key, []);
}

beforeEach(() => {
  vi.clearAllMocks();
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  seed();
});

describe('library document actions', () => {
  it('deleting documents refreshes every view of the library and reports what failed', async () => {
    api.deleteDocument.mockResolvedValueOnce(ok(undefined)).mockResolvedValueOnce({ ok: false, error: 'in use' });
    const progress = vi.fn();
    const { result } = renderHook(() => useLibraryDocumentActions(), { wrapper });
    let outcome;
    await act(async () => {
      outcome = await result.current.deleteDocuments.mutateAsync({ ids: ['a', 'b'], onProgress: progress });
    });
    expect(outcome).toEqual({ deleted: 1, errors: ['in use'] });
    expect(progress.mock.calls).toEqual([[1], [2]]);
    for (const key of [LIBRARY_DOCUMENTS_QUERY_KEY, CUSTOM_COLLECTIONS_QUERY_KEY, THEMES_QUERY_KEY, conversationKeys.memberships('a')]) {
      expect(invalidated(key)).toBe(true);
    }
  });

  it('a stopped bulk delete leaves the rest and still refreshes', async () => {
    api.deleteDocument.mockResolvedValue(ok(undefined));
    let stop = false;
    const { result } = renderHook(() => useLibraryDocumentActions(), { wrapper });
    await act(async () => {
      await result.current.deleteDocuments.mutateAsync({
        ids: ['a', 'b', 'c'],
        shouldStop: () => stop,
        onProgress: () => { stop = true; },
      });
    });
    expect(api.deleteDocument).toHaveBeenCalledExactlyOnceWith('a');
    expect(invalidated(LIBRARY_DOCUMENTS_QUERY_KEY)).toBe(true);
  });

  it('removing a file from the index resolves its path and refreshes the library and the index status', async () => {
    api.getFilePathById.mockResolvedValue(ok('/vault/resolved.pdf'));
    api.removeIndexedFile.mockResolvedValue(ok(undefined));
    const { result } = renderHook(() => useLibraryDocumentActions(), { wrapper });
    await act(() => result.current.removeFromIndex.mutateAsync(doc('a', 'relative/a.pdf')));
    expect(api.removeIndexedFile).toHaveBeenCalledWith('/vault/resolved.pdf');
    expect(invalidated(LIBRARY_DOCUMENTS_QUERY_KEY)).toBe(true);
    expect(invalidated(INDEXING_STATUS_QUERY_KEY)).toBe(true);
  });

  it('refuses to reindex a document with no file path, without writing', async () => {
    api.getFilePathById.mockResolvedValue({ ok: false, error: 'not found' });
    const { result } = renderHook(() => useLibraryDocumentActions(), { wrapper });
    await act(async () => {
      await expect(result.current.reindex.mutateAsync(doc('a', 'https://example.com/a'))).rejects.toThrow('No file path for this document.');
    });
    expect(api.reindexFile).not.toHaveBeenCalled();
    expect(invalidated(INDEXING_STATUS_QUERY_KEY)).toBe(false);
  });

  it('filing documents in a space refreshes their memberships, not the library', async () => {
    api.setDocumentsSpaceMembership.mockResolvedValue(ok({ status: 'success' }));
    const { result } = renderHook(() => useLibraryDocumentActions(), { wrapper });
    await act(() => result.current.addToSpace.mutateAsync({ documentIds: ['a'], spaceId: 'space_movies' }));
    expect(api.setDocumentsSpaceMembership).toHaveBeenCalledWith(['a'], 'space_movies', true);
    expect(invalidated(conversationKeys.memberships('a'))).toBe(true);
    expect(invalidated(conversationKeys.memberships('b'))).toBe(false);
    expect(invalidated(LIBRARY_DOCUMENTS_QUERY_KEY)).toBe(false);
  });

  it('a refused membership change rejects and refreshes nothing', async () => {
    api.setDocumentSpaceMembership.mockResolvedValue({ ok: false, error: 'Space archived' });
    const { result } = renderHook(() => useLibraryDocumentActions(), { wrapper });
    await act(async () => {
      await expect(result.current.setSpaceMembership.mutateAsync({ documentId: 'a', spaceId: 'space_old', assigned: true }))
        .rejects.toThrow('Space archived');
    });
    expect(invalidated(conversationKeys.memberships('a'))).toBe(false);
  });

  it('uses an absolute path as it is and asks the backend otherwise', async () => {
    expect(await resolveDocumentPath(doc('a'))).toBe('/vault/a.pdf');
    api.getFilePathById.mockResolvedValue(ok('/vault/b.pdf'));
    expect(await resolveDocumentPath(doc('b', 'https://example.com/b'))).toBe('/vault/b.pdf');
    expect(api.getFilePathById).toHaveBeenCalledExactlyOnceWith('b');
  });
});
