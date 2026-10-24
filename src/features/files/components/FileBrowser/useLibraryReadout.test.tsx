import { renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { CustomCollection, DocumentMetadata, LibraryScope, SourceFilter } from '@/types/fileBrowser';

import { useLibraryReadout } from './useLibraryReadout';

const h = vi.hoisted(() => ({ folders: undefined as unknown[] | undefined, identity: { name: 'identity' } }));
vi.mock('@/features/files/hooks/useIndexedFoldersQuery', () => ({ useIndexedFoldersQuery: () => ({ data: h.folders }) }));
vi.mock('@/features/files/hooks/useCorpusIdentity', () => ({ useCorpusIdentity: () => h.identity }));

const docs = [
  { id: '1', fileName: 'a.md', filePath: '/notes/a.md', category: 'document' },
  { id: '2', fileName: 'b.pdf', filePath: '/notes/b.pdf', category: 'document' },
  { id: '3', fileName: 'Bread', filePath: 'https://example.org/bread', category: 'web article' },
] as DocumentMetadata[];

const collections = [{ id: 'k1', name: 'Fermentation' }] as CustomCollection[];
const themes = [{ id: 't1', label: 'Grains' }] as never;

const readout = (scope: LibraryScope, filterBySource: SourceFilter = 'all', documents = docs) =>
  renderHook(() => useLibraryReadout({ documents, customCollections: collections, scope, filterBySource, themes })).result.current;

describe('useLibraryReadout', () => {
  beforeEach(() => {
    h.folders = [{ path: '/notes' }];
  });

  it('counts local and web documents and describes the library', () => {
    const result = readout({ kind: 'all' } as LibraryScope);
    expect(result.sourceCounts).toEqual({ all: 3, web: 1, local: 2 });
    expect(result.headerMeta).toBe('3 documents · 2 local · 1 web · 1 folder');
    expect(result.identity).toBe(h.identity);
  });

  it('counts no folders before they load', () => {
    h.folders = undefined;
    expect(readout({ kind: 'all' } as LibraryScope, 'all', []).headerMeta).toBe('0 documents · 0 local · 0 web · 0 folders');
  });

  it.each([
    [{ kind: 'folder', path: '/notes/kitchen' }, 'all', 'kitchen'],
    [{ kind: 'collection', id: 'k1' }, 'all', 'Fermentation'],
    [{ kind: 'collection', id: 'gone' }, 'all', 'Collection'],
    [{ kind: 'theme', id: 't1' }, 'all', 'Grains'],
    [{ kind: 'theme', id: 'gone' }, 'all', 'Theme'],
    [{ kind: 'all' }, 'web', 'Web'],
    [{ kind: 'all' }, 'local', 'Local'],
    [{ kind: 'all' }, 'all', 'All documents'],
  ])('names scope %j with source %s as %s', (scope, source, name) => {
    expect(readout(scope as LibraryScope, source as SourceFilter).scopeName).toBe(name);
  });
});
