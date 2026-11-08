import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { LessonQuestionEditor } from '@/features/learning/lessons/edit/LessonQuestionEditor';

type Question = { kind: string; prompt: string; choices: string[]; right: number | null; explanation: string; rationales: string[] };

const api = vi.hoisted(() => ({ get: vi.fn(), edit: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: { getLearningLessonQuestion: api.get, editLearningLessonQuestion: api.edit } }));

const ok = <T,>(data: T) => ({ ok: true, data });
const outcome = (overrides: Record<string, unknown> = {}) => ({
  changed: true, statementsKept: 0, statementsChanged: 0, statementsRemoved: 0, sentencesWritten: 0, checkedStatementsAffected: 0, revision: null, ...overrides,
});

const QUESTION: Question = {
  kind: 'single_choice',
  prompt: 'What does DictReader do with spaces in a header name?',
  choices: ['It keeps them.', 'It trims them.', 'It drops the column.', 'It raises an error.'],
  right: 0,
  explanation: 'Header names keep their spaces exactly as written.',
  rationales: ['', 'Nothing trims a header name.', 'The column stays in the file.', 'No error is raised for spaces.'],
};
const TRUE_FALSE: Question = {
  kind: 'true_false', prompt: 'DictReader reads every row as a list.', choices: ['True', 'False'], right: 1, explanation: 'It reads each row as a dictionary.', rationales: [],
};
const reasonBox = (n: number) => screen.getByRole('textbox', { name: `Why choice ${n} is wrong` }) as HTMLTextAreaElement;
const REFUSAL = 'This question changed since you opened it. Copy your changes, close the editor and open it again.';

function show(question: Question = QUESTION) {
  api.get.mockResolvedValue(ok(question));
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  const invalidate = vi.spyOn(client, 'invalidateQueries');
  const onClose = vi.fn();
  const onSaved = vi.fn();
  render(<QueryClientProvider client={client}>
    <LessonQuestionEditor programId="program-1" lessonId="lesson-1" questionIndex={1} open onClose={onClose} onSaved={onSaved} />
  </QueryClientProvider>);
  return { invalidate, onClose, onSaved };
}

const loaded = () => screen.findByRole('textbox', { name: 'Question' }) as Promise<HTMLTextAreaElement>;
const choiceBox = (n: number) => screen.getByRole('textbox', { name: `Choice ${n}` }) as HTMLInputElement;
const radios = () => screen.getAllByRole('radio', { name: /^Right answer: choice \d+$/ }) as HTMLInputElement[];
const removes = () => screen.getAllByRole('button', { name: 'Remove' });
const saveButton = () => screen.getByRole('button', { name: 'Save' });
const requests = () => api.edit.mock.calls.map(([request]) => request);

describe('LessonQuestionEditor', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    api.edit.mockResolvedValue(ok(outcome()));
  });
  afterEach(() => { vi.useRealTimers(); });

  it('loads the question when it opens and shows its prompt, choices, right answer and explanation', async () => {
    show();
    expect(screen.getByRole('dialog', { name: 'Edit question 2' })).toBeVisible();
    expect(await loaded()).toHaveValue(QUESTION.prompt);
    expect(api.get).toHaveBeenCalledWith('program-1', 'lesson-1', 1);
    QUESTION.choices.forEach((choice, index) => expect(choiceBox(index + 1)).toHaveValue(choice));
    expect(radios().map((radio) => radio.checked)).toEqual([true, false, false, false]);
    expect(screen.getByRole('textbox', { name: 'Explanation' })).toHaveValue(QUESTION.explanation);
    expect(screen.getByRole('button', { name: 'Add a choice' })).toBeEnabled();
    expect(screen.getByText('No changes yet.')).toBeVisible();
    expect(saveButton()).toBeDisabled();
    expect(api.edit).not.toHaveBeenCalled();
  });

  it('labels each right-answer radio by its choice, and renumbers them when a choice goes', async () => {
    show();
    await loaded();
    expect(radios().map((radio) => radio.getAttribute('aria-label'))).toEqual([
      'Right answer: choice 1', 'Right answer: choice 2', 'Right answer: choice 3', 'Right answer: choice 4',
    ]);
    await userEvent.click(removes()[0]);
    expect(radios().map((radio) => radio.getAttribute('aria-label'))).toEqual([
      'Right answer: choice 1', 'Right answer: choice 2', 'Right answer: choice 3',
    ]);
  });

  it('saves the edited prompt, choices, right answer and explanation in one request, then closes, reloads and announces it', async () => {
    const { invalidate, onClose, onSaved } = show();
    fireEvent.change(await loaded(), { target: { value: 'What does DictReader do with a header name that holds spaces?' } });
    fireEvent.change(choiceBox(2), { target: { value: 'It trims them silently.' } });
    await userEvent.click(radios()[2]);
    fireEvent.change(screen.getByRole('textbox', { name: 'Explanation' }), { target: { value: 'Trimming is up to you.' } });
    await userEvent.click(saveButton());
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    const saves = requests().filter((request) => request.dryRun === false);
    expect(saves).toEqual([{
      programId: 'program-1', lessonId: 'lesson-1', questionIndex: 1, dryRun: false,
      before: QUESTION,
      after: {
        kind: 'single_choice', prompt: 'What does DictReader do with a header name that holds spaces?',
        choices: ['It keeps them.', 'It trims them silently.', 'It drops the column.', 'It raises an error.'], right: 2, explanation: 'Trimming is up to you.',
        rationales: ['', 'Nothing trims a header name.', '', 'No error is raised for spaces.'],
      },
    }]);
    expect(onSaved).toHaveBeenCalledTimes(1);
    const keys = invalidate.mock.calls.map(([filter]) => JSON.stringify(filter?.queryKey));
    expect(keys).toEqual(expect.arrayContaining([
      JSON.stringify(['learning-program', 'program-1']),
      JSON.stringify(['learning-lesson-evidence', 'program-1']),
      JSON.stringify(['learning-claim-review', 'program-1', 'lesson-1']),
    ]));
  });

  it('shows what saving does from a dry run that goes out 400 ms after typing stops', async () => {
    show();
    const prompt = await loaded();
    vi.useFakeTimers();
    api.edit.mockResolvedValue(ok(outcome({ checkedStatementsAffected: 2 })));
    fireEvent.change(prompt, { target: { value: `${QUESTION.prompt} Why?` } });
    await act(async () => { await vi.advanceTimersByTimeAsync(399); });
    expect(api.edit).not.toHaveBeenCalled();
    await act(async () => { await vi.advanceTimersByTimeAsync(1); });
    expect(requests()).toEqual([{
      programId: 'program-1', lessonId: 'lesson-1', questionIndex: 1, dryRun: true,
      before: QUESTION, after: { ...QUESTION, prompt: `${QUESTION.prompt} Why?` },
    }]);
    expect(screen.getByText('Saving changes 2 statements that were checked against the sources. What you write is marked as written by you, not checked.')).toBeVisible();
  });

  it('shows a reason field under each wrong choice and none under the right one', async () => {
    show();
    await loaded();
    expect(screen.queryByRole('textbox', { name: 'Why choice 1 is wrong' })).not.toBeInTheDocument();
    [2, 3, 4].forEach((n) => expect(reasonBox(n)).toHaveValue(QUESTION.rationales[n - 1]));
  });

  it('sends an edited reason with the question, and the others as they were', async () => {
    show();
    await loaded();
    fireEvent.change(reasonBox(3), { target: { value: 'The column stays, spaces and all.' } });
    await userEvent.click(saveButton());
    await waitFor(() => expect(api.edit).toHaveBeenCalledWith(expect.objectContaining({ dryRun: false })));
    const [save] = requests().filter((request) => request.dryRun === false);
    expect(save.before).toEqual(QUESTION);
    expect(save.after.rationales).toEqual(['', 'Nothing trims a header name.', 'The column stays, spaces and all.', 'No error is raised for spaces.']);
  });

  it('keeps each reason with its choice when a choice is removed or added', async () => {
    show();
    await loaded();
    await userEvent.click(removes()[1]);
    expect(reasonBox(2)).toHaveValue(QUESTION.rationales[2]);
    expect(reasonBox(3)).toHaveValue(QUESTION.rationales[3]);
    await userEvent.click(screen.getByRole('button', { name: 'Add a choice' }));
    await userEvent.type(choiceBox(4), 'It ignores them.');
    await userEvent.type(reasonBox(4), 'Nothing ignores a header name.');
    await userEvent.click(saveButton());
    await waitFor(() => expect(api.edit).toHaveBeenCalledWith(expect.objectContaining({ dryRun: false })));
    const [save] = requests().filter((request) => request.dryRun === false);
    expect(save.after.choices).toEqual(['It keeps them.', 'It drops the column.', 'It raises an error.', 'It ignores them.']);
    expect(save.after.rationales).toEqual(['', 'The column stays in the file.', 'No error is raised for spaces.', 'Nothing ignores a header name.']);
  });

  it('moves the empty reason to the new right answer when the right answer changes', async () => {
    show();
    await loaded();
    await userEvent.click(radios()[2]);
    expect(screen.queryByRole('textbox', { name: 'Why choice 3 is wrong' })).not.toBeInTheDocument();
    expect(reasonBox(1)).toHaveValue('');
    await userEvent.type(reasonBox(1), 'It does keep them, so this is not wrong.');
    await userEvent.click(saveButton());
    await waitFor(() => expect(api.edit).toHaveBeenCalledWith(expect.objectContaining({ dryRun: false })));
    const [save] = requests().filter((request) => request.dryRun === false);
    expect(save.after.right).toBe(2);
    expect(save.after.rationales).toEqual(['It does keep them, so this is not wrong.', 'Nothing trims a header name.', '', 'No error is raised for spaces.']);
  });

  it('keeps a reason written for a choice when the right answer moves away and back', async () => {
    show();
    await loaded();
    await userEvent.click(radios()[2]);
    await userEvent.click(radios()[0]);
    expect(reasonBox(3)).toHaveValue('The column stays in the file.');
    expect(screen.getByText('No changes yet.')).toBeInTheDocument();
    expect(saveButton()).toBeDisabled();
  });

  it('a question stored without reasons shows empty fields and is not changed by showing them', async () => {
    show({ ...QUESTION, rationales: [] });
    await loaded();
    [2, 3, 4].forEach((n) => expect(reasonBox(n)).toHaveValue(''));
    expect(screen.getByText('No changes yet.')).toBeVisible();
    expect(saveButton()).toBeDisabled();
    fireEvent.change(await loaded(), { target: { value: `${QUESTION.prompt} Why?` } });
    await userEvent.click(saveButton());
    await waitFor(() => expect(api.edit).toHaveBeenCalledWith(expect.objectContaining({ dryRun: false })));
    const [save] = requests().filter((request) => request.dryRun === false);
    expect(save.after.rationales).toEqual([]);
  });

  it('cannot remove a choice at two choices', async () => {
    show();
    await loaded();
    await userEvent.click(removes()[3]);
    await userEvent.click(removes()[2]);
    expect(screen.getAllByRole('textbox', { name: /^Choice / })).toHaveLength(2);
    removes().forEach((button) => expect(button).toBeDisabled());
  });

  it('removing a choice keeps the right answer on the same choice, and removing the right one leaves it unchosen', async () => {
    show({ ...QUESTION, right: 2 });
    await loaded();
    await userEvent.click(removes()[0]);
    expect(radios().map((radio) => radio.checked)).toEqual([false, true, false]);
    await userEvent.click(removes()[1]);
    expect(radios().every((radio) => !radio.checked)).toBe(true);
    expect(screen.getByText('Choose the right answer.')).toBeVisible();
    expect(saveButton()).toBeDisabled();
    expect(api.edit).not.toHaveBeenCalled();
  });

  it('adds a choice, and a blank choice is refused without a dry run', async () => {
    show();
    await loaded();
    await userEvent.click(screen.getByRole('button', { name: 'Add a choice' }));
    expect(screen.getAllByRole('textbox', { name: /^Choice / })).toHaveLength(5);
    expect(choiceBox(5)).toHaveValue('');
    expect(screen.getByText('Each choice needs some text.')).toBeVisible();
    expect(saveButton()).toBeDisabled();
    expect(api.edit).not.toHaveBeenCalled();
    await userEvent.type(choiceBox(5), 'It ignores them.');
    expect(screen.queryByText('Each choice needs some text.')).not.toBeInTheDocument();
    expect(saveButton()).toBeEnabled();
  });

  it('a blank prompt disables Save and says so', async () => {
    show();
    fireEvent.change(await loaded(), { target: { value: '  ' } });
    expect(screen.getByText('A question needs a prompt.')).toBeVisible();
    expect(saveButton()).toBeDisabled();
    expect(api.edit).not.toHaveBeenCalled();
  });

  it('a true or false question has only its right answer to choose', async () => {
    const { onClose } = show(TRUE_FALSE);
    await loaded();
    const group = screen.getByRole('radiogroup', { name: 'Right answer' });
    const isTrue = within(group).getByRole('radio', { name: 'True' }) as HTMLInputElement;
    const isFalse = within(group).getByRole('radio', { name: 'False' }) as HTMLInputElement;
    expect([isTrue.checked, isFalse.checked]).toEqual([false, true]);
    expect(screen.queryByRole('button', { name: 'Add a choice' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Remove' })).not.toBeInTheDocument();
    expect(screen.queryByRole('textbox', { name: /^Choice / })).not.toBeInTheDocument();
    await userEvent.click(isTrue);
    await userEvent.click(saveButton());
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(requests().filter((request) => request.dryRun === false)).toEqual([{
      programId: 'program-1', lessonId: 'lesson-1', questionIndex: 1, dryRun: false,
      before: TRUE_FALSE, after: { ...TRUE_FALSE, right: 0 },
    }]);
  });

  it('shows the refusal in an alert, keeps the text and stays open', async () => {
    api.edit.mockImplementation(async (request: { dryRun: boolean }) => (request.dryRun ? ok(outcome()) : { ok: false, error: REFUSAL }));
    const { onClose, onSaved } = show();
    fireEvent.change(await loaded(), { target: { value: 'My own wording.' } });
    await userEvent.click(saveButton());
    expect(await screen.findByRole('alert')).toHaveTextContent(REFUSAL);
    expect(screen.getByRole('textbox', { name: 'Question' })).toHaveValue('My own wording.');
    expect(onClose).not.toHaveBeenCalled();
    expect(onSaved).not.toHaveBeenCalled();
  });

  it('shows a refused dry run as a plain sentence', async () => {
    const sentence = "The question's stem gives away its answer.";
    const message = sentence;
    api.edit.mockImplementation(async (request: { dryRun: boolean }) => (request.dryRun
      ? { ok: false, error: message, details: { code: 'INVALID_INPUT', message, details: null } }
      : ok(outcome())));
    show();
    fireEvent.change(await loaded(), { target: { value: 'My own wording.' } });
    expect(await screen.findByText(sentence)).toBeVisible();
  });

  it('shows a refused save as a plain sentence', async () => {
    const sentence = 'This question changed since you opened it.';
    const message = sentence;
    api.edit.mockImplementation(async (request: { dryRun: boolean }) => (request.dryRun
      ? ok(outcome())
      : { ok: false, error: message, details: { code: 'INVALID_STATE', message, details: null } }));
    show();
    fireEvent.change(await loaded(), { target: { value: 'My own wording.' } });
    await userEvent.click(saveButton());
    expect(await screen.findByRole('alert')).toHaveTextContent(sentence);
  });

  it('shows a refused load as a plain sentence', async () => {
    const sentence = 'That question is not in this lesson.';
    const message = sentence;
    api.get.mockResolvedValue({ ok: false, error: message, details: { code: 'INVALID_INPUT', message, details: null } });
    const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
    render(<QueryClientProvider client={client}>
      <LessonQuestionEditor programId="program-1" lessonId="lesson-1" questionIndex={1} open onClose={vi.fn()} />
    </QueryClientProvider>);
    expect(await screen.findByRole('alert')).toHaveTextContent(sentence);
  });

  it('asks before throwing changes away', async () => {
    const { onClose } = show();
    fireEvent.change(await loaded(), { target: { value: 'My own wording.' } });
    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(screen.getByText('Discard your changes?')).toBeVisible();
    expect(onClose).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole('button', { name: 'Discard' }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
