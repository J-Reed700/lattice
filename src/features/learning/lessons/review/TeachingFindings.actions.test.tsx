import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { teachingFinding } from '@/features/learning/lessons/review/claimReviewTestData';
import { TeachingFindings } from '@/features/learning/lessons/review/TeachingFindings';
import type { TeachingFindingDto } from '@/lib/bindings';

const api = vi.hoisted(() => ({ settle: vi.fn(), resolveMany: vi.fn(), removeSentences: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  settleLearningTeachingFindings: api.settle, resolveLearningClaims: api.resolveMany, removeLearningTeachingSentences: api.removeSentences,
} }));

const ok = <T,>(data: T) => ({ ok: true, data });
const REVISION = 7;

/** A waiting note about one sentence the review says can be removed, with the live claims on it. */
const note = (index: number, overrides: Partial<TeachingFindingDto> = {}) => teachingFinding({
  findingKey: `quotation:section-${index}`, kind: 'quotation', passageRole: 'quotation', unitIndex: index, unitTitle: `Section ${index + 1}: Part ${index + 1}`,
  summary: `No source is cited for this sentence: “Sentence ${index + 1}.”`, passage: null, sentence: `Sentence ${index + 1}.`, claimIds: [`claim-${index}a`, `claim-${index}b`], removable: true, ...overrides,
});

function show(findings: TeachingFindingDto[], props: { published?: boolean; onAddSource?: (claimId: string, findingKey: string) => void } = {}) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return render(<QueryClientProvider client={client}>
    <TeachingFindings programId="program-1" lessonId="lesson-1" findings={findings} published={props.published ?? true} revision={REVISION} onAddSource={props.onAddSource} />
  </QueryClientProvider>);
}

const card = (index: number) => screen.getByRole('article', { name: `Section ${index + 1}: Part ${index + 1}: Source quotation` });
const box = (index: number) => screen.getByRole('checkbox', { name: `Select the note on Section ${index + 1}: Part ${index + 1}` });
const removal = (findingKeys: string[], revision = REVISION) => ({
  programId: 'program-1', lessonId: 'lesson-1', findingKeys, expectedRevision: revision,
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
    api.removeSentences.mockResolvedValue(ok({ revision: 9, claimIds: ['claim-0a', 'created-0'] }));
  });

  it('offers Remove this sentence on a published lesson for a removable note, and Add a source only for a note with claim ids', () => {
    show([note(0), note(1, { claimIds: [] }), note(2, { removable: false })]);
    expect(within(card(0)).getByRole('button', { name: 'Keep as written' })).toBeVisible();
    expect(within(card(0)).getByRole('button', { name: 'Remove this sentence' })).toBeVisible();
    expect(within(card(0)).getByRole('button', { name: 'Add a source' })).toBeVisible();
    expect(within(card(1)).getByRole('button', { name: 'Keep as written' })).toBeVisible();
    expect(within(card(1)).getByRole('button', { name: 'Remove this sentence' })).toBeVisible();
    expect(within(card(1)).queryByRole('button', { name: 'Add a source' })).not.toBeInTheDocument();
    expect(within(card(2)).getByRole('button', { name: 'Keep as written' })).toBeVisible();
    expect(within(card(2)).queryByRole('button', { name: 'Remove this sentence' })).not.toBeInTheDocument();
    expect(within(card(2)).getByRole('button', { name: 'Add a source' })).toBeVisible();
  });

  it('offers only Keep as written on a draft lesson, even for a note with claim ids', () => {
    show([note(0)], { published: false });
    expect(within(card(0)).getByRole('button', { name: 'Keep as written' })).toBeVisible();
    expect(within(card(0)).queryByRole('button', { name: 'Remove this sentence' })).not.toBeInTheDocument();
    expect(within(card(0)).queryByRole('button', { name: 'Add a source' })).not.toBeInTheDocument();
  });

  it('offers only Keep as written on a question note', () => {
    show([note(0, { kind: 'answer_key', passageRole: 'lesson', site: 'question', unitTitle: 'Question 1', removable: false })]);
    const question = screen.getByRole('article', { name: 'Question 1: Answer key' });
    expect(within(question).getByRole('button', { name: 'Keep as written' })).toBeVisible();
    expect(within(question).queryByRole('button', { name: 'Remove this sentence' })).not.toBeInTheDocument();
    expect(within(question).queryByRole('button', { name: 'Add a source' })).not.toBeInTheDocument();
  });

  it('removes the sentence of a note with no claims with one call on its key and the review revision, and settles nothing', async () => {
    show([note(0, { claimIds: [] }), note(1)]);
    await userEvent.click(within(card(0)).getByRole('button', { name: 'Remove this sentence' }));
    expect(await screen.findByRole('status')).toHaveTextContent('Removed the sentence from the lesson.');
    expect(api.removeSentences).toHaveBeenCalledTimes(1);
    expect(api.removeSentences).toHaveBeenCalledWith(removal(['quotation:section-0']));
    expect(api.resolveMany).not.toHaveBeenCalled();
    expect(api.settle).not.toHaveBeenCalled();
  });

  it('says the sentence was removed, and Undo is one undo call on the claims the removal returned and its revision, with no settle', async () => {
    show([note(0, { claimIds: [] })]);
    await userEvent.click(within(card(0)).getByRole('button', { name: 'Remove this sentence' }));
    const status = await screen.findByRole('status');
    expect(status).toHaveTextContent('Removed the sentence from the lesson.');
    await userEvent.click(within(status).getByRole('button', { name: 'Undo' }));
    await waitFor(() => expect(api.resolveMany).toHaveBeenCalledTimes(1));
    expect(api.resolveMany).toHaveBeenCalledWith(undo(['claim-0a', 'created-0'], 9));
    expect(await screen.findByText('1 teaching note waits for you again.')).toBeVisible();
    expect(api.settle).not.toHaveBeenCalled();
  });

  it('shows a refusal of the removal as the card alert and settles nothing', async () => {
    api.removeSentences.mockResolvedValue({ ok: false, error: 'This statement changed since you opened it; reload and try again.' });
    show([note(0), note(1)]);
    await userEvent.click(within(card(0)).getByRole('button', { name: 'Remove this sentence' }));
    expect(await within(card(0)).findByRole('alert')).toHaveTextContent('This statement changed since you opened it; reload and try again.');
    expect(within(card(1)).queryByRole('alert')).not.toBeInTheDocument();
    expect(api.settle).not.toHaveBeenCalled();
    expect(screen.queryByText('Removed the sentence from the lesson.')).not.toBeInTheDocument();
  });

  it('shows the backend refusal of an Undo as an alert and keeps the removal status', async () => {
    show([note(0, { claimIds: [] })]);
    await userEvent.click(within(card(0)).getByRole('button', { name: 'Remove this sentence' }));
    const status = await screen.findByRole('status');
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

  it('shows a removed note with its chip and an Undo, and no checkbox', () => {
    show([note(0, { state: 'resolved', resolution: 'removed', resolvedAt: 5, claimIds: ['claim-0a', 'claim-0b'] }), note(1)]);
    expect(within(card(0)).getByText('Removed from the lesson')).toBeVisible();
    expect(within(card(0)).queryByRole('button', { name: 'Keep as written' })).not.toBeInTheDocument();
    expect(within(card(0)).queryByRole('button', { name: 'Remove this sentence' })).not.toBeInTheDocument();
    expect(within(card(0)).getByRole('button', { name: 'Undo' })).toBeVisible();
    expect(screen.queryByRole('checkbox', { name: 'Select the note on Section 1: Part 1' })).not.toBeInTheDocument();
  });

  it('shows a removed note that has no claims to undo with no Undo and says the section is where to bring it back', () => {
    show([note(0, { state: 'resolved', resolution: 'removed', resolvedAt: 5, claimIds: [] }), note(1)]);
    expect(within(card(0)).getByText('Removed from the lesson')).toBeVisible();
    expect(within(card(0)).getByText('Edit the section to bring it back.')).toBeVisible();
    expect(within(card(0)).queryByRole('button', { name: 'Undo' })).not.toBeInTheDocument();
  });

  it('takes a removed note back with one undo call on its claims and the review revision, and settles nothing', async () => {
    show([note(0, { state: 'resolved', resolution: 'removed', resolvedAt: 5, claimIds: ['claim-0a', 'claim-0b'] })]);
    await userEvent.click(within(card(0)).getByRole('button', { name: 'Undo' }));
    await waitFor(() => expect(api.resolveMany).toHaveBeenCalledTimes(1));
    expect(api.resolveMany).toHaveBeenCalledWith(undo(['claim-0a', 'claim-0b'], REVISION));
    expect(api.settle).not.toHaveBeenCalled();
  });

  it('shows the refusal of a removed note\'s Undo as that card\'s alert', async () => {
    api.resolveMany.mockResolvedValue({ ok: false, error: 'This removal cannot be undone yet.' });
    show([note(0, { state: 'resolved', resolution: 'removed', resolvedAt: 5, claimIds: ['claim-0a'] }), note(1)]);
    await userEvent.click(within(card(0)).getByRole('button', { name: 'Undo' }));
    expect(await within(card(0)).findByRole('alert')).toHaveTextContent('This removal cannot be undone yet.');
    expect(within(card(1)).queryByRole('alert')).not.toBeInTheDocument();
    expect(api.settle).not.toHaveBeenCalled();
  });

  it('shows a note that had a source added with its chip, and Undo only reopens it', async () => {
    show([note(0, { state: 'resolved', resolution: 'source_added', resolvedAt: 5, claimIds: [] })]);
    expect(within(card(0)).getByText('Source added')).toBeVisible();
    expect(screen.queryByRole('checkbox')).not.toBeInTheDocument();
    await userEvent.click(within(card(0)).getByRole('button', { name: 'Undo' }));
    await waitFor(() => expect(api.settle).toHaveBeenCalledWith(settled('reopen', ['quotation:section-0'])));
    expect(api.resolveMany).not.toHaveBeenCalled();
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
    api.removeSentences.mockResolvedValue(ok({ revision: 9, claimIds: ['claim-0a', 'created-0'] }));
  });

  const selectAll = async () => userEvent.click(screen.getByRole('button', { name: /^Select all \d+$/ }));

  it('removes every selected note\'s sentence with one call on their keys, settles nothing, and counts the sentences', async () => {
    show([note(0), note(1, { claimIds: [] }), note(2)]);
    fireEvent.click(box(0));
    fireEvent.click(box(2));
    const bar = screen.getByRole('toolbar', { name: '2 teaching notes selected' });
    await userEvent.click(within(bar).getByRole('button', { name: 'Remove these sentences' }));
    expect(await screen.findByRole('status')).toHaveTextContent('Removed 2 sentences from the lesson.');
    expect(api.removeSentences).toHaveBeenCalledTimes(1);
    expect(api.removeSentences).toHaveBeenCalledWith(removal(['quotation:section-0', 'quotation:section-2']));
    expect(api.resolveMany).not.toHaveBeenCalled();
    expect(api.settle).not.toHaveBeenCalled();
    expect(screen.queryByRole('toolbar')).not.toBeInTheDocument();
  });

  it('says one sentence when two selected notes are about the same sentence', async () => {
    show([note(0), note(0, { findingKey: 'teaching:section-0', kind: 'teaching', passageRole: 'lesson' })]);
    await selectAll();
    await userEvent.click(within(screen.getByRole('toolbar')).getByRole('button', { name: 'Remove these sentences' }));
    expect(await screen.findByRole('status')).toHaveTextContent('Removed the sentence from the lesson.');
    expect(api.removeSentences).toHaveBeenCalledWith(removal(['quotation:section-0', 'teaching:section-0']));
  });

  it('counts three notes on two sentences as two sentences', async () => {
    show([note(0), note(0, { findingKey: 'teaching:section-0', kind: 'teaching', passageRole: 'lesson' }), note(1)]);
    await selectAll();
    await userEvent.click(within(screen.getByRole('toolbar')).getByRole('button', { name: 'Remove these sentences' }));
    expect(await screen.findByRole('status')).toHaveTextContent('Removed 2 sentences from the lesson.');
  });

  it('takes the whole batch back with one undo call on the claims the removal returned, in the order it returned them', async () => {
    api.removeSentences.mockResolvedValue(ok({ revision: 9, claimIds: ['claim-0a', 'created-1', 'claim-0b'] }));
    show([note(0), note(1, { claimIds: [] })]);
    await selectAll();
    await userEvent.click(within(screen.getByRole('toolbar')).getByRole('button', { name: 'Remove these sentences' }));
    const status = await screen.findByRole('status');
    api.resolveMany.mockClear();
    await userEvent.click(within(status).getByRole('button', { name: 'Undo' }));
    await waitFor(() => expect(api.resolveMany).toHaveBeenCalledTimes(1));
    expect(api.resolveMany).toHaveBeenCalledWith(undo(['claim-0a', 'created-1', 'claim-0b'], 9));
    expect(await screen.findByText('2 teaching notes wait for you again.')).toBeVisible();
    expect(api.settle).not.toHaveBeenCalled();
  });

  it('is disabled with its hint while a selected note is not about one sentence, and the hint goes with the selection', async () => {
    const hint = 'Some of these notes are not about one sentence, so they can\'t be removed. Unselect them to remove the rest.';
    show([note(0), note(1, { removable: false })]);
    await selectAll();
    const bar = screen.getByRole('toolbar');
    expect(within(bar).getByRole('button', { name: 'Remove these sentences' })).toBeDisabled();
    expect(within(bar).getByRole('button', { name: 'Keep as written' })).toBeEnabled();
    expect(within(bar).getByText(hint)).toBeVisible();
    fireEvent.click(box(1));
    expect(within(screen.getByRole('toolbar')).getByRole('button', { name: 'Remove these sentences' })).toBeEnabled();
    expect(screen.queryByText(hint)).not.toBeInTheDocument();
  });

  it('enables Remove these sentences for one removable note that holds no claim', async () => {
    show([note(0, { claimIds: [] }), note(1)]);
    fireEvent.click(box(0));
    expect(within(screen.getByRole('toolbar')).getByRole('button', { name: 'Remove these sentences' })).toBeEnabled();
  });

  it('says the one chosen note is not about one whole sentence when it cannot be removed', async () => {
    const hint = 'This note isn\'t about one whole sentence of a section, so there\'s no sentence to remove. Edit the section, or keep it as written.';
    show([note(0), note(1, { removable: false })]);
    fireEvent.click(box(1));
    const bar = screen.getByRole('toolbar', { name: '1 teaching note selected' });
    expect(within(bar).getByRole('button', { name: 'Remove these sentences' })).toBeDisabled();
    expect(within(bar).getByText(hint)).toBeVisible();
    expect(screen.queryByText('Some of these notes are not about one sentence, so they can\'t be removed. Unselect them to remove the rest.')).not.toBeInTheDocument();
  });

  it('has no Remove these sentences on a draft lesson', async () => {
    show([note(0), note(1)], { published: false });
    await selectAll();
    expect(within(screen.getByRole('toolbar')).queryByRole('button', { name: 'Remove these sentences' })).not.toBeInTheDocument();
    expect(within(screen.getByRole('toolbar')).getByRole('button', { name: 'Keep as written' })).toBeVisible();
  });

  it('shows a refusal of the batch as an alert, keeps the selection and settles nothing', async () => {
    api.removeSentences.mockResolvedValue({ ok: false, error: 'This statement changed since you opened it; reload and try again.' });
    show([note(0), note(1)]);
    await selectAll();
    await userEvent.click(within(screen.getByRole('toolbar')).getByRole('button', { name: 'Remove these sentences' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('This statement changed since you opened it; reload and try again.');
    expect(box(0)).toBeChecked();
    expect(api.settle).not.toHaveBeenCalled();
  });
});
