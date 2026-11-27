import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useToastStore } from '@/stores/toastStore';

import { EditHistory } from './EditHistory';

import type { ContentEditHistory, ContentEditOutcome, FieldSpec } from './types';

const mocks = vi.hoisted(() => ({
  getContentEditHistory: vi.fn(),
  revertContentEdit: vi.fn(),
  refreshAffected: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('./api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./api')>();
  return {
    ...actual,
    getContentEditHistory: mocks.getContentEditHistory,
    revertContentEdit: mocks.revertContentEdit,
  };
});

vi.mock('./affected', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./affected')>();
  return {
    ...actual,
    refreshAffected: mocks.refreshAffected,
  };
});

const content = { kind: 'lesson.section', key: 's1' };
const form: FieldSpec[] = [
  { type: 'text', path: '/title', label: 'Title', format: 'line', required: true },
  { type: 'flag', path: '/published', label: 'Published', exclusive: false },
];

const sampleHistory: ContentEditHistory = {
  content,
  revision: 'rev-4',
  edits: [
    {
      id: 'edit-1',
      seq: 1,
      origin: { type: 'learner' },
      target: null,
      changes: [{ path: '/title', before: 'Draft', after: 'Quillet' }],
      createdAt: 1_700_000_001_000,
      revertsEditId: null,
      revertedBy: null,
      standing: true,
      revertible: true,
      later: 2,
    },
    {
      id: 'edit-2',
      seq: 2,
      origin: { type: 'learner' },
      target: null,
      changes: [{ path: '/published', before: false, after: true }],
      createdAt: 1_700_000_002_000,
      revertsEditId: null,
      revertedBy: 'edit-3',
      standing: false,
      revertible: true,
      later: 1,
    },
    {
      id: 'edit-3',
      seq: 3,
      origin: { type: 'revert', editId: 'edit-2' },
      target: null,
      changes: [{ path: '/published', before: true, after: false }],
      createdAt: 1_700_000_003_000,
      revertsEditId: 'edit-2',
      revertedBy: null,
      standing: true,
      revertible: true,
      later: 0,
    },
    {
      id: 'edit-4',
      seq: 4,
      origin: { type: 'action', name: 'unknown_action' },
      target: null,
      changes: [{ path: '/title', before: 'Quillet', after: null }],
      createdAt: 1_700_000_004_000,
      revertsEditId: null,
      revertedBy: null,
      standing: true,
      revertible: false,
      later: 0,
    },
  ],
};

function renderHistory() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <EditHistory content={content} form={form} />
    </QueryClientProvider>,
  );
}

describe('EditHistory', () => {
  beforeEach(() => {
    useToastStore.getState().dismissAll();
    mocks.getContentEditHistory.mockReset().mockResolvedValue(sampleHistory);
    mocks.revertContentEdit.mockReset();
    mocks.refreshAffected.mockReset().mockResolvedValue(undefined);
  });

  it('renders rows newest first with before/after labels and sends correct revert/redo payloads', async () => {
    const revertOutcome: ContentEditOutcome = {
      editId: 'edit-5',
      changed: true,
      content,
      scope: { feature: 'learning', container: 'lesson-1' },
      revision: 'rev-5',
      affected: ['lesson-outline'],
      consequences: [],
      reverted: ['edit-1'],
    };
    mocks.revertContentEdit.mockResolvedValue(revertOutcome);

    renderHistory();

    const undoLater = await screen.findByRole('button', { name: 'Undo this and 2 later changes' });
    const redoBtn = screen.getByRole('button', { name: 'Redo' });
    const undoBtn = screen.getByRole('button', { name: 'Undo' });

    const rows = screen.getAllByRole('listitem');
    expect(rows).toHaveLength(4);
    expect(rows[0]).toHaveTextContent('None');
    expect(rows[0]).not.toHaveTextContent('Undo');
    expect(rows[0]).not.toHaveTextContent('Redo');
    expect(rows[1]).toContainElement(undoBtn);
    expect(rows[2]).toContainElement(redoBtn);
    expect(rows[2]).toHaveTextContent('No');
    expect(rows[2]).toHaveTextContent('Yes');
    expect(rows[3]).toContainElement(undoLater);
    expect(rows[3]).toHaveTextContent('Draft');
    expect(rows[3]).toHaveTextContent('Quillet');

    fireEvent.click(undoLater);
    await waitFor(() => expect(mocks.revertContentEdit).toHaveBeenCalledTimes(1));
    expect(mocks.revertContentEdit.mock.calls[0][0]).toMatchObject({
      editId: 'edit-1',
      expectedRevision: 'rev-4',
      andLater: true,
    });
    expect(useToastStore.getState().toasts.some((t) => t.title === 'Undone.')).toBe(true);

    fireEvent.click(redoBtn);
    await waitFor(() => expect(mocks.revertContentEdit).toHaveBeenCalledTimes(2));
    expect(mocks.revertContentEdit.mock.calls[1][0]).toMatchObject({
      editId: 'edit-2',
      expectedRevision: 'rev-4',
      andLater: false,
    });
    expect(useToastStore.getState().toasts.some((t) => t.title === 'Redone.')).toBe(true);
  });
});
