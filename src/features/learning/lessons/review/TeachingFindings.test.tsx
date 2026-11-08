import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TeachingFindings } from '@/features/learning/lessons/review/TeachingFindings';
import { teachingFinding } from '@/features/learning/lessons/review/claimReviewTestData';
import type { TeachingFindingDto } from '@/lib/bindings';

const api = vi.hoisted(() => ({ settle: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: { settleLearningTeachingFindings: api.settle } }));

const ok = <T,>(data: T) => ({ ok: true, data });

const note = (index: number, overrides: Partial<TeachingFindingDto> = {}) => teachingFinding({
  findingKey: `teaching:section-${index}`, unitIndex: index, unitTitle: `Section ${index + 1}: Part ${index + 1}`,
  summary: `Part ${index + 1} has a problem the repairs did not fix.`, passage: `Passage of part ${index + 1}.`, ...overrides,
});

function show(findings: TeachingFindingDto[]) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return render(<QueryClientProvider client={client}>
    <TeachingFindings programId="program-1" lessonId="lesson-1" findings={findings} />
  </QueryClientProvider>);
}

const box = (index: number) => screen.getByRole('checkbox', { name: `Select the note on Section ${index + 1}: Part ${index + 1}` });

describe('TeachingFindings', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    api.settle.mockResolvedValue(ok(null));
  });

  it('shows a card as its kind, its part, its summary, its whole passage and what the repairs did', () => {
    const passage = 'The function returns the sum, so the response reads {"result": "4"}. '.repeat(8).trim();
    show([note(0, { passage, repairs: 2 })]);
    const card = screen.getByRole('article', { name: 'Section 1: Part 1: Teaching review' });
    expect(within(card).getByText('Part 1 has a problem the repairs did not fix.')).toBeVisible();
    expect(card).toHaveTextContent(passage);
    expect(within(card).getByText('2 repairs tried and did not settle it.')).toBeVisible();
  });

  it('never shows a note written for the model that repairs the lesson', () => {
    const finding = { ...note(0), note: 'Fix blocks[2]: add an entry to the citations of blocks[2] with the sourceIndex.' } as TeachingFindingDto;
    show([finding]);
    expect(screen.queryByText(/blocks\[2\]/)).not.toBeInTheDocument();
    expect(screen.queryByText(/sourceIndex/)).not.toBeInTheDocument();
  });

  it('puts a checkbox on a waiting card and none on a kept one', () => {
    show([note(0), note(1, { state: 'kept' })]);
    expect(box(0)).not.toBeChecked();
    expect(screen.queryByRole('checkbox', { name: 'Select the note on Section 2: Part 2' })).not.toBeInTheDocument();
  });

  it('selects a range with shift and says how many notes are selected', () => {
    show([note(0), note(1), note(2), note(3)]);
    fireEvent.click(box(0));
    fireEvent.click(box(2), { shiftKey: true });
    expect([0, 1, 2, 3].map((index) => (box(index) as HTMLInputElement).checked)).toEqual([true, true, true, false]);
    expect(screen.getByRole('toolbar', { name: '3 teaching notes selected' })).toBeVisible();
  });

  it('offers Select all with two or more waiting, and Select none once all are selected', async () => {
    const { unmount } = show([note(0)]);
    expect(screen.queryByRole('button', { name: /^Select all/ })).not.toBeInTheDocument();
    unmount();
    show([note(0), note(1), note(2, { state: 'kept' })]);
    await userEvent.click(screen.getByRole('button', { name: 'Select all 2' }));
    expect(box(0)).toBeChecked();
    expect(box(1)).toBeChecked();
    expect(screen.queryByRole('button', { name: 'Select all 2' })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Select none' }));
    expect(box(0)).not.toBeChecked();
    expect(box(1)).not.toBeChecked();
  });

  it('keeps every selected note with one request', async () => {
    show([note(0), note(1), note(2), note(3, { state: 'kept' })]);
    await userEvent.click(screen.getByRole('button', { name: 'Select all 3' }));
    await userEvent.click(within(screen.getByRole('toolbar', { name: '3 teaching notes selected' })).getByRole('button', { name: 'Keep as written' }));
    await waitFor(() => expect(api.settle).toHaveBeenCalledTimes(1));
    expect(api.settle).toHaveBeenCalledWith({
      programId: 'program-1', lessonId: 'lesson-1', settle: 'keep',
      findingKeys: ['teaching:section-0', 'teaching:section-1', 'teaching:section-2'],
    });
    expect(screen.queryByRole('toolbar')).not.toBeInTheDocument();
  });

  it('says how many were kept and undoes exactly that batch', async () => {
    show([note(0), note(1), note(2), note(3)]);
    fireEvent.click(box(1));
    fireEvent.click(box(3), { shiftKey: true });
    await userEvent.click(within(screen.getByRole('toolbar')).getByRole('button', { name: 'Keep as written' }));
    const status = await screen.findByRole('status');
    expect(status).toHaveTextContent('Kept 3 teaching notes as written.');
    api.settle.mockClear();
    await userEvent.click(within(status).getByRole('button', { name: 'Undo' }));
    await waitFor(() => expect(api.settle).toHaveBeenCalledWith({
      programId: 'program-1', lessonId: 'lesson-1', settle: 'reopen',
      findingKeys: ['teaching:section-1', 'teaching:section-2', 'teaching:section-3'],
    }));
    expect(await screen.findByRole('status')).toHaveTextContent('3 teaching notes wait for you again.');
    expect(within(screen.getByRole('status')).queryByRole('button', { name: 'Undo' })).not.toBeInTheDocument();
  });

  it('keeps one note from its own card, and says nothing about a batch', async () => {
    show([note(0), note(1)]);
    const card = screen.getByRole('article', { name: 'Section 1: Part 1: Teaching review' });
    await userEvent.click(within(card).getByRole('button', { name: 'Keep as written' }));
    await waitFor(() => expect(api.settle).toHaveBeenCalledWith({
      programId: 'program-1', lessonId: 'lesson-1', settle: 'keep', findingKeys: ['teaching:section-0'],
    }));
    expect(screen.queryByText(/as written\.$/)).not.toBeInTheDocument();
  });

  it('shows a kept card with its chip and an Undo that reopens it', async () => {
    show([note(0, { state: 'kept' }), note(1)]);
    const card = screen.getByRole('article', { name: 'Section 1: Part 1: Teaching review' });
    expect(within(card).getByText('Kept as written')).toBeVisible();
    expect(within(card).queryByRole('button', { name: 'Keep as written' })).not.toBeInTheDocument();
    await userEvent.click(within(card).getByRole('button', { name: 'Undo' }));
    await waitFor(() => expect(api.settle).toHaveBeenCalledWith({
      programId: 'program-1', lessonId: 'lesson-1', settle: 'reopen', findingKeys: ['teaching:section-0'],
    }));
  });

  it('shows a refusal as an alert and keeps the selection', async () => {
    api.settle.mockResolvedValue({ ok: false, error: 'Some of these notes changed since you opened them. Reload the lesson.' });
    show([note(0), note(1)]);
    await userEvent.click(screen.getByRole('button', { name: 'Select all 2' }));
    await userEvent.click(within(screen.getByRole('toolbar')).getByRole('button', { name: 'Keep as written' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Some of these notes changed since you opened them. Reload the lesson.');
    expect(box(0)).toBeChecked();
    expect(screen.queryByText(/Kept 2 teaching notes/)).not.toBeInTheDocument();
  });
});
