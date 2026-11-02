import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { LessonCard } from '@/features/learning/lessons/LessonCard';
import { review, teachingFinding, unsupportedItem } from '@/features/learning/lessons/review/claimReviewTestData';
import type { LearningGenerationJob, LearningLessonDto, LearningProgramDto } from '@/lib/bindings';

const api = vi.hoisted(() => ({ plan: vi.fn(), review: vi.fn(), index: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningPlan: api.plan, getLearningLessonClaimReview: api.review, getLearningLessonEvidence: async () => ({ ok: true, data: null }),
  courseIndexStatus: api.index,
} }));
vi.mock('@/features/learning/tangents/StudyTangentSelection', () => ({
  StudyTangentSelection: ({ children }: { children: ReactNode }) => children,
  SectionTangentBar: () => null,
}));

const lesson: LearningLessonDto = { id: 'lesson-1', title: 'Phase changes', objective: 'Explain boiling.', estimatedMinutes: 20, preparation: 'outline', blocks: [], completed: false };
const program = { summary: { id: 'program-1', revision: 44, status: 'active' }, sources: [], modules: [] } as unknown as LearningProgramDto;
const held: LearningGenerationJob = {
  id: 'job-1', programId: 'program-1', operationId: 'op', kind: 'lesson_preparation', payloadSha256: 'hash', baseRevisionNumber: 44,
  status: 'completed', progressCompleted: 1, progressTotal: 1, progressMessage: 'Needs your review: 3 claims', activity: null,
  resultId: 'needs_review:lesson-1', error: null, retryOfJobId: null, createdAt: 1, startedAt: 1, finishedAt: 2,
};
const noReview = { ok: true, data: null };

function show(card: LearningLessonDto, onReview = vi.fn()) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={client}>
    <LessonCard lesson={card} moduleId="module-1" program={program} isCurrent={false} selected={false} pending={false}
      onPrepare={vi.fn()} onComplete={vi.fn()} onSelect={vi.fn()} onPractice={vi.fn()} onReview={onReview} />
  </QueryClientProvider>);
  return onReview;
}

describe('Lesson card review state', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    api.index.mockResolvedValue({ ok: true, data: { state: 'ready', mode: 'hybrid' } });
  });

  it('a held lesson is one line with a Review button that opens the review screen', async () => {
    api.plan.mockResolvedValue({ ok: true, data: { jobs: [held] } });
    api.review.mockResolvedValue({ ok: true, data: review([unsupportedItem(1), unsupportedItem(2), unsupportedItem(3)]) });
    const onReview = show(lesson);
    const row = screen.getByRole('article');
    expect(await within(row).findByText('3 claims need you')).toBeVisible();
    expect(within(row).queryByText(/failed|Outline/)).not.toBeInTheDocument();
    // The row says how many and leads away; no claim text is on the course page.
    expect(screen.queryByText('Statement number 1.')).not.toBeInTheDocument();
    await userEvent.click(within(row).getByRole('button', { name: 'Review' }));
    expect(onReview).toHaveBeenCalledWith('program-1', 'lesson-1');
    await userEvent.click(screen.getByRole('button', { name: /Phase changes/ }));
    expect(screen.queryByRole('region', { name: /claim review/i })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Prepare lesson' })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Review 3 claims' }));
    expect(onReview).toHaveBeenCalledTimes(2);
  });

  it('says a published lesson with teaching notes is ready, with a Review button', async () => {
    api.plan.mockResolvedValue({ ok: true, data: { jobs: [] } });
    api.review.mockResolvedValue({ ok: true, data: review([], { state: 'ready', teachingFindings: [teachingFinding(), teachingFinding({ findingKey: 'teaching:section-3' })] }) });
    const onReview = show({ ...lesson, preparation: 'ready' });
    expect(await screen.findByText('Ready · 2 teaching notes for you')).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: 'Review' }));
    expect(onReview).toHaveBeenCalledWith('program-1', 'lesson-1');
  });

  it('keeps an outline lesson with no review as it was', async () => {
    api.plan.mockResolvedValue({ ok: true, data: { jobs: [] } });
    api.review.mockResolvedValue(noReview);
    show(lesson);
    expect(await screen.findByText('Outline')).toBeVisible();
    expect(screen.queryByRole('button', { name: 'Review' })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: /Phase changes/ }));
    expect(await screen.findByRole('button', { name: 'Prepare lesson' })).toBeVisible();
    expect(screen.queryByRole('region', { name: /claim review/i })).not.toBeInTheDocument();
  });

  it.each([false, true])('keeps the notebook mounted until saving resolves (%s)', async (saved) => {
    api.plan.mockResolvedValue({ ok: true, data: { jobs: [] } });
    api.review.mockResolvedValue(noReview);
    let finishSave!: (saved: boolean) => void;
    const onSelect = vi.fn(() => new Promise<boolean>((resolve) => { finishSave = resolve; }));
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(<QueryClientProvider client={client}>
      <LessonCard lesson={{ ...lesson, preparation: 'ready' }} moduleId="module-1" program={program} isCurrent selected pending={false}
        onPrepare={vi.fn()} onComplete={vi.fn()} onSelect={onSelect} onPractice={vi.fn()}
        notebook={<textarea aria-label="Draft lesson notes" defaultValue="Keep this unsaved thought." />} />
    </QueryClientProvider>);
    const toggle = screen.getByRole('button', { name: lesson.title });
    await userEvent.click(toggle);
    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(screen.getByLabelText('Draft lesson notes')).toHaveValue('Keep this unsaved thought.');
    await act(async () => finishSave(saved));
    expect(toggle).toHaveAttribute('aria-expanded', String(!saved));
    if (saved) expect(screen.queryByLabelText('Draft lesson notes')).not.toBeInTheDocument();
    else expect(screen.getByLabelText('Draft lesson notes')).toHaveValue('Keep this unsaved thought.');
  });

  it.each([
    ['waits while the course indexes', { state: 'indexing', embedded: 4_210, total: 13_457, referencesNew: 4, passagesPerSecond: 14, waitingTurn: false }, false],
    ['is open when the course is ready', { state: 'ready', mode: 'hybrid' }, true],
    ['is open when references are searched by words only', { state: 'noEmbeddingModel' }, true],
  ])('Prepare lesson %s', async (_, status, open) => {
    api.plan.mockResolvedValue({ ok: true, data: { jobs: [] } });
    api.review.mockResolvedValue(noReview);
    api.index.mockResolvedValue({ ok: true, data: status });
    show(lesson);
    await userEvent.click(await screen.findByRole('button', { name: /Phase changes/ }));
    const prepare = await screen.findByRole('button', { name: 'Prepare lesson' });
    await vi.waitFor(() => expect(api.index).toHaveBeenCalledWith('program-1'));
    if (open) {
      await vi.waitFor(() => expect(prepare).toBeEnabled());
      expect(screen.queryByText(/Waits for the course index/)).not.toBeInTheDocument();
    } else {
      await vi.waitFor(() => expect(prepare).toBeDisabled());
      // The reason sits under the button as text, with the time left.
      expect(screen.getByText('Waits for the course index · about 11 min left')).toBeVisible();
      expect(prepare).toHaveAccessibleDescription('Waits for the course index · about 11 min left');
    }
  });

  it('offers Listen on every section of a prepared lesson', async () => {
    api.plan.mockResolvedValue({ ok: true, data: { jobs: [] } });
    api.review.mockResolvedValue(noReview);
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const blocks = [
      { kind: 'explanation', title: 'Boiling', body: 'Water boils when its vapour pressure meets the air’s.', sourceIds: [] },
      { kind: 'recap', title: 'Key takeaways', body: '- Pressure sets the boiling point.', sourceIds: [] },
    ] as LearningLessonDto['blocks'];
    render(<QueryClientProvider client={client}><MemoryRouter>
      <LessonCard lesson={{ ...lesson, preparation: 'ready', blocks }} moduleId="module-1" program={program} isCurrent selected={false} pending={false}
        onPrepare={vi.fn()} onComplete={vi.fn()} onSelect={vi.fn()} onPractice={vi.fn()} />
    </MemoryRouter></QueryClientProvider>);
    expect(await screen.findByRole('button', { name: 'Listen to the section “Boiling”' })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Listen to the section “Key takeaways”' })).toBeVisible();
  });
});
