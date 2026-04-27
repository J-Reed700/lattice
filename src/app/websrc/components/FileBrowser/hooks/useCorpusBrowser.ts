import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import VaultAPI from '../../../lib/api';
import {
  selectFilteredDocuments,
  useFileBrowserStore,
} from '../../../stores/fileBrowserStore';
import {
  type CustomCollection,
  type DocumentMetadata,
  type SortField,
  type SortOrder,
} from '../../../types/fileBrowser';
import {
  type CorpusGroup,
  type CorpusListItem,
  type GroupByMode,
  canSearchByContent,
  flattenToListItems,
  groupDocuments,
  matchesTypeFilter,
  normalizeToken,
  sortDocuments,
} from '../utils/corpusUtils';

export type { CorpusGroup, CorpusListItem, GroupByMode } from '../utils/corpusUtils';
export { extractProjectName } from '../utils/corpusUtils';

export type DensityMode = 'list' | 'detail';

export interface UseCorpusBrowserResult {
  groupBy: GroupByMode;
  setGroupBy: (mode: GroupByMode) => void;
  sortField: SortField;
  sortOrder: SortOrder;
  setSort: (field: SortField, order: SortOrder) => void;
  density: DensityMode;
  setDensity: (density: DensityMode) => void;

  filteredDocuments: DocumentMetadata[];
  sortedDocuments: DocumentMetadata[];
  groups: CorpusGroup[];
  listItems: CorpusListItem[];

  normalizedSearchQuery: string;
  activeFilter: string | null;
  isContentSearchLoading: boolean;
  clearAllFilters: () => void;
  hasActiveFilters: boolean;

  activeCollectionId: string | null;
  setActiveCollectionId: (id: string | null) => void;

  customCollections: CustomCollection[];
}

const COLLECTION_STORAGE_KEY = 'lattice:file-browser-active-collection-id:v1';
const GROUP_BY_STORAGE_KEY = 'lattice:file-browser-group-by:v1';
const SORT_STORAGE_KEY = 'lattice:file-browser-sort:v1';
const DENSITY_STORAGE_KEY = 'lattice:file-browser-density:v1';

const loadGroupBy = (): GroupByMode => {
  if (typeof window === 'undefined') return 'date';
  try {
    const raw = window.localStorage.getItem(GROUP_BY_STORAGE_KEY);
    if (raw === 'none' || raw === 'date' || raw === 'type' || raw === 'folder' || raw === 'source') {
      return raw;
    }
  } catch {
    // ignore
  }
  return 'date';
};

const loadSort = (): { field: SortField; order: SortOrder } => {
  if (typeof window === 'undefined') return { field: 'modified', order: 'desc' };
  try {
    const raw = window.localStorage.getItem(SORT_STORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as { field?: string; order?: string };
      const field = parsed.field;
      const order = parsed.order;
      if (
        (field === 'name' || field === 'size' || field === 'modified' || field === 'type') &&
        (order === 'asc' || order === 'desc')
      ) {
        return { field, order };
      }
    }
  } catch {
    // ignore
  }
  return { field: 'modified', order: 'desc' };
};

const loadDensity = (): DensityMode => {
  if (typeof window === 'undefined') return 'list';
  try {
    const raw = window.localStorage.getItem(DENSITY_STORAGE_KEY);
    if (raw === 'list' || raw === 'detail') return raw;
  } catch {
    // ignore
  }
  return 'list';
};

const loadActiveCollectionId = (): string | null => {
  if (typeof window === 'undefined') return null;
  try {
    return window.localStorage.getItem(COLLECTION_STORAGE_KEY);
  } catch {
    return null;
  }
};

export function useCorpusBrowser(): UseCorpusBrowserResult {
  const documents = useFileBrowserStore((state) => state.documents);
  const filteredDocuments = useFileBrowserStore(selectFilteredDocuments);
  const customCollections = useFileBrowserStore((state) => state.customCollections);
  const searchQuery = useFileBrowserStore((state) => state.searchQuery);
  const filterByType = useFileBrowserStore((state) => state.filterByType);
  const filterBySource = useFileBrowserStore((state) => state.filterBySource);
  const setSearchQuery = useFileBrowserStore((state) => state.setSearchQuery);
  const setFilterByType = useFileBrowserStore((state) => state.setFilterByType);
  const setFilterBySource = useFileBrowserStore((state) => state.setFilterBySource);
  const isContentSearchLoading = useFileBrowserStore((state) => state.isContentSearchLoading);
  const setContentSearchMatches = useFileBrowserStore((state) => state.setContentSearchMatches);
  const setContentSearchLoading = useFileBrowserStore((state) => state.setContentSearchLoading);
  const clearContentSearch = useFileBrowserStore((state) => state.clearContentSearch);

  // Content search scheduling refs — lifted AS-IS from legacy FileBrowser.tsx:320-406.
  const contentCacheRef = useRef<Map<string, string>>(new Map());
  const contentReadFailuresRef = useRef<Set<string>>(new Set());
  const contentSearchSequenceRef = useRef(0);

  // Local UI state with localStorage persistence.
  const [groupBy, setGroupByState] = useState<GroupByMode>(loadGroupBy);
  const [sort, setSortState] = useState<{ field: SortField; order: SortOrder }>(loadSort);
  const [density, setDensityState] = useState<DensityMode>(loadDensity);
  const [activeCollectionId, setActiveCollectionIdState] = useState<string | null>(loadActiveCollectionId);

  const setGroupBy = useCallback((mode: GroupByMode) => {
    setGroupByState(mode);
    try {
      if (typeof window !== 'undefined') {
        window.localStorage.setItem(GROUP_BY_STORAGE_KEY, mode);
      }
    } catch {
      // ignore
    }
  }, []);

  const setSort = useCallback((field: SortField, order: SortOrder) => {
    setSortState({ field, order });
    try {
      if (typeof window !== 'undefined') {
        window.localStorage.setItem(SORT_STORAGE_KEY, JSON.stringify({ field, order }));
      }
    } catch {
      // ignore
    }
  }, []);

  const setDensity = useCallback((value: DensityMode) => {
    setDensityState(value);
    try {
      if (typeof window !== 'undefined') {
        window.localStorage.setItem(DENSITY_STORAGE_KEY, value);
      }
    } catch {
      // ignore
    }
  }, []);

  const setActiveCollectionId = useCallback((id: string | null) => {
    setActiveCollectionIdState(id);
    try {
      if (typeof window !== 'undefined') {
        if (id) {
          window.localStorage.setItem(COLLECTION_STORAGE_KEY, id);
        } else {
          window.localStorage.removeItem(COLLECTION_STORAGE_KEY);
        }
      }
    } catch {
      // ignore
    }
  }, []);

  const normalizedSearchQuery = useMemo(() => normalizeToken(searchQuery), [searchQuery]);
  const activeFilter = useMemo(
    () => (filterByType ? normalizeToken(filterByType) : null),
    [filterByType],
  );

  const contentSearchScope = useMemo(
    () => documents.filter((doc) => matchesTypeFilter(doc, activeFilter)),
    [activeFilter, documents],
  );

  // Cache housekeeping.
  useEffect(() => {
    const currentIds = new Set(documents.map((doc) => doc.id));
    for (const id of contentCacheRef.current.keys()) {
      if (!currentIds.has(id)) contentCacheRef.current.delete(id);
    }
    for (const id of contentReadFailuresRef.current) {
      if (!currentIds.has(id)) contentReadFailuresRef.current.delete(id);
    }
  }, [documents]);

  // CONTENT SEARCH SCHEDULING — lifted verbatim from FileBrowser.tsx:320-406. Do not rewrite.
  useEffect(() => {
    const requestId = contentSearchSequenceRef.current + 1;
    contentSearchSequenceRef.current = requestId;

    if (!normalizedSearchQuery) {
      clearContentSearch();
      return;
    }

    const documentsToCheck = contentSearchScope.filter((doc) => {
      const fileNameMatches = normalizeToken(doc.fileName).includes(normalizedSearchQuery);
      return !fileNameMatches && canSearchByContent(doc);
    });

    if (documentsToCheck.length === 0) {
      setContentSearchMatches([]);
      setContentSearchLoading(false);
      return;
    }

    let cancelled = false;

    const timeoutId = window.setTimeout(() => {
      void (async () => {
        setContentSearchLoading(true);

        const contentMatches = new Set<string>();
        const batchSize = 6;

        for (let start = 0; start < documentsToCheck.length; start += batchSize) {
          if (cancelled || requestId !== contentSearchSequenceRef.current) {
            return;
          }

          const batch = documentsToCheck.slice(start, start + batchSize);
          const batchResults = await Promise.all(batch.map(async (doc) => {
            const cachedContent = contentCacheRef.current.get(doc.id);
            if (typeof cachedContent === 'string') {
              return cachedContent.includes(normalizedSearchQuery) ? doc.id : null;
            }

            if (contentReadFailuresRef.current.has(doc.id)) {
              return null;
            }

            const result = await VaultAPI.readFileContent(doc.filePath);
            if (!result.ok) {
              contentReadFailuresRef.current.add(doc.id);
              return null;
            }

            const normalizedContent = result.data.toLowerCase();
            contentCacheRef.current.set(doc.id, normalizedContent);

            return normalizedContent.includes(normalizedSearchQuery) ? doc.id : null;
          }));

          for (const docId of batchResults) {
            if (docId) {
              contentMatches.add(docId);
            }
          }
        }

        if (!cancelled && requestId === contentSearchSequenceRef.current) {
          setContentSearchMatches(contentMatches);
          setContentSearchLoading(false);
        }
      })().catch(() => {
        if (!cancelled && requestId === contentSearchSequenceRef.current) {
          setContentSearchMatches([]);
          setContentSearchLoading(false);
        }
      });
    }, 220);

    return () => {
      cancelled = true;
      window.clearTimeout(timeoutId);
    };
  }, [
    clearContentSearch,
    contentSearchScope,
    normalizedSearchQuery,
    setContentSearchLoading,
    setContentSearchMatches,
  ]);

  // Guard against referring to a deleted collection.
  useEffect(() => {
    if (activeCollectionId && !customCollections.some((c) => c.id === activeCollectionId)) {
      setActiveCollectionId(null);
    }
  }, [activeCollectionId, customCollections, setActiveCollectionId]);

  // Apply collection filter on top of base filtered documents.
  const collectionFilteredDocuments = useMemo(() => {
    if (!activeCollectionId) return filteredDocuments;
    const collection = customCollections.find((c) => c.id === activeCollectionId);
    if (!collection) return filteredDocuments;
    const idSet = new Set(collection.documentIds);
    return filteredDocuments.filter((doc) => idSet.has(doc.id));
  }, [filteredDocuments, customCollections, activeCollectionId]);

  const sortedDocuments = useMemo(
    () => sortDocuments(collectionFilteredDocuments, sort.field, sort.order),
    [collectionFilteredDocuments, sort.field, sort.order],
  );

  const groups = useMemo(
    () => groupDocuments(sortedDocuments, groupBy),
    [sortedDocuments, groupBy],
  );

  const listItems = useMemo(
    () => flattenToListItems(groups, groupBy !== 'none'),
    [groups, groupBy],
  );

  const hasActiveFilters =
    Boolean(normalizedSearchQuery) ||
    activeFilter !== null ||
    filterBySource !== 'all' ||
    activeCollectionId !== null;

  const clearAllFilters = useCallback(() => {
    setSearchQuery('');
    setFilterByType(null);
    setFilterBySource('all');
    setActiveCollectionId(null);
  }, [setSearchQuery, setFilterByType, setFilterBySource, setActiveCollectionId]);

  return {
    groupBy,
    setGroupBy,
    sortField: sort.field,
    sortOrder: sort.order,
    setSort,
    density,
    setDensity,
    filteredDocuments: collectionFilteredDocuments,
    sortedDocuments,
    groups,
    listItems,
    normalizedSearchQuery,
    activeFilter,
    isContentSearchLoading,
    clearAllFilters,
    hasActiveFilters,
    activeCollectionId,
    setActiveCollectionId,
    customCollections,
  };
}
