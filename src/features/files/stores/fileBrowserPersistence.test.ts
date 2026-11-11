import { afterEach, describe, expect, it, vi } from 'vitest';

import type { SavedSearchPreset } from '@/types/fileBrowser';

import {
  buildUniqueName,
  loadSavedSearches,
  persistSavedSearches,
  sanitizeName,
} from './fileBrowserPersistence';

const SEARCHES = 'lattice:file-browser-saved-searches:v1';

const search = (overrides: Partial<SavedSearchPreset> = {}): SavedSearchPreset => ({
  id: 's1',
  name: 'Roman roads',
  query: 'via appia',
  filterByType: null,
  filterBySource: 'all',
  pinned: false,
  createdAt: '2026-01-01T00:00:00Z',
  updatedAt: '2026-01-02T00:00:00Z',
  ...overrides,
});

describe('fileBrowserPersistence', () => {
  afterEach(() => {
    localStorage.removeItem(SEARCHES);
    localStorage.removeItem(SOURCES);
    vi.restoreAllMocks();
  });

  describe('names', () => {
    it('collapses whitespace', () => {
      expect(sanitizeName('  Bird   calls \n of spring ')).toBe('Bird calls of spring');
    });

    it('numbers a name already taken, ignoring case', () => {
      expect(buildUniqueName('Herbs', ['Spices'])).toBe('Herbs');
      expect(buildUniqueName('herbs', ['Herbs', 'Herbs (2)'])).toBe('herbs (3)');
      expect(buildUniqueName('   ', [])).toBe('Untitled');
    });
  });

  describe('saved searches', () => {
    it('round-trips through storage, cleaning what it reads', () => {
      persistSavedSearches([search({ name: '  Roman   roads ', filterByType: ' PDF ', pinned: true })]);
      expect(loadSavedSearches()).toEqual([search({ name: 'Roman roads', filterByType: 'pdf', pinned: true })]);
    });

    it('defaults a missing source filter to all and a blank type to none', () => {
      localStorage.setItem(SEARCHES, JSON.stringify([{ ...search(), filterBySource: undefined, filterByType: '  ' }]));
      expect(loadSavedSearches()).toEqual([search()]);
    });

    it('drops entries that are not saved searches', () => {
      localStorage.setItem(SEARCHES, JSON.stringify([
        null,
        'text',
        { ...search(), id: 7 },
        { ...search(), filterBySource: 'cloud' },
        search({ id: 'ok' }),
      ]));
      expect(loadSavedSearches().map((entry) => entry.id)).toEqual(['ok']);
    });

    it('reads nothing from empty, non-list or broken storage', () => {
      expect(loadSavedSearches()).toEqual([]);
      localStorage.setItem(SEARCHES, '{"a":1}');
      expect(loadSavedSearches()).toEqual([]);
      localStorage.setItem(SEARCHES, '{broken');
      expect(loadSavedSearches()).toEqual([]);
    });

    it('survives storage that refuses writes', () => {
      vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('quota'); });
      expect(() => persistSavedSearches([search()])).not.toThrow();
    });
  });
});
