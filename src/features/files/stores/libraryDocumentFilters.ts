import { typeBucket } from '@/features/files/model/documentMetadata';
import {
  type CustomCollection,
  type LibraryScope,
  type SourceFilter,
  type SortField,
  type SortOrder,
  type DocumentMetadata,
} from '@/types/fileBrowser';

export const sortLibraryDocuments = (
  documents: DocumentMetadata[],
  sortField: SortField,
  sortOrder: SortOrder
): DocumentMetadata[] => {
  const direction = sortOrder === 'asc' ? 1 : -1;
  const toComparable = (doc: DocumentMetadata): string | number => {
    if (sortField === 'name') {
      return doc.fileName.toLowerCase();
    }
    if (sortField === 'size') {
      return doc.wordCount;
    }
    if (sortField === 'modified') {
      return new Date(doc.modifiedAt).getTime();
    }
    return doc.fileType.toLowerCase();
  };

  return documents
    .map((doc, index) => ({ doc, index, value: toComparable(doc) }))
    .sort((a, b) => {
      if (a.value < b.value) return -1 * direction;
      if (a.value > b.value) return 1 * direction;
      return a.index - b.index;
    })
    .map(({ doc }) => doc);
};

export const normalizeToken = (value: string): string => value.trim().toLowerCase();
const isHttpUrl = (value: string): boolean => /^https?:\/\//i.test(value);
const isWebDocument = (doc: DocumentMetadata): boolean => {
  const normalizedCategory = normalizeToken(doc.category).replace(/[_-]+/g, ' ');
  return (
    normalizedCategory.includes('web article') ||
    normalizedCategory === 'web' ||
    isHttpUrl(doc.filePath) ||
    doc.filePath.includes('/.lattice/web-archive/')
  );
};

const collectDescendantCollectionIds = (
  collections: CustomCollection[],
  collectionId: string
): Set<string> => {
  const descendants = new Set<string>();
  const queue = [collectionId];

  while (queue.length > 0) {
    const current = queue.shift();
    if (!current) {
      continue;
    }

    for (const collection of collections) {
      if (collection.parentId === current && !descendants.has(collection.id)) {
        descendants.add(collection.id);
        queue.push(collection.id);
      }
    }
  }

  return descendants;
};

export interface LibraryFilterState {
  searchQuery: string;
  filterByType: string | null;
  filterBySource: SourceFilter;
  contentSearchMatches: Set<string>;
  customCollections: CustomCollection[];
  /**
   * Themes from the most recent clustering run, for `scope.kind === 'theme'`.
   * Optional because themes live in React Query, not in this store — callers
   * that never scope to a theme (and the store's own tests) omit it.
   */
  themes?: Array<{ id: string; memberDocumentIds: string[] }>;
  scope: LibraryScope;
}

const normalizeFolderPath = (value: string): string => {
  const normalized = value.replace(/\\/g, '/').replace(/\/+$/, '');
  return normalized.toLowerCase();
};

const isInsideFolder = (filePath: string, folderPath: string): boolean => {
  const file = filePath.replace(/\\/g, '/').toLowerCase();
  const folder = normalizeFolderPath(folderPath);
  return folder.length > 0 && file.startsWith(`${folder}/`);
};

export const filterLibraryDocuments = (
  documents: DocumentMetadata[],
  state: LibraryFilterState
): DocumentMetadata[] => {
  const query = normalizeToken(state.searchQuery);
  const normalizedFilter = state.filterByType ? normalizeToken(state.filterByType) : null;

  let scopedDocIds: Set<string> | null = null;
  if (state.scope.kind === 'collection') {
    const inScope = new Set<string>([
      state.scope.id,
      ...collectDescendantCollectionIds(state.customCollections, state.scope.id),
    ]);
    scopedDocIds = new Set<string>();
    for (const collection of state.customCollections) {
      if (inScope.has(collection.id)) {
        for (const docId of collection.documentIds) {
          scopedDocIds.add(docId);
        }
      }
    }
  }
  if (state.scope.kind === 'theme') {
    const scopeId = state.scope.id;
    const theme = state.themes?.find((candidate) => candidate.id === scopeId);
    scopedDocIds = new Set<string>(theme?.memberDocumentIds ?? []);
  }
  const scopedFolder = state.scope.kind === 'folder' ? state.scope.path : null;

  return documents.filter(doc => {
    const normalizedCategory = normalizeToken(doc.category).replace(/[_-]+/g, ' ');
    const normalizedFileType = normalizeToken(doc.fileType);
    const normalizedFileName = normalizeToken(doc.fileName);
    const matchesQuery = query
      ? normalizedFileName.includes(query) || state.contentSearchMatches.has(doc.id)
      : true;
    const matchesType = normalizedFilter
      ? normalizeToken(typeBucket(doc)) === normalizedFilter ||
        normalizedCategory.includes(normalizedFilter) ||
        normalizedFileType === normalizedFilter ||
        (normalizedFilter === 'web' && normalizedCategory.includes('web article'))
      : true;
    const matchesSource =
      state.filterBySource === 'all' ||
      (state.filterBySource === 'web' && isWebDocument(doc)) ||
      (state.filterBySource === 'local' && !isWebDocument(doc));
    const matchesScope =
      (!scopedDocIds || scopedDocIds.has(doc.id)) &&
      (!scopedFolder || isInsideFolder(doc.filePath, scopedFolder));
    return matchesQuery && matchesType && matchesSource && matchesScope;
  });
};
