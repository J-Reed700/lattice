import { describe, expect, it } from 'vitest';

import type { DocumentMetadata, LibraryScope } from '@/types/fileBrowser';

import { filterLibraryDocuments, sortLibraryDocuments, type LibraryFilterState } from './libraryDocumentFilters';

const doc = (id: string, overrides: Partial<DocumentMetadata> = {}) => ({
  id, fileName: `${id}.md`, filePath: `/notes/${id}.md`, fileType: 'md', category: 'document',
  modifiedAt: '2026-01-01T00:00:00Z', indexedAt: '', wordCount: 100, ...overrides,
}) as DocumentMetadata;

const state = (overrides: Partial<LibraryFilterState> = {}): LibraryFilterState => ({
  searchQuery: '', filterByType: null, filterBySource: 'all', contentSearchMatches: new Set(), customCollections: [], scope: { kind: 'all' } as LibraryScope,
  ...overrides,
});

const ids = (list: DocumentMetadata[]) => list.map((item) => item.id);

describe('libraryDocumentFilters', () => {
  describe('sortLibraryDocuments', () => {
    const docs = [
      doc('b', { fileName: 'Barley.md', wordCount: 300, modifiedAt: '2026-03-01', fileType: 'pdf' }),
      doc('a', { fileName: 'apple.md', wordCount: 100, modifiedAt: '2026-01-01', fileType: 'MD' }),
      doc('c', { fileName: 'Cumin.md', wordCount: 100, modifiedAt: '2026-02-01', fileType: 'epub' }),
    ];

    it.each([
      ['name', 'asc', ['a', 'b', 'c']],
      ['size', 'desc', ['b', 'a', 'c']],
      ['modified', 'desc', ['b', 'c', 'a']],
      ['type', 'asc', ['c', 'a', 'b']],
    ] as const)('sorts by %s %s, keeping ties in order', (field, order, expected) => {
      expect(ids(sortLibraryDocuments(docs, field, order))).toEqual(expected);
    });
  });

  describe('filterLibraryDocuments', () => {
    const docs = [
      doc('local', { fileName: 'Bread notes.md' }),
      doc('web', { fileName: 'Rye', filePath: 'https://example.org/rye', category: 'web_article', fileType: 'html' }),
      doc('archived', { fileName: 'Spelt', filePath: '/data/.lattice/web-archive/spelt', category: 'web', fileType: 'html' }),
      doc('pdf', { fileName: 'Grain atlas.pdf', fileType: 'pdf', filePath: 'C:\\Notes\\Grain\\atlas.pdf' }),
    ];

    it('matches names and content hits', () => {
      expect(ids(filterLibraryDocuments(docs, state({ searchQuery: ' BREAD ' })))).toEqual(['local']);
      expect(ids(filterLibraryDocuments(docs, state({ searchQuery: 'kneading', contentSearchMatches: new Set(['pdf']) })))).toEqual(['pdf']);
    });

    it('filters by type, including web articles', () => {
      expect(ids(filterLibraryDocuments(docs, state({ filterByType: 'PDF' })))).toEqual(['pdf']);
      expect(ids(filterLibraryDocuments(docs, state({ filterByType: 'web' })))).toEqual(['web', 'archived']);
    });

    it('filters by source', () => {
      expect(ids(filterLibraryDocuments(docs, state({ filterBySource: 'web' })))).toEqual(['web', 'archived']);
      expect(ids(filterLibraryDocuments(docs, state({ filterBySource: 'local' })))).toEqual(['local', 'pdf']);
    });

    it('scopes to a folder, ignoring case and slashes', () => {
      expect(ids(filterLibraryDocuments(docs, state({ scope: { kind: 'folder', path: 'c:/notes/grain/' } as LibraryScope })))).toEqual(['pdf']);
      expect(ids(filterLibraryDocuments(docs, state({ scope: { kind: 'folder', path: '/' } as LibraryScope })))).toEqual([]);
    });

    it('scopes to a theme’s members, and to nothing for a theme it does not know', () => {
      const themes = [{ id: 't1', memberDocumentIds: ['web', 'pdf'] }];
      expect(ids(filterLibraryDocuments(docs, state({ themes, scope: { kind: 'theme', id: 't1' } as LibraryScope })))).toEqual(['web', 'pdf']);
      expect(ids(filterLibraryDocuments(docs, state({ scope: { kind: 'theme', id: 't9' } as LibraryScope })))).toEqual([]);
    });

    it('scopes to a collection and its nested collections without looping', () => {
      const customCollections = [
        { id: 'k1', parentId: null, documentIds: ['local'] },
        { id: 'k2', parentId: 'k1', documentIds: ['pdf'] },
        { id: 'k3', parentId: 'k2', documentIds: ['k1'] },
        { id: 'k4', parentId: null, documentIds: ['web'] },
      ] as never;
      expect(ids(filterLibraryDocuments(docs, state({ customCollections, scope: { kind: 'collection', id: 'k1' } as LibraryScope })))).toEqual(['local', 'pdf']);
    });
  });
});
