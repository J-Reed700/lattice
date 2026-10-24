import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import { ReferenceList } from '@/features/references/components/ReferenceList';
import type { UseReferenceInboxResult } from '@/features/references/hooks/useReferenceInbox';
import type { PassageReferenceDto } from '@/types/api/references';

const passage: PassageReferenceDto = {
  id: 'pref_1',
  documentId: 'doc_1',
  chunkId: 'chunk_1',
  filePath: '/vault/paper.pdf',
  fileName: 'paper.pdf',
  locator: 'p. 12',
  text: 'A saved passage.',
  title: null,
  note: null,
  createdAt: '2026-09-04T10:00:00.000Z',
};

const state = (overrides: Partial<UseReferenceInboxResult> = {}): UseReferenceInboxResult =>
  ({
    bookmarks: [],
    filteredItems: [],
    selectedItem: null,
    spacesById: new Map(),
    journalsById: new Map(),
    capturedIndex: new Map(),
    isLoading: false,
    listLoadFailed: false,
    isRetryingLoad: false,
    retryLoad: vi.fn(),
    query: '',
    setQuery: vi.fn(),
    originFilter: 'all',
    setOriginFilter: vi.fn(),
    statusChip: 'all',
    setStatusChip: vi.fn(),
    selectedId: null,
    setSelectedId: vi.fn(),
    ...overrides,
  }) as unknown as UseReferenceInboxResult;

const renderList = (overrides: Partial<UseReferenceInboxResult> = {}) =>
  render(
    <TooltipProvider>
      <ReferenceList
        state={state(overrides)}
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
    const retryLoad = vi.fn();
    renderList({ listLoadFailed: true, retryLoad });

    expect(screen.getByText(/Couldn't load references\./)).toBeInTheDocument();
    expect(screen.queryByText('No references yet.')).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: 'Retry' }));
    expect(retryLoad).toHaveBeenCalledTimes(1);
  });

  it('still says a list failed, with a retry, when the other list has items', async () => {
    const retryLoad = vi.fn();
    renderList({
      listLoadFailed: true,
      retryLoad,
      filteredItems: [{ kind: 'passage', id: passage.id, createdAt: passage.createdAt, passage }],
    });

    expect(screen.getByText('paper.pdf')).toBeInTheDocument();
    expect(screen.getByText(/Couldn't load all references\./)).toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: 'Retry' }));
    expect(retryLoad).toHaveBeenCalledTimes(1);
  });

  it('still says the shelf is empty when the list really is empty', () => {
    renderList();

    expect(screen.getByText('No references yet.')).toBeInTheDocument();
    expect(screen.queryByText(/Couldn't load/)).not.toBeInTheDocument();
  });
});
