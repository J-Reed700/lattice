import {
  type SavedSearchPreset,
  type SourceConnection,
  type SourceFilter,
} from '@/types/fileBrowser';

import { normalizeToken } from './libraryDocumentFilters';

const SOURCE_CONNECTIONS_STORAGE_KEY = 'lattice:file-browser-source-connections:v1';
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

const sanitizeSourceConnection = (item: unknown): SourceConnection | null => {
  if (!item || typeof item !== 'object') {
    return null;
  }

  const candidate = item as Partial<SourceConnection>;
  if (
    typeof candidate.id !== 'string' ||
    typeof candidate.name !== 'string' ||
    typeof candidate.provider !== 'string' ||
    typeof candidate.mode !== 'string' ||
    typeof candidate.health !== 'string' ||
    typeof candidate.enabled !== 'boolean' ||
    typeof candidate.createdAt !== 'string' ||
    typeof candidate.updatedAt !== 'string'
  ) {
    return null;
  }

  if (
    !['web_import', 'dropbox', 'google_drive', 'onedrive', 'icloud'].includes(candidate.provider) ||
    !['referenced', 'managed'].includes(candidate.mode) ||
    !['healthy', 'attention', 'error', 'paused'].includes(candidate.health)
  ) {
    return null;
  }

  return {
    id: candidate.id,
    name: candidate.name,
    provider: candidate.provider,
    mode: candidate.mode,
    health: candidate.health,
    enabled: candidate.enabled,
    path: typeof candidate.path === 'string' ? candidate.path : undefined,
    lastSyncedAt: typeof candidate.lastSyncedAt === 'string' ? candidate.lastSyncedAt : null,
    createdAt: candidate.createdAt,
    updatedAt: candidate.updatedAt,
  };
};

export const loadSourceConnections = (): SourceConnection[] => {
  if (typeof window === 'undefined') {
    return [];
  }

  try {
    const raw = window.localStorage.getItem(SOURCE_CONNECTIONS_STORAGE_KEY);
    if (!raw) {
      return [];
    }

    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) {
      return [];
    }

    return parsed
      .map((item) => sanitizeSourceConnection(item))
      .filter((item): item is SourceConnection => item !== null);
  } catch {
    return [];
  }
};

export const persistSourceConnections = (connections: SourceConnection[]): void => {
  if (typeof window === 'undefined') {
    return;
  }

  try {
    const clientOnlyConnections = connections.filter(connection => connection.provider !== 'local_folder');
    window.localStorage.setItem(SOURCE_CONNECTIONS_STORAGE_KEY, JSON.stringify(clientOnlyConnections));
  } catch {
    // Ignore persistence failures; in-memory state remains valid
  }
};

export const sortSourceConnections = (connections: SourceConnection[]): SourceConnection[] =>
  [...connections].sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }));
