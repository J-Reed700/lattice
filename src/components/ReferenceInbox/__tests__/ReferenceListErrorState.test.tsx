import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '../../ui/tooltip';
import { ReferenceList } from '../ReferenceList';

import type { UseReferenceInboxResult } from '../useReferenceInbox';

const refetch = vi.fn();

const passagesQuery = vi.hoisted(() => ({
  current: { isError: false, isFetching: false, refetch: () => {} },
}));

vi.mock('@/hooks/queries/usePassageReferencesQuery', () => ({
  usePassageReferencesQuery: () => passagesQuery.current,
}));

const state = (): UseReferenceInboxResult =>
  ({
    bookmarks: [],
    filteredItems: [],
    selectedItem: null,
    spacesById: new Map(),
    journalsById: new Map(),
    capturedIndex: new Map(),
    isLoading: false,
    query: '',
    setQuery: vi.fn(),
    originFilter: 'all',
    setOriginFilter: vi.fn(),
    statusChip: 'all',
    setStatusChip: vi.fn(),
    selectedId: null,
    setSelectedId: vi.fn(),
  }) as unknown as UseReferenceInboxResult;

const renderList = () =>
  render(
    <TooltipProvider>
      <ReferenceList
        state={state()}
        onToggleCollapse={vi.fn()}
        onOpenInOrigin={vi.fn()}
        onCopy={vi.fn()}
        onDelete={vi.fn()}
        onOpenPassageSource={vi.fn()}
        onCopyPassage={vi.fn()}
        onDeletePassage={vi.fn()}
      />
    </TooltipProvider>
  );

describe('ReferenceList — load failure', () => {
  it('admits the failure instead of claiming there is nothing saved', async () => {
    passagesQuery.current = { isError: true, isFetching: false, refetch };
    renderList();

    expect(screen.getByText(/Couldn't load references\./)).toBeInTheDocument();
    expect(screen.queryByText('No references yet.')).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: 'Retry' }));
    expect(refetch).toHaveBeenCalledTimes(1);
  });

  it('still says the shelf is empty when the list really is empty', () => {
    passagesQuery.current = { isError: false, isFetching: false, refetch: () => {} };
    renderList();

    expect(screen.getByText('No references yet.')).toBeInTheDocument();
    expect(screen.queryByText(/Couldn't load references/)).not.toBeInTheDocument();
  });
});
