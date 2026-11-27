import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { clearDraft, flushDrafts, readDraft } from '@/shared/drafts/drafts';
import { useToastStore } from '@/stores/toastStore';

import { ContentEditor } from './ContentEditor';
import { openEditor, useEditorStore } from './editorStore';

import type { ContentEditOutcome, ContentEditPreview, EditableContent, FieldSpec } from './types';

const mocks = vi.hoisted(() => ({
  openContentEdit: vi.fn(),
  previewContentEdit: vi.fn(),
  applyContentEdit: vi.fn(),
  revertContentEdit: vi.fn(),
  getContentEditHistory: vi.fn(),
  refreshAffected: vi.fn().mockResolvedValue(undefined),
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

vi.mock('./affected', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./affected')>();
  return {
    ...actual,
    refreshAffected: mocks.refreshAffected,
  };
});

const content = { kind: 'lesson.section', key: 's1' };
const draftKey = 'edit:lesson.section:s1';

const allVariantsForm: FieldSpec[] = [
  { type: 'text', path: '/title', label: 'Title', format: 'line', required: true },
  { type: 'text', path: '/summary', label: 'Summary', format: 'plain', required: false },
  { type: 'text', path: '/body', label: 'Body', format: 'markdown', required: true },
  { type: 'flag', path: '/featured', label: 'Featured', exclusive: false },
  { type: 'choice', path: '/category', label: 'Category', options: ['Concept', 'Example'] },
  { type: 'number', path: '/minutes', label: 'Time', min: 1, unit: 'minutes' },
  {
    type: 'list',
    path: '/choices',
    label: 'Choices',
    itemLabel: 'Choice',
    minItems: 2,
    maxItems: 3,
    item: [
      { type: 'text', path: '/text', label: 'Choice text', format: 'line', required: true },
      { type: 'flag', path: '/correct', label: 'Correct', exclusive: true },
    ],
  },
];

function makeEditable(overrides: Partial<EditableContent> = {}): EditableContent {
  return {
    content,
    label: 'Section 1',
    scope: { feature: 'learning', container: 'lesson-1' },
    editable: { state: 'editable' },
    form: allVariantsForm,
    document: {
      title: 'Quillet Title',
      summary: 'Short summary',
      body: 'First\nTarget block\nLast',
      featured: false,
      category: 0,
      minutes: 5,
      choices: [
        { text: 'Alpha', correct: true },
        { text: 'Beta', correct: false },
      ],
    },
    revision: 'rev-1',
    notes: [],
    editedPaths: [],
    ...overrides,
  };
}

const emptyPreview: ContentEditPreview = {
  current: true,
  changed: false,
  problems: [],
  notices: [],
  consequences: [],
};

function renderEditor() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <ContentEditor />
    </QueryClientProvider>,
  );
}

describe('ContentEditor', () => {
  beforeEach(() => {
    clearDraft(draftKey);
    flushDrafts();
    localStorage.clear();
    useEditorStore.setState({ current: null, next: null });
    useToastStore.getState().dismissAll();
    mocks.openContentEdit.mockReset().mockResolvedValue(makeEditable());
    mocks.previewContentEdit.mockReset().mockResolvedValue(emptyPreview);
    mocks.applyContentEdit.mockReset();
    mocks.revertContentEdit.mockReset();
    mocks.getContentEditHistory.mockReset().mockResolvedValue({ content, revision: 'rev-1', edits: [] });
    mocks.refreshAffected.mockReset().mockResolvedValue(undefined);
  });

  afterEach(() => {
    clearDraft(draftKey);
    flushDrafts();
    vi.restoreAllMocks();
  });

  it('renders every field variant, enforces list bounds, and clears exclusive sibling flags', async () => {
    renderEditor();
    openEditor({ content, path: null, fragment: null });

    expect(await screen.findByLabelText('Title')).toHaveValue('Quillet Title');
    expect(screen.getByLabelText('Summary')).toHaveValue('Short summary');
    expect(screen.getByLabelText('Body')).toHaveValue('First\nTarget block\nLast');
    expect(screen.getByRole('region', { name: 'Preview' })).toBeInTheDocument();
    expect(screen.getByLabelText('Featured')).not.toBeChecked();
    expect(screen.getByRole('radiogroup', { name: 'Category' })).toBeInTheDocument();
    expect(screen.getByLabelText('Time')).toHaveValue(5);

    const remove1 = screen.getByRole('button', { name: 'Remove Choice 1' });
    const remove2 = screen.getByRole('button', { name: 'Remove Choice 2' });
    const addBtn = screen.getByRole('button', { name: 'Add Choice' });
    expect(remove1).toBeDisabled();
    expect(remove2).toBeDisabled();
    expect(addBtn).toBeEnabled();

    fireEvent.click(addBtn);
    expect(screen.getByRole('button', { name: 'Remove Choice 1' })).toBeEnabled();
    expect(addBtn).toBeDisabled();

    const choice1 = screen.getByRole('group', { name: 'Choice 1' });
    const choice2 = screen.getByRole('group', { name: 'Choice 2' });
    const flag1 = within(choice1).getByRole('radio', { name: 'Correct' });
    const flag2 = within(choice2).getByRole('radio', { name: 'Correct' });
    expect(flag1).toBeChecked();
    expect(flag2).not.toBeChecked();

    fireEvent.click(flag2);
    expect(flag2).toBeChecked();
    expect(flag1).not.toBeChecked();
  });

  it('focuses and selects the aimed range when source matches, and collapses selection otherwise', async () => {
    const simpleEditable = makeEditable({
      form: [{ type: 'text', path: '/body', label: 'Body', format: 'markdown', required: true }],
      document: { body: 'First\nTarget block\nLast' },
    });
    mocks.openContentEdit.mockResolvedValue(simpleEditable);

    renderEditor();
    openEditor(
      { content, path: '/body', fragment: null },
      { aim: { lines: [1, 2], source: 'First\nTarget block\nLast', sourceStart: 0 } },
    );

    const textarea = (await screen.findByLabelText('Body')) as HTMLTextAreaElement;
    await waitFor(() => {
      expect(textarea.selectionStart).toBe('First\n'.length);
      expect(textarea.selectionEnd).toBe('First\nTarget block'.length);
    });

    act(() => useEditorStore.setState({ current: null, next: null }));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    act(() => {
      openEditor(
        { content, path: '/body', fragment: null },
        { aim: { lines: [1, 2], source: 'Different text', sourceStart: 0 } },
      );
    });

    const nextTextarea = (await screen.findByLabelText('Body')) as HTMLTextAreaElement;
    await waitFor(() => {
      expect(nextTextarea).toHaveFocus();
      expect(nextTextarea.selectionStart).toBe(nextTextarea.selectionEnd);
    });
  });

  it('saves operations, refreshes affected data, restores focus, toasts Undo, and retries with the same editId', async () => {
    const simpleEditable = makeEditable({
      form: [{ type: 'text', path: '/title', label: 'Title', format: 'line', required: true }],
      document: { title: 'Quillet Title' },
    });
    mocks.openContentEdit.mockResolvedValue(simpleEditable);
    const outcome: ContentEditOutcome = {
      editId: 'edit-1',
      changed: true,
      content,
      scope: { feature: 'learning', container: 'lesson-1' },
      revision: 'rev-2',
      affected: ['lesson-outline'],
      consequences: [],
      reverted: [],
    };
    mocks.applyContentEdit
      .mockRejectedValueOnce(new Error('Disk full'))
      .mockResolvedValueOnce(outcome);

    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={client}>
        <button type="button" data-testid="opener">Open edit</button>
        <ContentEditor />
      </QueryClientProvider>,
    );
    const opener = screen.getByTestId('opener');
    openEditor({ content, path: null, fragment: null }, { returnFocusTo: opener });

    const input = await screen.findByLabelText('Title');
    fireEvent.change(input, { target: { value: 'Updated Title' } });

    const saveButton = screen.getByRole('button', { name: 'Save' });
    fireEvent.click(saveButton);

    const retryButton = await screen.findByRole('button', { name: 'Try again' });
    expect(screen.getByRole('alert')).toHaveTextContent('Disk full');
    const firstCall = mocks.applyContentEdit.mock.calls[0][0];
    expect(firstCall.ops).toEqual([{ op: 'replace', path: '/title', value: 'Updated Title' }]);
    expect(firstCall.expectedRevision).toBe('rev-1');
    expect(firstCall.source).toEqual({ type: 'learner' });

    fireEvent.click(retryButton);
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());

    const secondCall = mocks.applyContentEdit.mock.calls[1][0];
    expect(secondCall.editId).toBe(firstCall.editId);
    expect(mocks.refreshAffected).toHaveBeenCalledWith(['lesson-outline'], { feature: 'learning', container: 'lesson-1' }, client);
    expect(opener).toHaveFocus();

    const savedToast = useToastStore.getState().toasts.find((t) => t.title === 'Saved.');
    expect(savedToast?.action?.label).toBe('Undo');
  });

  it('shows conflict view on CONFLICT and saves rebased draft with new revision and editId', async () => {
    const rev1 = makeEditable({
      form: [{ type: 'text', path: '/title', label: 'Title', format: 'line', required: true }],
      document: { title: 'Original' },
      revision: 'rev-1',
    });
    const rev2 = makeEditable({
      form: [{ type: 'text', path: '/title', label: 'Title', format: 'line', required: true }],
      document: { title: 'Remote Title' },
      revision: 'rev-2',
    });
    mocks.openContentEdit.mockResolvedValueOnce(rev1).mockResolvedValueOnce(rev2);
    const conflictErr = Object.assign(new Error('Conflict'), { code: 'CONFLICT' });
    const resolvedOutcome: ContentEditOutcome = {
      editId: 'edit-2',
      changed: true,
      content,
      scope: { feature: 'learning', container: 'lesson-1' },
      revision: 'rev-3',
      affected: [],
      consequences: [],
      reverted: [],
    };
    mocks.applyContentEdit.mockRejectedValueOnce(conflictErr).mockResolvedValueOnce(resolvedOutcome);

    renderEditor();
    openEditor({ content, path: null, fragment: null });

    const input = await screen.findByLabelText('Title');
    fireEvent.change(input, { target: { value: 'My Draft Title' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('This changed since you opened it.');
    expect(screen.getByText('Remote Title')).toBeInTheDocument();
    expect(screen.getByText('My Draft Title')).toBeInTheDocument();
    expect(readDraft(draftKey)).not.toBe('');

    const firstEditId = mocks.applyContentEdit.mock.calls[0][0].editId;
    fireEvent.click(screen.getByRole('button', { name: 'Save my draft over this' }));

    await waitFor(() => expect(mocks.applyContentEdit).toHaveBeenCalledTimes(2));
    const secondCall = mocks.applyContentEdit.mock.calls[1][0];
    expect(secondCall.expectedRevision).toBe('rev-2');
    expect(secondCall.editId).not.toBe(firstEditId);
    expect(secondCall.ops).toEqual([{ op: 'replace', path: '/title', value: 'My Draft Title' }]);
  });

  it('shows reason, omits Save, and locks controls when editable state is not_now', async () => {
    mocks.openContentEdit.mockResolvedValue(
      makeEditable({
        editable: { state: 'not_now', reason: 'Locked during synthesis.' },
      }),
    );

    renderEditor();
    openEditor({ content, path: null, fragment: null });

    expect(await screen.findByText('Locked during synthesis.')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Save' })).not.toBeInTheDocument();
    expect(screen.getByLabelText('Title')).toHaveAttribute('readonly');
  });

  it('announces field problems via aria-describedby and queues related content to open after closing', async () => {
    const relatedContent = { kind: 'lesson', key: 'l-next' };
    const simpleEditable = makeEditable({
      form: [{ type: 'text', path: '/title', label: 'Title', format: 'line', required: true }],
      document: { title: 'Quillet' },
    });
    mocks.openContentEdit.mockResolvedValue(simpleEditable);
    mocks.previewContentEdit
      .mockResolvedValueOnce(emptyPreview)
      .mockResolvedValueOnce({
        current: true,
        changed: true,
        problems: [{ path: '/title', message: 'Title needs content.' }],
        notices: [],
        consequences: [
          {
            code: 'related',
            message: 'Affects linked lesson.',
            related: [{ content: relatedContent, label: 'Linked Lesson' }],
          },
        ],
      });

    renderEditor();
    openEditor({ content, path: null, fragment: null });

    const input = await screen.findByLabelText('Title');
    fireEvent.change(input, { target: { value: '' } });

    const problemAlert = await screen.findByRole('alert', undefined, { timeout: 2000 });
    expect(problemAlert).toHaveTextContent('Title needs content.');
    expect(input).toHaveAccessibleDescription('Title needs content.');

    fireEvent.click(screen.getByRole('button', { name: 'Linked Lesson' }));
    expect(useEditorStore.getState().current?.address.content).toEqual(content);

    fireEvent.click(screen.getByRole('button', { name: 'Discard changes' }));
    await waitFor(() => expect(useEditorStore.getState().current?.address.content).toEqual(relatedContent));
  });
});
