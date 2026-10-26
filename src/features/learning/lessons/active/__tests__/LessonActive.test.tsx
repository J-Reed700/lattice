import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { checkCardId } from '@/features/learning/lessons/active/checkReveal';
import { comesBack, LessonCheck } from '@/features/learning/lessons/active/LessonCheck';
import { LessonFromMemory } from '@/features/learning/lessons/active/LessonFromMemory';
import { LessonWarmUp, warmUpQueue } from '@/features/learning/lessons/active/LessonWarmUp';
import type { LearningRecallCardDto } from '@/lib/bindings';

const mocks = vi.hoisted(() => ({ recall: vi.fn(), save: vi.fn(), review: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningRecallWorkspace: mocks.recall,
  saveLearningRecallCard: mocks.save,
  reviewLearningRecallCard: mocks.review,
} }));
vi.mock('@/features/reading/components/viewers/MarkdownViewer', () => ({ MarkdownViewer: ({ content }: { content: string }) => <div>{content}</div> }));

const ok = <T,>(data: T) => ({ ok: true as const, data });
const fail = (error: string) => ({ ok: false as const, error });
const DAY = 86_400_000;
const VERSION = '0b5c3a3e-8f0e-4d7a-9c1b-2a6f4e8d1c90';
const parts = { question: 'A sealed jar is carried up a mountain. What does its lid do?', reveal: 'It bulges.\n\nOutside pressure falls; inside pressure stays. Caving in reverses which side lost pressure.' };
const body = `${parts.question}\n\n### Check your thinking\n\n${parts.reveal}`;

function card(id: string, overrides: Partial<LearningRecallCardDto> = {}): LearningRecallCardDto {
  return {
    id, format: 'question_answer',
    content: { prompt: `Prompt ${id}`, answer: `Answer ${id}`, explanation: '', options: [], correctOptionIndex: null, language: null, clozeDeletions: [] },
    sourceVersionIds: [], contentRevision: 1,
    scheduler: { stability: null, difficulty: null, lastReviewedAt: null, dueAt: Date.now() - DAY, intervalDays: 0, reviewCount: 0 },
    versions: [], createdAt: 0, updatedAt: 0, ...overrides,
  };
}
const workspace = (cards: LearningRecallCardDto[]) => ({ programId: 'program-1', cards, duplicates: [], dueCount: cards.length, schedulerDisclosure: '' });

function wrapper() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  return ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}

function renderCheck(active = true) {
  return render(<LessonCheck programId="program-1" lessonId="lesson-1" lessonTitle="Pressure" section={3} body={body} parts={parts} sourceIds={[VERSION, 'not-a-version']} active={active} questionView={<p>{parts.question}</p>} />, { wrapper: wrapper() });
}

describe('a lesson check', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    localStorage.clear();
    mocks.recall.mockResolvedValue(ok(workspace([])));
  });

  it('holds the reasoning back until the learner commits, then schedules the case by how close they came', async () => {
    const user = userEvent.setup();
    const id = await checkCardId('lesson-1', 3, body);
    const created = card(id);
    mocks.save.mockResolvedValue(ok(workspace([created])));
    mocks.review.mockResolvedValue(ok(workspace([card(id, { scheduler: { ...created.scheduler, dueAt: Date.now() + DAY, intervalDays: 1, reviewCount: 1 } })])));
    renderCheck();
    expect(screen.queryByText(/Outside pressure falls/)).toBeNull();
    const commit = screen.getByRole('button', { name: 'Check my thinking' });
    expect(commit).toBeDisabled();
    await user.type(screen.getByRole('textbox', { name: 'Your answer' }), 'It caves in');
    await user.click(commit);
    expect(screen.getByText(/Outside pressure falls/)).toBeInTheDocument();
    await user.click(await screen.findByRole('button', { name: 'Missed it' }));
    await waitFor(() => expect(mocks.review).toHaveBeenCalledTimes(1));
    expect(mocks.save.mock.calls[0][0]).toMatchObject({
      programId: 'program-1', cardId: id, expectedContentRevision: null, format: 'question_answer',
      content: { prompt: parts.question, answer: 'It bulges.', explanation: 'Outside pressure falls; inside pressure stays. Caving in reverses which side lost pressure.' },
      sourceVersionIds: [VERSION], changeReason: 'From the check in “Pressure”',
    });
    expect(mocks.review.mock.calls[0][0]).toMatchObject({ programId: 'program-1', cardId: id, expectedReviewCount: 0, rating: 'again', selectedOption: null });
    expect(await screen.findByRole('status')).toHaveTextContent('this case comes back tomorrow');
    expect(screen.queryByRole('button', { name: 'Missed it' })).toBeNull();
  });

  it('keeps the learner’s answer and the reveal when they come back', async () => {
    const user = userEvent.setup();
    const first = renderCheck();
    await user.type(screen.getByRole('textbox', { name: 'Your answer' }), 'It bulges');
    await user.click(screen.getByRole('button', { name: 'I don’t know yet: show me' }));
    first.unmount();
    renderCheck();
    expect(screen.getByRole('textbox', { name: 'Your answer' })).toHaveValue('It bulges');
    expect(screen.getByText(/Outside pressure falls/)).toBeInTheDocument();
  });

  it('retries a failed rating without creating the card twice', async () => {
    const user = userEvent.setup();
    const id = await checkCardId('lesson-1', 3, body);
    mocks.save.mockResolvedValue(ok(workspace([card(id)])));
    mocks.review.mockResolvedValueOnce(fail('Connection interrupted')).mockResolvedValue(ok(workspace([card(id, { scheduler: { ...card(id).scheduler, dueAt: Date.now() + 3 * DAY, reviewCount: 1 } })])));
    renderCheck();
    await user.click(screen.getByRole('button', { name: 'I don’t know yet: show me' }));
    await user.click(await screen.findByRole('button', { name: 'Got it' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Connection interrupted');
    await user.click(screen.getByRole('button', { name: 'Got it' }));
    await waitFor(() => expect(mocks.review).toHaveBeenCalledTimes(2));
    expect(mocks.save).toHaveBeenCalledTimes(1);
    expect(mocks.review.mock.calls[1][0].reviewId).toBe(mocks.review.mock.calls[0][0].reviewId);
    expect(await screen.findByRole('status')).toHaveTextContent('comes back in 3 days');
  });

  it('only reveals in a course that is not active: nothing is scheduled', async () => {
    const user = userEvent.setup();
    renderCheck(false);
    await user.click(screen.getByRole('button', { name: 'I don’t know yet: show me' }));
    expect(screen.getByText(/Outside pressure falls/)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Got it' })).toBeNull();
    expect(mocks.recall).not.toHaveBeenCalled();
  });

  it('says when a card comes back in words', () => {
    const now = 1_000 * DAY;
    expect(comesBack(now, now)).toBe('later today');
    expect(comesBack(now + DAY, now)).toBe('tomorrow');
    expect(comesBack(now + 5 * DAY, now)).toBe('in 5 days');
  });
});

describe('explaining a lesson from memory', () => {
  beforeEach(() => localStorage.clear());

  it('asks for an explanation before comparing, and allows showing the takeaways anyway', async () => {
    const user = userEvent.setup();
    const onCompare = vi.fn();
    const view = render(<LessonFromMemory lessonId="lesson-1" compared={false} onCompare={onCompare} />);
    const compare = screen.getByRole('button', { name: 'Compare with the key takeaways' });
    expect(compare).toBeDisabled();
    await user.type(screen.getByRole('textbox', { name: 'Explain it from memory' }), 'Pressure moves from high to low.');
    await user.click(compare);
    expect(onCompare).toHaveBeenCalledTimes(1);
    view.rerender(<LessonFromMemory lessonId="lesson-1" compared onCompare={onCompare} />);
    expect(screen.queryByRole('button', { name: 'Show the takeaways now' })).toBeNull();
    expect(screen.getByRole('textbox', { name: 'Explain it from memory' })).toHaveValue('Pressure moves from high to low.');
  });
});

describe('the warm-up before a lesson', () => {
  beforeEach(() => {
    vi.resetAllMocks();
  });

  it('queues at most three due cards, longest overdue first', () => {
    const now = 100 * DAY;
    const at = (id: string, dueAt: number) => card(id, { scheduler: { ...card(id).scheduler, dueAt } });
    expect(warmUpQueue([at('later', now + DAY), at('b', now - DAY), at('a', now - 3 * DAY), at('c', now), at('d', now - 2 * DAY)], now)).toEqual(['a', 'd', 'b']);
  });

  it('reviews each due card from memory and then gets out of the way', async () => {
    const user = userEvent.setup();
    const due = card('due-1');
    mocks.recall.mockResolvedValue(ok(workspace([due])));
    mocks.review.mockResolvedValue(ok(workspace([card('due-1', { scheduler: { ...due.scheduler, dueAt: Date.now() + 2 * DAY, reviewCount: 1 } })])));
    render(<LessonWarmUp programId="program-1" />, { wrapper: wrapper() });
    expect(await screen.findByRole('heading', { name: 'Warm up from memory' })).toBeInTheDocument();
    expect(screen.queryByText('Answer due-1')).toBeNull();
    await user.click(screen.getByRole('button', { name: 'Reveal the answer' }));
    expect(screen.getByText('Answer due-1')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Good' }));
    await waitFor(() => expect(mocks.review).toHaveBeenCalledTimes(1));
    expect(mocks.review.mock.calls[0][0]).toMatchObject({ programId: 'program-1', cardId: 'due-1', expectedReviewCount: 0, rating: 'good', selectedOption: null });
    expect(await screen.findByRole('status')).toHaveTextContent('Warmed up. Nothing else is due.');
  });

  it('shows nothing when no card is due', async () => {
    mocks.recall.mockResolvedValue(ok(workspace([card('later', { scheduler: { ...card('later').scheduler, dueAt: Date.now() + DAY } })])));
    const view = render(<LessonWarmUp programId="program-1" />, { wrapper: wrapper() });
    await waitFor(() => expect(mocks.recall).toHaveBeenCalled());
    expect(view.container).toBeEmptyDOMElement();
  });
});
