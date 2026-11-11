import {
  type SavedSearchPreset,
  type SourceFilter,
} from '@/types/fileBrowser';

import { normalizeToken } from './libraryDocumentFilters';

const SAVED_SEARCHES_STORAGE_KEY = 'lattice:file-browser-saved-searches:v1';

export const sanitizeName = (value: string): string => value.trim().replace(/\s+/g, ' ');
export const buildUniqueName = (candidate: string, existingNames: string[]): string => {
  const normalizedExisting = new Set(existingNames.map((name) => normalizeToken(name)));
  const name = sanitizeName(candidate) || 'Untitled';
  if (!normalizedExisting.has(normalizeToken(name))) return name;
  let index = 2;
  while (normalizedExisting.has(normalizeToken(`${name} (${index})`))) index += 1;
  return `${name} (${index})`;
};
const sanitizeSavedSearch = (value: unknown): SavedSearchPreset | null => {
  if (!value || typeof value !== 'object') {
    return null;
  }

  const candidate = value as Partial<SavedSearchPreset>;
  const sourceFilter = typeof candidate.filterBySource === 'string' ? candidate.filterBySource : 'all';
  if (
    typeof candidate.id !== 'string' ||
    typeof candidate.name !== 'string' ||
    typeof candidate.query !== 'string' ||
    typeof candidate.createdAt !== 'string' ||
    typeof candidate.updatedAt !== 'string' ||
    !['all', 'local', 'web'].includes(sourceFilter)
  ) {
    return null;
  }

  return {
    id: candidate.id,
    name: sanitizeName(candidate.name),
    query: candidate.query,
    filterByType:
      typeof candidate.filterByType === 'string' && normalizeToken(candidate.filterByType)
        ? normalizeToken(candidate.filterByType)
        : null,
    filterBySource: sourceFilter as SourceFilter,
    pinned: Boolean(candidate.pinned),
    createdAt: candidate.createdAt,
    updatedAt: candidate.updatedAt,
  };
};

export const loadSavedSearches = (): SavedSearchPreset[] => {
  if (typeof window === 'undefined') {
    return [];
  }

  try {
    const raw = window.localStorage.getItem(SAVED_SEARCHES_STORAGE_KEY);
    if (!raw) {
      return [];
    }

    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) {
      return [];
    }

    return parsed
      .map((item) => sanitizeSavedSearch(item))
      .filter((item): item is SavedSearchPreset => item !== null);
  } catch {
    return [];
  }
};

export const persistSavedSearches = (searches: SavedSearchPreset[]): void => {
  if (typeof window === 'undefined') {
    return;
  }

  try {
    window.localStorage.setItem(SAVED_SEARCHES_STORAGE_KEY, JSON.stringify(searches));
  } catch {
    // Ignore persistence failures; in-memory state remains valid
  }
};
