import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { LessonSectionEditor } from '@/features/learning/lessons/edit/LessonSectionEditor';
import type { LessonSectionEditDto } from '@/lib/bindings';

const api = vi.hoisted(() => ({ edit: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: { editLearningLessonSection: api.edit } }));
vi.mock('@/features/reading/components/viewers/MarkdownViewer', () => ({
  MarkdownViewer: ({ content }: { content: string }) => <div data-testid="markdown-preview">{content}</div>,
}));

const ok = <T,>(data: T) => ({ ok: true, data });
const outcome = (overrides: Partial<LessonSectionEditDto> = {}): LessonSectionEditDto => ({
  changed: true, statementsKept: 0, statementsChanged: 0, statementsRemoved: 0, sentencesWritten: 0, checkedStatementsAffected: 0, revision: null, ...overrides,
});

const TITLE = 'Why water boils';
const BODY = 'Water boils when its vapour pressure meets the air pressure. The second sentence is here. A third follows.';
const REFUSAL = 'This section changed since you opened it. Copy your changes, close the editor and open it again.';
const NOT_CHECKED = 'What you write is marked as written by you, not checked.';

function show(overrides: Partial<Parameters<typeof LessonSectionEditor>[0]> = {}) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  const invalidate = vi.spyOn(client, 'invalidateQueries');
  const onClose = vi.fn();
  const onSaved = vi.fn();
  render(<QueryClientProvider client={client}>
    <LessonSectionEditor programId="program-1" lessonId="lesson-1" sectionIndex={2} title={TITLE} body={BODY} open onClose={onClose} onSaved={onSaved} {...overrides} />
  </QueryClientProvider>);
  return { invalidate, onClose, onSaved };
}

const textBox = () => screen.getByLabelText('Text') as HTMLTextAreaElement;
const titleBox = () => screen.getByLabelText('Title') as HTMLInputElement;
const requests = () => api.edit.mock.calls.map(([request]) => request);

/** Types into the text box with fake timers running and lets the dry run go out and answer. */
async function typeText(value: string, wait = 400) {
  fireEvent.change(textBox(), { target: { value } });
  await act(async () => { await vi.advanceTimersByTimeAsync(wait); });
}

describe('LessonSectionEditor', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    api.edit.mockResolvedValue(ok(outcome()));
  });
  afterEach(() => { vi.useRealTimers(); });

  it('opens with the section title and text and says nothing has changed', () => {
    show();
    expect(screen.getByRole('dialog', { name: 'Edit section 3' })).toBeVisible();
    expect(titleBox()).toHaveValue(TITLE);
    expect(textBox()).toHaveValue(BODY);
    expect(screen.getByText('No changes yet.')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    expect(screen.getByText("Saved changes can't be undone. You can edit the section again at any time.")).toBeVisible();
    expect(api.edit).not.toHaveBeenCalled();
  });

  it('shows what saving does from a dry run that goes out 400 ms after typing stops', async () => {
    vi.useFakeTimers();
    api.edit.mockResolvedValue(ok(outcome({ checkedStatementsAffected: 2 })));
    show();
    fireEvent.change(textBox(), { target: { value: `${BODY} More.` } });
    await act(async () => { await vi.advanceTimersByTimeAsync(399); });
    expect(api.edit).not.toHaveBeenCalled();
    await act(async () => { await vi.advanceTimersByTimeAsync(1); });
    expect(requests()).toEqual([{
      programId: 'program-1', lessonId: 'lesson-1', sectionIndex: 2, beforeTitle: TITLE, beforeBody: BODY, title: TITLE, body: `${BODY} More.`, dryRun: true,
    }]);
    expect(screen.getByText(`Saving changes 2 statements that were checked against the sources. ${NOT_CHECKED}`)).toBeVisible();
  });

  it('says one statement, in the singular, and nothing about checked statements when none are affected', async () => {
    vi.useFakeTimers();
    api.edit.mockResolvedValueOnce(ok(outcome({ checkedStatementsAffected: 1 }))).mockResolvedValueOnce(ok(outcome()));
    show();
    await typeText(`${BODY} More.`);
    expect(screen.getByText(`Saving changes 1 statement that was checked against the sources. ${NOT_CHECKED}`)).toBeVisible();
    await typeText(`${BODY} Even more.`);
    expect(screen.getByText('What you write is marked as written by you, not checked against the sources.')).toBeVisible();
  });

  it('keeps the previous line while a dry run is out and goes back to no changes when the text is put back', async () => {
    vi.useFakeTimers();
    api.edit.mockResolvedValueOnce(ok(outcome({ checkedStatementsAffected: 1 })));
    show();
    await typeText(`${BODY} More.`);
    const first = `Saving changes 1 statement that was checked against the sources. ${NOT_CHECKED}`;
    api.edit.mockReturnValueOnce(new Promise(() => {}));
    await typeText(`${BODY} More again.`);
    expect(screen.getByText(first)).toBeVisible();
    fireEvent.change(textBox(), { target: { value: BODY } });
    expect(screen.getByText('No changes yet.')).toBeVisible();
  });

  it('save sends the text as it was opened as the before text, then closes, reloads the review and announces it', async () => {
    const { invalidate, onClose, onSaved } = show();
    fireEvent.change(titleBox(), { target: { value: 'Why liquids boil' } });
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    const save = requests().find((request) => request.dryRun === false);
    expect(save).toEqual({
      programId: 'program-1', lessonId: 'lesson-1', sectionIndex: 2, beforeTitle: TITLE, beforeBody: BODY, title: 'Why liquids boil', body: BODY, dryRun: false,
    });
    expect(onSaved).toHaveBeenCalledTimes(1);
    const keys = invalidate.mock.calls.map(([filter]) => JSON.stringify(filter?.queryKey));
    expect(keys).toEqual(expect.arrayContaining([
      JSON.stringify(['learning-program', 'program-1']),
      JSON.stringify(['learning-lesson-evidence', 'program-1']),
      JSON.stringify(['learning-claim-review', 'program-1', 'lesson-1']),
    ]));
  });

  it('shows the Save button as Saving while it saves', async () => {
    let finish: (value: unknown) => void = () => {};
    api.edit.mockImplementation((request: { dryRun: boolean }) => (request.dryRun ? Promise.resolve(ok(outcome())) : new Promise((resolve) => { finish = resolve; })));
    show();
    fireEvent.change(titleBox(), { target: { value: 'Why liquids boil' } });
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect(await screen.findByRole('button', { name: 'Saving…' })).toBeDisabled();
    await act(async () => { finish(ok(outcome())); });
  });

  it('shows the refusal in an alert, keeps the text and stays open', async () => {
    api.edit.mockImplementation(async (request: { dryRun: boolean }) => (request.dryRun ? ok(outcome()) : { ok: false, error: REFUSAL }));
    const { onClose, onSaved } = show();
    fireEvent.change(textBox(), { target: { value: `${BODY} My own sentence.` } });
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(REFUSAL);
    expect(textBox()).toHaveValue(`${BODY} My own sentence.`);
    expect(onClose).not.toHaveBeenCalled();
    expect(onSaved).not.toHaveBeenCalled();
  });

  it('shows a refused save as a plain sentence', async () => {
    const sentence = 'This section changed since you opened it.';
    const message = sentence;
    api.edit.mockImplementation(async (request: { dryRun: boolean }) => (request.dryRun
      ? ok(outcome())
      : { ok: false, error: message, details: { code: 'INVALID_STATE', message, details: null } }));
    show();
    fireEvent.change(textBox(), { target: { value: `${BODY} My own sentence.` } });
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(sentence);
  });

  it('shows a refused dry run as a plain sentence', async () => {
    const sentence = 'This section changed since you opened it.';
    const message = sentence;
    api.edit.mockImplementation(async (request: { dryRun: boolean }) => (request.dryRun
      ? { ok: false, error: message, details: { code: 'INVALID_INPUT', message, details: null } }
      : ok(outcome())));
    show();
    fireEvent.change(textBox(), { target: { value: `${BODY} My own sentence.` } });
    expect(await screen.findByText(sentence)).toBeVisible();
  });

  it('a blank title disables Save, says so and runs no dry run', async () => {
    vi.useFakeTimers();
    show();
    fireEvent.change(titleBox(), { target: { value: '   ' } });
    await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
    expect(screen.getByText('A section needs a title.')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    expect(api.edit).not.toHaveBeenCalled();
  });

  it('blank text disables Save, says so and runs no dry run', async () => {
    vi.useFakeTimers();
    show();
    await typeText(' \n ', 1000);
    expect(screen.getByText('A section needs some text.')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    expect(api.edit).not.toHaveBeenCalled();
  });

  it('closes at once when nothing changed', async () => {
    const { onClose } = show();
    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('asks before throwing changes away, whichever way the editor is closed', async () => {
    const { onClose } = show();
    fireEvent.change(titleBox(), { target: { value: 'Why liquids boil' } });
    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(screen.getByText('Discard your changes?')).toBeVisible();
    expect(onClose).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole('button', { name: 'Keep editing' }));
    expect(screen.queryByText('Discard your changes?')).not.toBeInTheDocument();
    expect(titleBox()).toHaveValue('Why liquids boil');
    await userEvent.keyboard('{Escape}');
    expect(screen.getByText('Discard your changes?')).toBeVisible();
    expect(onClose).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole('button', { name: 'Discard' }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('opens with the sentence it was asked to focus selected', async () => {
    show({ focusText: 'The second sentence is here.' });
    const start = BODY.indexOf('The second sentence is here.');
    await waitFor(() => {
      expect(textBox().selectionStart).toBe(start);
      expect(textBox().selectionEnd).toBe(start + 'The second sentence is here.'.length);
    });
  });

  it('opens with nothing selected when the sentence is not in the text', async () => {
    show({ focusText: 'A sentence that is not there.' });
    await waitFor(() => expect(textBox()).toBeVisible());
    expect(textBox().selectionStart).toBe(textBox().selectionEnd);
  });

  it('previews the Markdown and goes back to editing', async () => {
    show();
    await userEvent.click(screen.getByRole('button', { name: 'Preview' }));
    expect(screen.getByTestId('markdown-preview')).toHaveTextContent(BODY);
    expect(textBox()).not.toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: 'Back to editing' }));
    expect(screen.queryByTestId('markdown-preview')).not.toBeInTheDocument();
    expect(textBox()).toBeVisible();
    expect(textBox()).toHaveValue(BODY);
  });
});
