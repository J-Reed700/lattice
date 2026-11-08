import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TeachingFindings } from '@/features/learning/lessons/review/TeachingFindings';
import { teachingFinding } from '@/features/learning/lessons/review/claimReviewTestData';
import type { TeachingFindingDto } from '@/lib/bindings';

const api = vi.hoisted(() => ({ settle: vi.fn(), resolveMany: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: { settleLearningTeachingFindings: api.settle, resolveLearningClaims: api.resolveMany } }));

const ok = <T,>(data: T) => ({ ok: true, data });
const REVISION = 7;

/** A waiting note about one sentence, with the live claims on it. */
const note = (index: number, overrides: Partial<TeachingFindingDto> = {}) => teachingFinding({
  findingKey: `quotation:section-${index}`, kind: 'quotation', passageRole: 'quotation', unitIndex: index, unitTitle: `Section ${index + 1}: Part ${index + 1}`,
  summary: `No source is cited for this sentence: “Sentence ${index + 1}.”`, passage: null, sentence: `Sentence ${index + 1}.`, claimIds: [`claim-${index}a`, `claim-${index}b`], ...overrides,
});

function show(findings: TeachingFindingDto[], props: { published?: boolean; onAddSource?: (claimId: string, findingKey: string) => void } = {}) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return render(<QueryClientProvider client={client}>
    <TeachingFindings programId="program-1" lessonId="lesson-1" findings={findings} published={props.published ?? true} revision={REVISION} onAddSource={props.onAddSource} />
  </QueryClientProvider>);
}

const card = (index: number) => screen.getByRole('article', { name: `Section ${index + 1}: Part ${index + 1}: Source quotation` });
const box = (index: number) => screen.getByRole('checkbox', { name: `Select the note on Section ${index + 1}: Part ${index + 1}` });
const remove = (claimIds: string[], revision = REVISION) => ({
  programId: 'program-1', lessonId: 'lesson-1', claimIds, action: { action: 'remove' }, expectedRevision: revision, operationId: expect.any(String),
});
const undo = (claimIds: string[], revision: number) => ({
  programId: 'program-1', lessonId: 'lesson-1', claimIds, action: { action: 'undo' }, expectedRevision: revision, operationId: expect.any(String),
});
const settled = (settle: string, findingKeys: string[]) => ({ programId: 'program-1', lessonId: 'lesson-1', settle, findingKeys });

describe('TeachingFindings, acting on a note about a sentence', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    api.settle.mockResolvedValue(ok(null));
    api.resolveMany.mockResolvedValue(ok({ revision: 9 }));
  });

  it('offers Remove this sentence and Add a source on a published lesson, only for a note with claim ids', () => {
    show([note(0), note(1, { claimIds: [] })]);
    expect(within(card(0)).getByRole('button', { name: 'Keep as written' })).toBeVisible();
    expect(within(card(0)).getByRole('button', { name: 'Remove this sentence' })).toBeVisible();
    expect(within(card(0)).getByRole('button', { name: 'Add a source' })).toBeVisible();
    expect(within(card(1)).getByRole('button', { name: 'Keep as written' })).toBeVisible();
    expect(within(card(1)).queryByRole('button', { name: 'Remove this sentence' })).not.toBeInTheDocument();
    expect(within(card(1)).queryByRole('button', { name: 'Add a source' })).not.toBeInTheDocument();
  });

  it('offers only Keep as written on a draft lesson, even for a note with claim ids', () => {
    show([note(0)], { published: false });
    expect(within(card(0)).getByRole('button', { name: 'Keep as written' })).toBeVisible();
    expect(within(card(0)).queryByRole('button', { name: 'Remove this sentence' })).not.toBeInTheDocument();
    expect(within(card(0)).queryByRole('button', { name: 'Add a source' })).not.toBeInTheDocument();
  });

  it('offers only Keep as written on a question note', () => {
    show([note(0, { kind: 'answer_key', passageRole: 'lesson', site: 'question', unitTitle: 'Question 1' })]);
    const question = screen.getByRole('article', { name: 'Question 1: Answer key' });
    expect(within(question).getByRole('button', { name: 'Keep as written' })).toBeVisible();
    expect(within(question).queryByRole('button', { name: 'Remove this sentence' })).not.toBeInTheDocument();
    expect(within(question).queryByRole('button', { name: 'Add a source' })).not.toBeInTheDocument();
  });

  it('removes the sentence with one call on all its claims and the review revision, then settles the note removed', async () => {
    show([note(0), note(1)]);
    await userEvent.click(within(card(0)).getByRole('button', { name: 'Remove this sentence' }));
    await waitFor(() => expect(api.settle).toHaveBeenCalledTimes(1));
    expect(api.resolveMany).toHaveBeenCalledTimes(1);
    expect(api.resolveMany).toHaveBeenCalledWith(remove(['claim-0a', 'claim-0b']));
    expect(api.settle).toHaveBeenCalledWith(settled('removed', ['quotation:section-0']));
    expect(api.resolveMany.mock.invocationCallOrder[0]).toBeLessThan(api.settle.mock.invocationCallOrder[0]);
  });

  it('says the sentence was removed, and Undo takes the claims back and then reopens the note', async () => {
    show([note(0)]);
    await userEvent.click(within(card(0)).getByRole('button', { name: 'Remove this sentence' }));
    const status = await screen.findByRole('status');
    expect(status).toHaveTextContent('Removed the sentence from the lesson.');
    api.resolveMany.mockClear();
    api.settle.mockClear();
    await userEvent.click(within(status).getByRole('button', { name: 'Undo' }));
    await waitFor(() => expect(api.settle).toHaveBeenCalledWith(settled('reopen', ['quotation:section-0'])));
    expect(api.resolveMany).toHaveBeenCalledWith(undo(['claim-0a', 'claim-0b'], 9));
    expect(api.resolveMany.mock.invocationCallOrder[0]).toBeLessThan(api.settle.mock.invocationCallOrder[0]);
    expect(await screen.findByRole('status')).toHaveTextContent('1 teaching note waits for you again.');
  });

  it('shows a refusal of the removal as the card alert and settles nothing', async () => {
    api.resolveMany.mockResolvedValue({ ok: false, error: 'This statement changed since you opened it; reload and try again.' });
    show([note(0), note(1)]);
    await userEvent.click(within(card(0)).getByRole('button', { name: 'Remove this sentence' }));
    expect(await within(card(0)).findByRole('alert')).toHaveTextContent('This statement changed since you opened it; reload and try again.');
    expect(within(card(1)).queryByRole('alert')).not.toBeInTheDocument();
    expect(api.settle).not.toHaveBeenCalled();
    expect(screen.queryByText('Removed the sentence from the lesson.')).not.toBeInTheDocument();
  });

  it('shows the backend refusal of an Undo as an alert and does not reopen the note', async () => {
    show([note(0)]);
    await userEvent.click(within(card(0)).getByRole('button', { name: 'Remove this sentence' }));
    const status = await screen.findByRole('status');
    api.settle.mockClear();
    api.resolveMany.mockResolvedValue({ ok: false, error: 'This removal cannot be undone yet.' });
    await userEvent.click(within(status).getByRole('button', { name: 'Undo' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('This removal cannot be undone yet.');
    expect(api.settle).not.toHaveBeenCalled();
  });

  it('hands the first claim to the Add a source flow and settles nothing itself', async () => {
    const onAddSource = vi.fn();
    show([note(0)], { onAddSource });
    await userEvent.click(within(card(0)).getByRole('button', { name: 'Add a source' }));
    expect(onAddSource).toHaveBeenCalledTimes(1);
    expect(onAddSource).toHaveBeenCalledWith('claim-0a', 'quotation:section-0');
    expect(api.settle).not.toHaveBeenCalled();
    expect(api.resolveMany).not.toHaveBeenCalled();
  });

  it('shows a removed note with its chip and an Undo, and no checkbox', async () => {
    show([note(0, { state: 'resolved', resolution: 'removed', resolvedAt: 5, claimIds: ['claim-0a', 'claim-0b'] }), note(1)]);
    expect(within(card(0)).getByText('Removed from the lesson')).toBeVisible();
    expect(within(card(0)).queryByRole('button', { name: 'Keep as written' })).not.toBeInTheDocument();
    expect(within(card(0)).queryByRole('button', { name: 'Remove this sentence' })).not.toBeInTheDocument();
    expect(screen.queryByRole('checkbox', { name: 'Select the note on Section 1: Part 1' })).not.toBeInTheDocument();
    await userEvent.click(within(card(0)).getByRole('button', { name: 'Undo' }));
    await waitFor(() => expect(api.settle).toHaveBeenCalledWith(settled('reopen', ['quotation:section-0'])));
    expect(api.resolveMany).toHaveBeenCalledWith(undo(['claim-0a', 'claim-0b'], REVISION));
    expect(api.resolveMany.mock.invocationCallOrder[0]).toBeLessThan(api.settle.mock.invocationCallOrder[0]);
  });

  it('shows a note that had a source added with its chip, and Undo only reopens it', async () => {
    show([note(0, { state: 'resolved', resolution: 'source_added', resolvedAt: 5, claimIds: [] })]);
    expect(within(card(0)).getByText('Source added')).toBeVisible();
    expect(screen.queryByRole('checkbox')).not.toBeInTheDocument();
    await userEvent.click(within(card(0)).getByRole('button', { name: 'Undo' }));
    await waitFor(() => expect(api.settle).toHaveBeenCalledWith(settled('reopen', ['quotation:section-0'])));
    expect(api.resolveMany).not.toHaveBeenCalled();
  });

  it('offers no Undo on a removed note whose claims are not known', () => {
    show([note(0, { state: 'resolved', resolution: 'removed', resolvedAt: 5, claimIds: [] })]);
    expect(within(card(0)).getByText('Removed from the lesson')).toBeVisible();
    expect(within(card(0)).queryByRole('button', { name: 'Undo' })).not.toBeInTheDocument();
  });

  it('does not say all notes were kept when one was removed', () => {
    show([note(0, { state: 'kept' }), note(1, { state: 'resolved', resolution: 'removed', resolvedAt: 5 })]);
    expect(screen.queryByText('All kept as written')).not.toBeInTheDocument();
    expect(screen.getByText('Nothing is waiting for you')).toBeVisible();
  });
});

describe('TeachingFindings, removing many sentences at once', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    api.settle.mockResolvedValue(ok(null));
    api.resolveMany.mockResolvedValue(ok({ revision: 9 }));
  });

  const selectAll = async () => userEvent.click(screen.getByRole('button', { name: /^Select all \d+$/ }));

  it('removes every selected note\'s claims with one call, settles those notes once, and says how many', async () => {
    show([note(0), note(1), note(2)]);
    fireEvent.click(box(0));
    fireEvent.click(box(2));
    const bar = screen.getByRole('toolbar', { name: '2 teaching notes selected' });
    await userEvent.click(within(bar).getByRole('button', { name: 'Remove these sentences' }));
    await waitFor(() => expect(api.settle).toHaveBeenCalledTimes(1));
    expect(api.resolveMany).toHaveBeenCalledTimes(1);
    expect(api.resolveMany).toHaveBeenCalledWith(remove(['claim-0a', 'claim-0b', 'claim-2a', 'claim-2b']));
    expect(api.settle).toHaveBeenCalledWith(settled('removed', ['quotation:section-0', 'quotation:section-2']));
    expect(await screen.findByRole('status')).toHaveTextContent('Removed 2 sentences from the lesson.');
    expect(screen.queryByRole('toolbar')).not.toBeInTheDocument();
  });

  it('names a claim shared by two notes once', async () => {
    show([note(0, { claimIds: ['shared', 'claim-0'] }), note(1, { claimIds: ['shared', 'claim-1'] })]);
    await selectAll();
    await userEvent.click(within(screen.getByRole('toolbar')).getByRole('button', { name: 'Remove these sentences' }));
    await waitFor(() => expect(api.resolveMany).toHaveBeenCalledWith(remove(['shared', 'claim-0', 'claim-1'])));
  });

  it('takes the whole batch back with one Undo', async () => {
    show([note(0), note(1)]);
    await selectAll();
    await userEvent.click(within(screen.getByRole('toolbar')).getByRole('button', { name: 'Remove these sentences' }));
    const status = await screen.findByRole('status');
    api.resolveMany.mockClear();
    api.settle.mockClear();
    await userEvent.click(within(status).getByRole('button', { name: 'Undo' }));
    await waitFor(() => expect(api.settle).toHaveBeenCalledWith(settled('reopen', ['quotation:section-0', 'quotation:section-1'])));
    expect(api.resolveMany).toHaveBeenCalledTimes(1);
    expect(api.resolveMany).toHaveBeenCalledWith(undo(['claim-0a', 'claim-0b', 'claim-1a', 'claim-1b'], 9));
    expect(await screen.findByRole('status')).toHaveTextContent('2 teaching notes wait for you again.');
  });

  it('is disabled with its hint while a selected note is not about one sentence, and the hint goes with the selection', async () => {
    show([note(0), note(1, { claimIds: [] })]);
    await selectAll();
    const bar = screen.getByRole('toolbar');
    expect(within(bar).getByRole('button', { name: 'Remove these sentences' })).toBeDisabled();
    expect(within(bar).getByRole('button', { name: 'Keep as written' })).toBeEnabled();
    expect(within(bar).getByText('Some of these notes are not about one sentence. Remove them one at a time.')).toBeVisible();
    fireEvent.click(box(1));
    expect(within(screen.getByRole('toolbar')).getByRole('button', { name: 'Remove these sentences' })).toBeEnabled();
    expect(screen.queryByText('Some of these notes are not about one sentence. Remove them one at a time.')).not.toBeInTheDocument();
  });

  it('has no Remove these sentences on a draft lesson', async () => {
    show([note(0), note(1)], { published: false });
    await selectAll();
    expect(within(screen.getByRole('toolbar')).queryByRole('button', { name: 'Remove these sentences' })).not.toBeInTheDocument();
    expect(within(screen.getByRole('toolbar')).getByRole('button', { name: 'Keep as written' })).toBeVisible();
  });

  it('shows a refusal of the batch as an alert, keeps the selection and settles nothing', async () => {
    api.resolveMany.mockResolvedValue({ ok: false, error: 'This statement changed since you opened it; reload and try again.' });
    show([note(0), note(1)]);
    await selectAll();
    await userEvent.click(within(screen.getByRole('toolbar')).getByRole('button', { name: 'Remove these sentences' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('This statement changed since you opened it; reload and try again.');
    expect(box(0)).toBeChecked();
    expect(api.settle).not.toHaveBeenCalled();
  });
});
