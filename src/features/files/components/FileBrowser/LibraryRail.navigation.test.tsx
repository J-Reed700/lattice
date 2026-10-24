import { fireEvent, render, screen, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import type { CustomCollection } from '@/types/fileBrowser';

import { LibraryRail } from './LibraryRail';

const h = vi.hoisted(() => ({ folders: undefined as unknown }));
vi.mock('@/features/files/hooks/useIndexedFoldersQuery', () => ({ useIndexedFoldersQuery: () => ({ data: h.folders }) }));

const collection = (id: string, name: string, kind: 'manual' | 'snapshot' = 'manual', documentIds: string[] = []) =>
  ({ id, name, kind, documentIds, parentId: null, createdAt: '', updatedAt: '' }) as CustomCollection;

type Props = Partial<React.ComponentProps<typeof LibraryRail>>;
const renderRail = (props: Props = {}) => {
  const all = {
    scope: { kind: 'all' } as React.ComponentProps<typeof LibraryRail>['scope'],
    onScopeChange: vi.fn(),
    collections: [] as CustomCollection[],
    onCreateCollection: vi.fn(),
    savedSearches: [],
    activeSavedSearchId: null,
    onApplySavedSearch: vi.fn(),
    onRenameSavedSearch: vi.fn(),
    onCreateSavedSearch: vi.fn(),
    sources: [],
    themes: [],
    themesEnabled: true,
    hasRunThemes: false,
    isFindingThemes: false,
    themeProgress: null,
    onFindThemes: vi.fn(),
    ...props,
  };
  render(<TooltipProvider><LibraryRail {...all} /></TooltipProvider>);
  return all;
};

describe('LibraryRail navigation', () => {
  beforeEach(() => {
    h.folders = [{ path: '/Users/ada/Recipes', documentCount: 1200 }, { path: '/Users/ada/Herbarium', documentCount: 3 }];
  });

  it('scopes to all documents or a folder', () => {
    const props = renderRail({ scope: { kind: 'folder', path: '/Users/ada/Herbarium' } });
    expect(screen.getByRole('button', { name: /^Recipes/ })).toHaveTextContent(`Recipes${(1200).toLocaleString()}`);
    fireEvent.click(screen.getByRole('button', { name: /^Recipes/ }));
    fireEvent.click(screen.getByRole('button', { name: 'All documents' }));
    expect(vi.mocked(props.onScopeChange).mock.calls).toEqual([[{ kind: 'folder', path: '/Users/ada/Recipes' }], [{ kind: 'all' }]]);
  });

  it('copes before folders load', () => {
    h.folders = undefined;
    renderRail();
    expect(screen.getByRole('button', { name: 'All documents' })).toBeInTheDocument();
  });

  describe('collections', () => {
    it('invites a first collection', () => {
      renderRail();
      expect(screen.getByText('Create a collection to organize your documents.')).toBeInTheDocument();
    });

    it('lists collections with counts, marking snapshots and the open one', () => {
      const props = renderRail({
        collections: [collection('k1', 'Breads', 'manual', ['a', 'b']), collection('k2', 'Snapshot: rye', 'snapshot', ['c'])],
        collectionCounts: new Map([['k1', 5]]),
        scope: { kind: 'collection', id: 'k2' },
      });
      expect(screen.getByRole('button', { name: /^Breads/ })).toHaveTextContent('Breads5');
      expect(screen.getByRole('button', { name: /^Breads/ })).not.toHaveAttribute('aria-current');
      expect(screen.getByRole('button', { name: /^Snapshot: rye/ })).toHaveTextContent('Snapshot: ryesnapshot1');
      expect(screen.getByRole('button', { name: /^Snapshot: rye/ })).toHaveAttribute('aria-current', 'page');
      fireEvent.click(screen.getByRole('button', { name: /^Breads/ }));
      expect(props.onScopeChange).toHaveBeenCalledWith({ kind: 'collection', id: 'k1' });
      expect(screen.queryByRole('button', { name: 'Actions for Breads' })).not.toBeInTheDocument();
    });

    it('renames and deletes from a collection’s menu', () => {
      const onRenameCollection = vi.fn();
      const onDeleteCollection = vi.fn();
      const breads = collection('k1', 'Breads');
      renderRail({ collections: [breads], onRenameCollection, onDeleteCollection });
      fireEvent.click(screen.getByRole('button', { name: 'Actions for Breads' }));
      fireEvent.click(screen.getByRole('button', { name: 'Rename collection' }));
      expect(onRenameCollection).toHaveBeenCalledWith(breads);
      fireEvent.click(screen.getByRole('button', { name: 'Actions for Breads' }));
      fireEvent.click(screen.getByRole('button', { name: 'Delete collection' }));
      expect(onDeleteCollection).toHaveBeenCalledWith(breads);
    });

    it('offers only the actions the host supports', () => {
      renderRail({ collections: [collection('k1', 'Breads')], onDeleteCollection: vi.fn() });
      fireEvent.click(screen.getByRole('button', { name: 'Actions for Breads' }));
      expect(screen.queryByRole('button', { name: 'Rename collection' })).not.toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Delete collection' })).toBeInTheDocument();
    });

    it('creates a collection by name on Enter or blur, and not when blank or cancelled', () => {
      const props = renderRail();
      fireEvent.click(screen.getByRole('button', { name: 'New collection' }));
      const input = screen.getByRole('textbox', { name: 'New collection' });
      fireEvent.change(input, { target: { value: '  Soups  ' } });
      fireEvent.keyDown(input, { key: 'Enter' });
      expect(props.onCreateCollection).toHaveBeenCalledWith('Soups');
      expect(screen.queryByRole('textbox', { name: 'New collection' })).not.toBeInTheDocument();

      fireEvent.click(screen.getByRole('button', { name: 'New collection' }));
      fireEvent.change(screen.getByRole('textbox', { name: 'New collection' }), { target: { value: 'Stews' } });
      fireEvent.keyDown(screen.getByRole('textbox', { name: 'New collection' }), { key: 'Escape' });
      fireEvent.click(screen.getByRole('button', { name: 'New collection' }));
      fireEvent.keyDown(screen.getByRole('textbox', { name: 'New collection' }), { key: 'a' });
      fireEvent.blur(screen.getByRole('textbox', { name: 'New collection' }));
      expect(props.onCreateCollection).toHaveBeenCalledTimes(1);

      fireEvent.click(screen.getByRole('button', { name: 'New collection' }));
      fireEvent.click(screen.getByRole('button', { name: 'New collection' }));
      expect(screen.queryByRole('textbox', { name: 'New collection' })).not.toBeInTheDocument();
    });
  });

  describe('themes', () => {
    const theme = (id: string, label: string, overrides: Record<string, unknown> = {}) => ({
      id, label, description: null, memberCount: 12, sampleTitles: [], labelSource: 'llm', inheritedFromClusterId: null, memberDocumentIds: [], ...overrides,
    });

    it('scopes to a theme and describes it', () => {
      const props = renderRail({
        hasRunThemes: true,
        scope: { kind: 'theme', id: 't2' },
        themes: [theme('t1', 'Fermentation', { description: 'Sourdough, kimchi and kefir' }), theme('t2', 'Knife skills')] as never,
      });
      expect(screen.getByText('Sourdough, kimchi and kefir')).toBeInTheDocument();
      fireEvent.click(screen.getByRole('button', { name: /Fermentation/ }));
      expect(props.onScopeChange).toHaveBeenCalledWith({ kind: 'theme', id: 't1' });
    });
  });

  describe('saved searches and sources', () => {
    it('applies a saved search on click and renames it on double-click', () => {
      const props = renderRail({
        savedSearches: [{ id: 's1', name: 'Saffron' }, { id: 's2', name: 'Cumin' }] as never,
        activeSavedSearchId: 's2',
        sources: [{ id: 'src1', name: 'Dropbox recipes' }] as never,
      });
      fireEvent.click(screen.getByRole('button', { name: 'Saffron' }));
      fireEvent.doubleClick(screen.getByRole('button', { name: 'Cumin' }));
      expect(props.onApplySavedSearch).toHaveBeenCalledWith('s1');
      expect(props.onRenameSavedSearch).toHaveBeenCalledWith('s2', 'Cumin');
      const sources = screen.getByRole('heading', { name: 'Sources' }).closest('section')!;
      expect(within(sources).getByText('Dropbox recipes')).toBeInTheDocument();
      expect(within(sources).queryByRole('button')).not.toBeInTheDocument();
    });

    it('saves the current search by name', () => {
      const props = renderRail();
      expect(screen.getByText('No saved searches.')).toBeInTheDocument();
      expect(screen.getByText('No sources.')).toBeInTheDocument();
      fireEvent.click(screen.getByRole('button', { name: 'Save current search' }));
      fireEvent.change(screen.getByRole('textbox', { name: 'Save current search' }), { target: { value: 'Paprika' } });
      fireEvent.blur(screen.getByRole('textbox', { name: 'Save current search' }));
      expect(props.onCreateSavedSearch).toHaveBeenCalledWith('Paprika');
    });
  });
});
