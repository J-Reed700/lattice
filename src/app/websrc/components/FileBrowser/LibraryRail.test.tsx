import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { LibraryRail, isFallbackLabel, rebuildLabel } from './LibraryRail';
import { TooltipProvider } from '../ui/tooltip';

import type { ClusterDto } from '../../types';

vi.mock('../../hooks/queries/useIndexedFoldersQuery', () => ({
  useIndexedFoldersQuery: () => ({ data: [] }),
}));

const theme = (overrides: Partial<ClusterDto>): ClusterDto => ({
  id: 'theme-1',
  label: 'Braising and slow cooking',
  description: null,
  memberCount: 47,
  sampleTitles: ['Braised short ribs', 'Weeknight dal', 'Harissa chicken'],
  labelSource: 'llm',
  inheritedFromClusterId: null,
  memberDocumentIds: [],
  ...overrides,
});

const rail = (props: Partial<React.ComponentProps<typeof LibraryRail>> = {}) => (
  <TooltipProvider>
    <LibraryRail
      scope={{ kind: 'all' }}
      onScopeChange={vi.fn()}
      collections={[]}
      onCreateCollection={vi.fn()}
      savedSearches={[]}
      activeSavedSearchId={null}
      onApplySavedSearch={vi.fn()}
      onRenameSavedSearch={vi.fn()}
      onCreateSavedSearch={vi.fn()}
      sources={[]}
      themes={[]}
      themesEnabled
      hasRunThemes={false}
      isFindingThemes={false}
      themeProgress={null}
      onFindThemes={vi.fn()}
      {...props}
    />
  </TooltipProvider>
);

const renderRail = (props: Partial<React.ComponentProps<typeof LibraryRail>> = {}) =>
  render(rail(props));

describe('LibraryRail themes section', () => {
  it('is absent on a vault too small for themes', () => {
    renderRail({ themesEnabled: false });
    expect(screen.queryByRole('heading', { name: 'Themes' })).not.toBeInTheDocument();
  });

  it('does not rename the existing Collections section', () => {
    renderRail();
    expect(screen.getByRole('heading', { name: 'Collections' })).toBeInTheDocument();
  });

  it('offers to find themes on a vault that has none yet', () => {
    renderRail();
    expect(screen.getByRole('heading', { name: 'Themes' })).toBeInTheDocument();
    expect(
      screen.getByText('No themes yet. Find themes to group your vault by topic.')
    ).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Find themes' })).toBeInTheDocument();
  });

  it('says Refresh themes once a run has produced some', () => {
    renderRail({ themes: [theme({})], hasRunThemes: true });
    expect(screen.getByRole('button', { name: 'Refresh themes' })).toBeInTheDocument();
  });

  it('shows preview titles only for a theme the model could not name', () => {
    const { rerender } = renderRail({ themes: [theme({})], hasRunThemes: true });
    expect(screen.queryByText('Braised short ribs · Weeknight dal · Harissa chicken')).toBeNull();

    rerender(
      rail({ themes: [theme({ label: 'Theme of 47', labelSource: 'fallback' })], hasRunThemes: true })
    );
    expect(
      screen.getByText('Braised short ribs · Weeknight dal · Harissa chicken')
    ).toBeInTheDocument();
  });

  it('replaces the list with one progress line while a run is in flight', () => {
    renderRail({
      themes: [theme({})],
      isFindingThemes: true,
      themeProgress: { phase: 'labeling', current: 7, total: 18 },
    });
    expect(screen.getByText('Naming 7 of 18…')).toBeInTheDocument();
    expect(screen.queryByText('Braising and slow cooking')).toBeNull();
  });
});

describe('rebuildLabel', () => {
  it('names each phase', () => {
    expect(rebuildLabel(null)).toBe('Finding themes…');
    expect(rebuildLabel({ phase: 'loading', current: 0, total: 100 })).toBe('Reading embeddings…');
    expect(rebuildLabel({ phase: 'clustering', current: 0, total: 100 })).toBe('Grouping documents…');
    expect(rebuildLabel({ phase: 'labeling', current: 7, total: 18 })).toBe('Naming 7 of 18…');
    expect(rebuildLabel({ phase: 'saving', current: 0, total: 3 })).toBe('Saving…');
  });
});

describe('isFallbackLabel', () => {
  it('detects both the flag and the shape of the generated name', () => {
    expect(isFallbackLabel(theme({ labelSource: 'fallback' }))).toBe(true);
    expect(isFallbackLabel(theme({ label: 'Theme of 47' }))).toBe(true);
    expect(isFallbackLabel(theme({}))).toBe(false);
  });
});
