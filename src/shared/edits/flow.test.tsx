import { useMemo } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ToastContainer } from '@/components/Toast/ToastContainer';
import { useKeyboardShortcuts } from '@/hooks/useKeyboardShortcuts';
import { clearDraft, flushDrafts } from '@/shared/drafts/drafts';
import { useToastStore } from '@/stores/toastStore';

import { registerAffected } from './affected';
import { ContentEditor } from './ContentEditor';
import { Editable } from './Editable';
import { useEditMode } from './editMode';
import { EditModeBar } from './EditModeBar';
import { useEditorStore } from './editorStore';

import type { ContentEditOutcome, ContentEditPreview, EditableContent } from './types';

const mocks = vi.hoisted(() => ({
  openContentEdit: vi.fn(),
  previewContentEdit: vi.fn(),
  applyContentEdit: vi.fn(),
  revertContentEdit: vi.fn(),
  getContentEditHistory: vi.fn(),
  affectedRefresh: vi.fn(),
}));

vi.mock('./api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./api')>();
  return {
    ...actual,
    openContentEdit: mocks.openContentEdit,
    previewContentEdit: mocks.previewContentEdit,
    applyContentEdit: mocks.applyContentEdit,
    revertContentEdit: mocks.revertContentEdit,
    getContentEditHistory: mocks.getContentEditHistory,
  };
});

const content = { kind: 'learning.section', key: 'lesson-1/0' };
const draftKey = `edit:${content.kind}:${content.key}`;
const initialBody = 'First paragraph\n\nTarget block\n\nLast paragraph';
const updatedBody = 'First paragraph\n\nUpdated block\n\nLast paragraph';
const targetStart = initialBody.indexOf('Target block');
const targetEnd = targetStart + 'Target block'.length;

function FlowHarness() {
  const toggleEditMode = useEditMode((state) => state.toggle);
  const shortcuts = useMemo(
    () => [
      {
        id: 'global.editMode',
        keys: 'Mod+Shift+E',
        description: 'Edit mode',
        category: 'global' as const,
        handler: toggleEditMode,
        global: true,
      },
    ],
    [toggleEditMode],
  );
  useKeyboardShortcuts(shortcuts, { enabled: true });

  return (
    <>
      <EditModeBar />
      <Editable
        content={content}
        label="section 1: Quillet overview"
        path="/body"
        source={initialBody}
        sourceStart={0}
      >
        <article>
          <h2>Quillet overview</h2>
          <p data-source-lines="2-3">Target block</p>
        </article>
      </Editable>
      <ContentEditor />
      <ToastContainer />
    </>
  );
}

describe('edit mode end-to-end UI flow', () => {
  let queryClient: QueryClient;

  beforeEach(() => {
    clearDraft(draftKey);
    flushDrafts();
    localStorage.clear();
    useEditMode.setState({ on: false });
    useEditorStore.setState({ current: null, next: null });
    useToastStore.getState().dismissAll();
    queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });

    registerAffected('learning.program', mocks.affectedRefresh);
    mocks.affectedRefresh.mockReset().mockResolvedValue(undefined);

    const opened: EditableContent = {
      content,
      label: 'Section 1: Quillet overview',
      scope: { feature: 'learning', container: 'program-1' },
      editable: { state: 'editable' },
      form: [
        { type: 'text', path: '/title', label: 'Title', format: 'line', required: true },
        { type: 'text', path: '/body', label: 'Body', format: 'markdown', required: true },
      ],
      document: {
        title: 'Quillet overview',
        body: initialBody,
      },
      revision: 'rev-open-1',
      notes: [],
      editedPaths: [],
    };
    const emptyPreview: ContentEditPreview = {
      current: true,
      changed: false,
      problems: [],
      notices: [],
      consequences: [],
    };
    const applyOutcome: ContentEditOutcome = {
      editId: 'edit-flow-1',
      changed: true,
      content,
      scope: { feature: 'learning', container: 'program-1' },
      revision: 'rev-after-2',
      affected: ['learning.program'],
      consequences: [],
      reverted: [],
    };
    const revertOutcome: ContentEditOutcome = {
      editId: 'revert-flow-1',
      changed: true,
      content,
      scope: { feature: 'learning', container: 'program-1' },
      revision: 'rev-open-1',
      affected: ['learning.program'],
      consequences: [],
      reverted: ['edit-flow-1'],
    };

    mocks.openContentEdit.mockReset().mockResolvedValue(opened);
    mocks.previewContentEdit.mockReset().mockResolvedValue(emptyPreview);
    mocks.applyContentEdit.mockReset().mockResolvedValue(applyOutcome);
    mocks.revertContentEdit.mockReset().mockResolvedValue(revertOutcome);
    mocks.getContentEditHistory.mockReset().mockResolvedValue({
      content,
      revision: 'rev-open-1',
      edits: [],
    });
  });

  afterEach(() => {
    clearDraft(draftKey);
    flushDrafts();
    localStorage.clear();
    useEditMode.setState({ on: false });
    useEditorStore.setState({ current: null, next: null });
    useToastStore.getState().dismissAll();
    queryClient.clear();
    vi.restoreAllMocks();
  });

  it('toggles edit mode by shortcut, opens with the clicked block selected, saves a replace op, and undoes from the toast', async () => {
    render(
      <QueryClientProvider client={queryClient}>
        <FlowHarness />
      </QueryClientProvider>,
    );

    expect(screen.queryByRole('region', { name: 'Edit mode' })).not.toBeInTheDocument();

    fireEvent.keyDown(document.body, { key: 'E', ctrlKey: true, shiftKey: true });
    expect(useEditMode.getState().on).toBe(true);
    expect(screen.getByRole('region', { name: 'Edit mode' })).toBeInTheDocument();

    fireEvent.click(screen.getByText('Target block'));
    const bodyField = (await screen.findByLabelText('Body')) as HTMLTextAreaElement;
    await waitFor(() => {
      expect(bodyField.selectionStart).toBe(targetStart);
      expect(bodyField.selectionEnd).toBe(targetEnd);
    });
    expect(bodyField.value.slice(bodyField.selectionStart, bodyField.selectionEnd)).toBe('Target block');

    fireEvent.change(bodyField, { target: { value: updatedBody } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => expect(mocks.applyContentEdit).toHaveBeenCalledTimes(1));
    const applyRequest = mocks.applyContentEdit.mock.calls[0][0];
    expect(applyRequest).toMatchObject({
      content,
      expectedRevision: 'rev-open-1',
      ops: [{ op: 'replace', path: '/body', value: updatedBody }],
      source: { type: 'learner' },
      target: {
        content,
        path: '/body',
        fragment: {
          exact: 'Target block',
          prefix: null,
          suffix: null,
          startByte: targetStart,
          endByte: targetEnd,
        },
      },
    });
    expect(mocks.affectedRefresh).toHaveBeenCalledTimes(1);
    expect(mocks.affectedRefresh).toHaveBeenLastCalledWith(
      { feature: 'learning', container: 'program-1' },
      queryClient,
    );

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(await screen.findByText('Saved.')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Undo' }));

    await waitFor(() => expect(mocks.revertContentEdit).toHaveBeenCalledTimes(1));
    expect(mocks.revertContentEdit).toHaveBeenCalledWith({
      revertId: expect.any(String),
      editId: 'edit-flow-1',
      expectedRevision: 'rev-after-2',
      andLater: false,
    });
    await waitFor(() => expect(mocks.affectedRefresh).toHaveBeenCalledTimes(2));
    expect(await screen.findByText('Undone.')).toBeInTheDocument();
  });
});
