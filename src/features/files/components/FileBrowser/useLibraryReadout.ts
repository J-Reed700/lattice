import { useMemo } from 'react';

import { useCorpusIdentity } from '@/features/files/hooks/useCorpusIdentity';
import { useIndexedFoldersQuery } from '@/features/files/hooks/useIndexedFoldersQuery';
import { isWebDocument, pathBasename } from '@/features/files/model/documentMetadata';
import {
  type CustomCollection,
  type DocumentMetadata,
  type LibraryScope,
  type SourceFilter,
} from '@/types/fileBrowser';

import { plural } from './fileBrowserHelpers';

import type { useLibraryThemes } from './useLibraryThemes';

/** What the Library says about itself: source counts, type facets, the header line and the scope heading. */
export function useLibraryReadout({
  documents,
  customCollections,
  scope,
  filterBySource,
  themes,
}: {
  documents: DocumentMetadata[];
  customCollections: CustomCollection[];
  scope: LibraryScope;
  filterBySource: SourceFilter;
  themes: ReturnType<typeof useLibraryThemes>['themes'];
}) {

  const indexedFoldersQuery = useIndexedFoldersQuery();

  const indexedFolders = useMemo(() => indexedFoldersQuery.data ?? [], [indexedFoldersQuery.data]);

  const sourceCounts = useMemo(() => {
    let web = 0;
    let local = 0;
    for (const doc of documents) {
      if (isWebDocument(doc)) {
        web += 1;
      } else {
        local += 1;
      }
    }
    return { all: documents.length, web, local };
  }, [documents]);

  // Counted over the unfiltered list on purpose: the facet line is a readout of
  // the whole library, so the numbers do not move while a facet is active.
  const identity = useCorpusIdentity(documents);

  const headerMeta = useMemo(
    () =>
      [
        plural(documents.length, 'document'),
        `${sourceCounts.local.toLocaleString()} local`,
        `${sourceCounts.web.toLocaleString()} web`,
        plural(indexedFolders.length, 'folder'),
      ].join(' · '),
    [documents.length, indexedFolders.length, sourceCounts.local, sourceCounts.web]
  );

  const scopeName = useMemo(() => {
    if (scope.kind === 'folder') {
      return pathBasename(scope.path);
    }
    if (scope.kind === 'collection') {
      return customCollections.find((collection) => collection.id === scope.id)?.name ?? 'Collection';
    }
    if (scope.kind === 'theme') {
      return themes.find((theme) => theme.id === scope.id)?.label ?? 'Theme';
    }
    if (filterBySource === 'web') return 'Web';
    if (filterBySource === 'local') return 'Local';
    return 'All documents';
  }, [customCollections, filterBySource, scope, themes]);

  return { sourceCounts, identity, headerMeta, scopeName };
}
