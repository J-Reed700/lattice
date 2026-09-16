import { useMemo } from 'react';

import { useQuery } from '@tanstack/react-query';

import { useClustersQuery } from '@/hooks/queries/useClustersQuery';
import VaultAPI from '@/lib/api';
import {
  filterLibraryDocuments,
  sortLibraryDocuments,
  useFileBrowserStore,
} from '@/stores/fileBrowserStore';
import type { DocumentMetadata } from '@/types/fileBrowser';

export const LIBRARY_DOCUMENTS_QUERY_KEY = ['library-documents'] as const;

export function useLibraryDocumentsQuery() {
  const query = useQuery<DocumentMetadata[]>({
    queryKey: LIBRARY_DOCUMENTS_QUERY_KEY,
    queryFn: async () => {
      const result = await VaultAPI.listAllDocuments(10_000);
      if (!result.ok) {
        throw new Error(result.error);
      }
      return result.data;
    },
    staleTime: 15_000,
  });
  const sortField = useFileBrowserStore(state => state.sortField);
  const sortOrder = useFileBrowserStore(state => state.sortOrder);
  const searchQuery = useFileBrowserStore(state => state.searchQuery);
  const filterByType = useFileBrowserStore(state => state.filterByType);
  const filterBySource = useFileBrowserStore(state => state.filterBySource);
  const contentSearchMatches = useFileBrowserStore(state => state.contentSearchMatches);
  const customCollections = useFileBrowserStore(state => state.customCollections);
  const scope = useFileBrowserStore(state => state.scope);
  // Themes are only fetched once the rail can show them; until the cache warms
  // a theme scope matches nothing, which is one render, not a wrong answer.
  const themesQuery = useClustersQuery(scope.kind === 'theme');
  const themes = useMemo(() => themesQuery.data ?? [], [themesQuery.data]);

  const documents = useMemo(
    () => sortLibraryDocuments(query.data ?? [], sortField, sortOrder),
    [query.data, sortField, sortOrder]
  );
  const filteredDocuments = useMemo(
    () => filterLibraryDocuments(documents, {
      searchQuery,
      filterByType,
      filterBySource,
      contentSearchMatches,
      customCollections,
      themes,
      scope,
    }),
    [
      contentSearchMatches,
      customCollections,
      documents,
      filterBySource,
      filterByType,
      scope,
      searchQuery,
      themes,
    ]
  );

  return {
    documents,
    filteredDocuments,
    isLoading: query.isLoading,
    error: query.error?.message ?? null,
    refreshFiles: query.refetch,
  };
}
