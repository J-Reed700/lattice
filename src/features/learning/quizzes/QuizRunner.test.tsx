import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { QuizFormView } from '@/features/learning/quizzes/QuizFormView';
import { QuizRunner } from '@/features/learning/quizzes/QuizRunner';
import { formItem, quizForm, reveal } from '@/features/learning/quizzes/quizTestData';
import type { LearningAssessmentFormDto, LearningProgramDto } from '@/lib/bindings';

vi.mock('@/components/TiptapEditor', () => ({ TiptapViewer: ({ content }: { content: string }) => <div>{content}</div> }));

const api = vi.hoisted(() => ({ save: vi.fn(), submit: vi.fn(), interrupt: vi.fn(), form: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  saveLearningAssessmentResponse: api.save, submitLearningAssessmentForm: api.submit, interruptLearningAssessmentForm: api.interrupt, getLearningAssessmentForm: api.form,
} }));

const ok = <T,>(data: T) => ({ ok: true as const, data });
const items = [
  formItem('single_choice', { id: 'one', prompt: 'Which step is rate-limiting?', options: ['Binding', 'Release'] }),
  formItem('true_false', { id: 'two', prompt: 'Km measures affinity.', options: ['True', 'False'], previouslyExposed: true }),
];

function wrapper() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}

function run(form: LearningAssessmentFormDto, onExit = vi.fn(), onSubmitted = vi.fn()) {
  render(<QuizRunner form={form} sources={[]} onExit={onExit} onSubmitted={onSubmitted} reload={async () => form} />, { wrapper: wrapper() });
  return { user: userEvent.setup(), onExit, onSubmitted };
}

describe('quiz runner', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    let revision = 1;
    api.save.mockImplementation(async () => ok(quizForm({ items, revision: ++revision })));
  });

  it('moves along the question rail with arrow keys and says each question’s state in words', async () => {
    const { user } = run(quizForm({ items }));
    const rail = within(screen.getByRole('navigation', { name: 'Questions' }));
    const first = rail.getByRole('button', { name: 'Question 1, not answered yet' });
    expect(first).toHaveAttribute('aria-current', 'step');
    expect(rail.getByRole('button', { name: 'Question 2, not answered yet, seen before' })).toHaveAttribute('tabindex', '-1');
    first.focus();
    await user.keyboard('{ArrowRight}');
    expect(rail.getByRole('button', { name: /Question 2/ })).toHaveFocus();
    await user.keyboard('{Enter}');
    expect(screen.getByText('Km measures affinity.')).toBeVisible();
    expect(screen.getByText('Seen before · a repeat from an earlier attempt')).toBeVisible();
  });

  it('waits for every answer, then saves and submits through the alert dialog', async () => {
    const submitted = quizForm({ items, status: 'submitted' });
    api.submit.mockResolvedValue(ok(submitted));
    const { user, onSubmitted } = run(quizForm({ items }));
    const submit = screen.getByRole('button', { name: 'Submit answers' });
    expect(submit).toBeDisabled();
    expect(screen.getByText(/Answer all 2 questions before submitting/)).toBeVisible();
    await user.click(screen.getByRole('radio', { name: /Binding/ }));
    await user.click(screen.getByRole('button', { name: /Next/ }));
    await user.click(screen.getByRole('radio', { name: /False/ }));
    expect(screen.getByText('2 of 2 answered')).toBeVisible();
    await user.click(submit);
    const dialog = within(screen.getByRole('alertdialog'));
    expect(dialog.getByText(/Your responses will be frozen and graded/)).toBeVisible();
    await user.click(dialog.getByRole('button', { name: 'Submit now' }));
    await waitFor(() => expect(onSubmitted).toHaveBeenCalledWith(submitted));
    expect(api.save).toHaveBeenCalledWith(expect.objectContaining({ formId: 'form-1', response: expect.objectContaining({ itemId: 'one', selectedIndices: [0] }) }));
    expect(api.submit).toHaveBeenCalledWith(expect.objectContaining({ formId: 'form-1', expectedRevision: 3 }));
  });

  it('pauses by saving, then marking the attempt paused', async () => {
    api.interrupt.mockResolvedValue(ok(quizForm({ items, status: 'interrupted', revision: 3 })));
    const { user, onExit } = run(quizForm({ items }));
    await user.click(screen.getByRole('radio', { name: /Release/ }));
    await user.click(screen.getByRole('button', { name: 'Pause' }));
    await user.click(within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Pause' }));
    await waitFor(() => expect(onExit).toHaveBeenCalled());
    expect(api.save).toHaveBeenCalledTimes(1);
    expect(api.interrupt).toHaveBeenCalledWith(expect.objectContaining({ formId: 'form-1', expectedRevision: 2 }));
  });

  it('retries a failed pause as a pause, never as a submission', async () => {
    api.interrupt.mockResolvedValueOnce({ ok: false, error: 'Disk is busy.' })
      .mockResolvedValueOnce(ok(quizForm({ items, status: 'interrupted', revision: 3 })));
    const { user, onExit } = run(quizForm({ items }));
    await user.click(screen.getByRole('radio', { name: /Release/ }));
    await user.click(screen.getByRole('button', { name: 'Pause' }));
    await user.click(within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Pause' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Disk is busy.');
    await user.click(screen.getByRole('button', { name: /Try again/ }));
    await waitFor(() => expect(onExit).toHaveBeenCalled());
    expect(api.interrupt).toHaveBeenCalledTimes(2);
    expect(api.submit).not.toHaveBeenCalled();
  });

  it('says a paused attempt is paused, and resumes it with the next answer', async () => {
    api.submit.mockResolvedValue(ok(quizForm({ items, status: 'submitted' })));
    const { user } = run(quizForm({ items, status: 'interrupted' }));
    expect(screen.getByText(/Paused · answer any question to pick up where you left off/)).toBeVisible();
    await user.click(screen.getByRole('radio', { name: /Binding/ }));
    await user.click(screen.getByRole('button', { name: /Next/ }));
    await user.click(screen.getByRole('radio', { name: /False/ }));
    await user.click(screen.getByRole('button', { name: 'Submit answers' }));
    await user.click(within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Submit now' }));
    await waitFor(() => expect(api.save).toHaveBeenCalled());
  });

  it('keeps answers and offers a retry when a save fails', async () => {
    api.save.mockResolvedValueOnce({ ok: false, error: 'Disk is busy.' });
    const { user, onExit } = run(quizForm({ items }));
    await user.click(screen.getByRole('radio', { name: /Release/ }));
    await user.click(screen.getByRole('button', { name: /Quizzes/ }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Disk is busy.');
    expect(onExit).not.toHaveBeenCalled();
    expect(screen.getByRole('radio', { name: /Release/ })).toHaveAttribute('aria-checked', 'true');
    await user.click(screen.getByRole('button', { name: /Try again/ }));
    await waitFor(() => expect(api.save).toHaveBeenCalledTimes(2));
  });
});

describe('practice mode', () => {
  beforeEach(() => vi.resetAllMocks());

  it('reveals the key for one answer as soon as it is checked', async () => {
    const user = userEvent.setup();
    const practice = quizForm({ mode: 'practice', items: [items[0]] });
    const answered = quizForm({ mode: 'practice', revision: 2, items: [{ ...items[0], selectedIndices: [1], keyReveal: reveal({ correctIndices: [0], explanation: 'Binding limits the rate.' }) }] });
    api.form.mockResolvedValue(ok(practice));
    api.save.mockResolvedValue(ok(answered));
    const program = { summary: { id: 'program-1', revision: 1 }, sources: [], modules: [] } as unknown as LearningProgramDto;
    render(<QuizFormView formId="form-1" program={program} onBack={vi.fn()} onOpenForm={vi.fn()} />, { wrapper: wrapper() });
    expect(await screen.findByText('Practice round')).toBeVisible();
    await user.click(screen.getByRole('radio', { name: /Release/ }));
    await user.click(screen.getByRole('button', { name: 'Check my answer' }));
    const feedback = within(await screen.findByRole('region', { name: 'Feedback on this answer' }));
    expect(feedback.getByText('You chose B; the key is A.')).toBeVisible();
    expect(feedback.getByText('Binding limits the rate.')).toBeVisible();
  });
});
