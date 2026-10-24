import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import type { SavedSearchPreset } from '@/types/fileBrowser';

import { loadSavedSearches } from './fileBrowserPersistence';
import { createSavedSearchActions } from './savedSearchActions';

import type { FileBrowserStore } from './fileBrowserStore.types';

type State = Pick<FileBrowserStore, 'savedSearches' | 'activeSavedSearchId' | 'searchQuery' | 'filterByType' | 'filterBySource' | 'contentSearchMatches' | 'isContentSearchLoading'>;

const preset = (id: string, name: string, overrides: Partial<SavedSearchPreset> = {}): SavedSearchPreset => ({
  id, name, query: name.toLowerCase(), filterByType: null, filterBySource: 'all', pinned: false,
  createdAt: '2026-01-01T00:00:00Z', updatedAt: '2026-01-01T00:00:00Z', ...overrides,
});

let state: State;
const makeActions = () => createSavedSearchActions(
  ((partial: Partial<State> | ((s: State) => Partial<State>)) => {
    state = { ...state, ...(typeof partial === 'function' ? partial(state) : partial) };
  }) as never,
  (() => state) as never,
);

describe('createSavedSearchActions', () => {
  beforeEach(() => {
    state = {
      savedSearches: [preset('a', 'Glaciers'), preset('b', 'Volcanoes')],
      activeSavedSearchId: null,
      searchQuery: '',
      filterByType: null,
      filterBySource: 'all',
      contentSearchMatches: new Set(['x']),
      isContentSearchLoading: true,
    };
  });

  afterEach(() => {
    localStorage.removeItem('lattice:file-browser-saved-searches:v1');
  });

  describe('createSavedSearch', () => {
    it('saves the current search pinned, first, and active', () => {
      state.searchQuery = 'moraine';
      state.filterByType = 'pdf';
      const id = makeActions().createSavedSearch('  Moraines  ');
      expect(id).toMatch(/^search:/);
      expect(state.savedSearches[0]).toMatchObject({ id, name: 'Moraines', query: 'moraine', filterByType: 'pdf', pinned: true });
      expect(state.activeSavedSearchId).toBe(id);
      expect(loadSavedSearches()[0].name).toBe('Moraines');
    });

    it('can save unpinned, and counts a source filter as a search', () => {
      state.filterBySource = 'web';
      const id = makeActions().createSavedSearch('Web only', { pinned: false });
      expect(state.savedSearches.find((entry) => entry.id === id)?.pinned).toBe(false);
    });

    it('refuses a blank name, an empty search, or a taken name', () => {
      const actions = makeActions();
      state.searchQuery = 'lava';
      expect(actions.createSavedSearch('   ')).toBeNull();
      expect(actions.createSavedSearch('volcanoes')).toBeNull();
      state.searchQuery = '  ';
      expect(actions.createSavedSearch('Lava')).toBeNull();
      expect(state.savedSearches).toHaveLength(2);
    });
  });

  describe('applySavedSearch', () => {
    it('restores the saved criteria and resets content matches', () => {
      state.savedSearches[1] = preset('b', 'Volcanoes', { filterByType: 'md', filterBySource: 'local' });
      makeActions().applySavedSearch('b');
      expect(state).toMatchObject({ activeSavedSearchId: 'b', searchQuery: 'volcanoes', filterByType: 'md', filterBySource: 'local', isContentSearchLoading: false });
      expect(state.contentSearchMatches.size).toBe(0);
    });

    it('ignores an unknown search', () => {
      const before = state;
      makeActions().applySavedSearch('missing');
      expect(state).toEqual(before);
    });
  });

  describe('updateSavedSearch', () => {
    it('renames and updates a search', () => {
      makeActions().updateSavedSearch('a', { name: '  Ice  sheets ', pinned: true });
      expect(state.savedSearches[0]).toMatchObject({ name: 'Ice sheets', pinned: true });
      expect(state.savedSearches[0].updatedAt).not.toBe('2026-01-01T00:00:00Z');
    });

    it('keeps the name when only other fields change', () => {
      makeActions().updateSavedSearch('a', { pinned: true });
      expect(state.savedSearches[0]).toMatchObject({ name: 'Glaciers', pinned: true });
    });

    it('refuses a blank or duplicate name, or an unknown search', () => {
      const actions = makeActions();
      actions.updateSavedSearch('a', { name: ' ' });
      actions.updateSavedSearch('a', { name: 'VOLCANOES' });
      actions.updateSavedSearch('missing', { name: 'x' });
      expect(state.savedSearches.map((entry) => entry.name)).toEqual(['Glaciers', 'Volcanoes']);
    });
  });

  describe('reorderSavedSearches', () => {
    it('puts listed searches in order and unlisted ones after', () => {
      state.savedSearches = [preset('a', 'A'), preset('b', 'B'), preset('c', 'C'), preset('d', 'D')];
      makeActions().reorderSavedSearches(['c', 'a']);
      expect(state.savedSearches.map((entry) => entry.id)).toEqual(['c', 'a', 'b', 'd']);
    });

    it('does nothing for an empty order or a single search', () => {
      const actions = makeActions();
      actions.reorderSavedSearches([]);
      expect(state.savedSearches.map((entry) => entry.id)).toEqual(['a', 'b']);
      state.savedSearches = [preset('a', 'A')];
      actions.reorderSavedSearches(['a']);
      expect(state.savedSearches.map((entry) => entry.id)).toEqual(['a']);
    });
  });

  describe('duplicateSavedSearch', () => {
    it('copies a search right after it with a unique name', () => {
      state.savedSearches = [preset('a', 'Glaciers'), preset('x', 'Glaciers Copy'), preset('b', 'Volcanoes')];
      const id = makeActions().duplicateSavedSearch('a');
      expect(state.savedSearches.map((entry) => entry.name)).toEqual(['Glaciers', 'Glaciers Copy (2)', 'Glaciers Copy', 'Volcanoes']);
      expect(state.savedSearches[1]).toMatchObject({ id, query: 'glaciers' });
    });

    it('returns null for an unknown search', () => {
      expect(makeActions().duplicateSavedSearch('missing')).toBeNull();
    });
  });

  describe('deleting and clearing', () => {
    it('deletes a search and clears it if it was active', () => {
      state.activeSavedSearchId = 'a';
      const actions = makeActions();
      actions.deleteSavedSearch('b');
      expect(state.activeSavedSearchId).toBe('a');
      actions.deleteSavedSearch('a');
      expect(state.savedSearches).toEqual([]);
      expect(state.activeSavedSearchId).toBeNull();
    });

    it('clears the active search', () => {
      state.activeSavedSearchId = 'b';
      makeActions().clearActiveSavedSearch();
      expect(state.activeSavedSearchId).toBeNull();
    });
  });
});
