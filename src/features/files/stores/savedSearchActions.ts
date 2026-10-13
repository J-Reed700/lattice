import { type StoreApi } from 'zustand';

import { type FileBrowserActions, type SavedSearchPreset } from '@/types/fileBrowser';

import { buildUniqueName, persistSavedSearches, sanitizeName } from './fileBrowserPersistence';
import { type FileBrowserStore } from './fileBrowserStore.types';
import { normalizeToken } from './libraryDocumentFilters';

type SavedSearchActions = Pick<
  FileBrowserActions,
  | 'createSavedSearch'
  | 'applySavedSearch'
  | 'updateSavedSearch'
  | 'reorderSavedSearches'
  | 'duplicateSavedSearch'
  | 'deleteSavedSearch'
  | 'clearActiveSavedSearch'
>;

/** The saved-search actions of the file browser store; each change is persisted. */
export const createSavedSearchActions = (
  set: StoreApi<FileBrowserStore>['setState'],
  get: StoreApi<FileBrowserStore>['getState']
): SavedSearchActions => ({
  createSavedSearch: (name: string, options) => {
    const normalizedName = sanitizeName(name);
    if (!normalizedName) {
      return null;
    }

    const state = get();
    const hasCriteria =
      Boolean(normalizeToken(state.searchQuery)) ||
      state.filterByType !== null ||
      state.filterBySource !== 'all';
    if (!hasCriteria) {
      return null;
    }

    const hasDuplicate = state.savedSearches.some(
      (search) => normalizeToken(search.name) === normalizeToken(normalizedName)
    );
    if (hasDuplicate) {
      return null;
    }

    const now = new Date().toISOString();
    const searchId = `search:${Date.now().toString(36)}:${Math.random().toString(36).slice(2, 9)}`;
    const nextSearch: SavedSearchPreset = {
      id: searchId,
      name: normalizedName,
      query: state.searchQuery,
      filterByType: state.filterByType,
      filterBySource: state.filterBySource,
      pinned: options?.pinned ?? true,
      createdAt: now,
      updatedAt: now,
    };

    set((currentState) => {
      const savedSearches = [nextSearch, ...currentState.savedSearches];
      persistSavedSearches(savedSearches);
      return {
        savedSearches,
        activeSavedSearchId: searchId,
      };
    });

    return searchId;
  },

  applySavedSearch: (searchId: string) => {
    set((state) => {
      const target = state.savedSearches.find((search) => search.id === searchId);
      if (!target) {
        return state;
      }

      return {
        activeSavedSearchId: target.id,
        searchQuery: target.query,
        filterByType: target.filterByType,
        filterBySource: target.filterBySource,
        contentSearchMatches: new Set(),
        isContentSearchLoading: false,
      };
    });
  },

  updateSavedSearch: (searchId, updates) => {
    set((state) => {
      const target = state.savedSearches.find((search) => search.id === searchId);
      if (!target) {
        return state;
      }

      const normalizedName =
        typeof updates.name === 'string' ? sanitizeName(updates.name) : target.name;
      if (!normalizedName) {
        return state;
      }

      const hasDuplicateName = state.savedSearches.some(
        (search) => search.id !== searchId && normalizeToken(search.name) === normalizeToken(normalizedName)
      );
      if (hasDuplicateName) {
        return state;
      }

      const nextSearch: SavedSearchPreset = {
        ...target,
        ...updates,
        name: normalizedName,
        updatedAt: new Date().toISOString(),
      };

      const savedSearches = state.savedSearches.map((search) => (search.id === searchId ? nextSearch : search));
      persistSavedSearches(savedSearches);
      return { savedSearches };
    });
  },

  reorderSavedSearches: (orderedIds: string[]) => {
    set((state) => {
      if (orderedIds.length === 0 || state.savedSearches.length < 2) {
        return state;
      }

      const desiredOrder = new Map<string, number>(
        orderedIds.map((id, index) => [id, index])
      );

      const ordered = [...state.savedSearches].sort((a, b) => {
        const left = desiredOrder.get(a.id);
        const right = desiredOrder.get(b.id);
        if (left === undefined && right === undefined) {
          return 0;
        }
        if (left === undefined) {
          return 1;
        }
        if (right === undefined) {
          return -1;
        }
        return left - right;
      });

      persistSavedSearches(ordered);
      return { savedSearches: ordered };
    });
  },

  duplicateSavedSearch: (searchId: string) => {
    const state = get();
    const source = state.savedSearches.find((search) => search.id === searchId);
    if (!source) {
      return null;
    }

    const now = new Date().toISOString();
    const duplicateId = `search:${Date.now().toString(36)}:${Math.random().toString(36).slice(2, 9)}`;
    const duplicateName = buildUniqueName(
      `${source.name} Copy`,
      state.savedSearches.map((search) => search.name)
    );

    const duplicate: SavedSearchPreset = {
      ...source,
      id: duplicateId,
      name: duplicateName,
      createdAt: now,
      updatedAt: now,
    };

    set((currentState) => {
      const sourceIndex = currentState.savedSearches.findIndex((search) => search.id === searchId);
      const insertIndex = sourceIndex >= 0 ? sourceIndex + 1 : currentState.savedSearches.length;
      const savedSearches = [...currentState.savedSearches];
      savedSearches.splice(insertIndex, 0, duplicate);
      persistSavedSearches(savedSearches);
      return { savedSearches };
    });

    return duplicateId;
  },

  deleteSavedSearch: (searchId: string) => {
    set((state) => {
      const savedSearches = state.savedSearches.filter((search) => search.id !== searchId);
      persistSavedSearches(savedSearches);
      return {
        savedSearches,
        activeSavedSearchId: state.activeSavedSearchId === searchId ? null : state.activeSavedSearchId,
      };
    });
  },

  clearActiveSavedSearch: () => {
    set({ activeSavedSearchId: null });
  },
});
