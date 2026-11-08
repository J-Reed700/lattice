import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@/tests/interactionDom';
import { MemoryRouter } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { LessonCard } from '@/features/learning/lessons/LessonCard';
import { review, unsupportedItem } from '@/features/learning/lessons/review/claimReviewTestData';
import type { LearningGenerationJob, LearningLessonDto, LearningProgramDto } from '@/lib/bindings';

// Every section of a ready lesson can be edited from the lesson page; a draft section waiting in the review cannot.
const api = vi.hoisted(() => ({ plan: vi.fn(), review: vi.fn(), index: vi.fn(), edit: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningPlan: api.plan, getLearningLessonClaimReview: api.review, getLearningLessonEvidence: async () => ({ ok: true, data: null }),
  courseIndexStatus: api.index, editLearningLessonSection: api.edit,
} }));
vi.mock('@/features/learning/tangents/StudyTangentSelection', () => ({
  StudyTangentSelection: ({ children }: { children: ReactNode }) => children,
  SectionTangentBar: () => null,
}));
vi.mock('@/features/reading/components/viewers/MarkdownViewer', () => ({ MarkdownViewer: ({ content }: { content: string }) => <div>{content}</div> }));

const blocks = [
  { kind: 'explanation', title: 'Boiling', body: 'Water boils when its vapour pressure meets the air pressure.', sourceIds: [] },
  { kind: 'recap', title: 'Key takeaways', body: '- Pressure sets the boiling point.', sourceIds: [] },
] as LearningLessonDto['blocks'];
const ready: LearningLessonDto = { id: 'lesson-1', title: 'Phase changes', objective: 'Explain boiling.', estimatedMinutes: 20, preparation: 'ready', blocks, completed: false };
const draft: LearningLessonDto = { ...ready, preparation: 'outline' };
const program = { summary: { id: 'program-1', revision: 44, status: 'active' }, sources: [], modules: [] } as unknown as LearningProgramDto;
const held: LearningGenerationJob = {
  id: 'job-1', programId: 'program-1', operationId: 'op', kind: 'lesson_preparation', payloadSha256: 'hash', baseRevisionNumber: 44,
  status: 'completed', progressCompleted: 1, progressTotal: 1, progressMessage: 'Needs your review: 1 claim', activity: null,
  resultId: 'needs_review:lesson-1', error: null, retryOfJobId: null, createdAt: 1, startedAt: 1, finishedAt: 2,
};

function show(lesson: LearningLessonDto, current: boolean) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(<QueryClientProvider client={client}><MemoryRouter>
    <LessonCard lesson={lesson} moduleId="module-1" program={program} isCurrent={current} selected={false} pending={false}
      onPrepare={vi.fn()} onComplete={vi.fn()} onSelect={vi.fn()} onPractice={vi.fn()} onReview={vi.fn()} />
  </MemoryRouter></QueryClientProvider>);
}

describe('Lesson card section editing', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    api.index.mockResolvedValue({ ok: true, data: { state: 'ready', mode: 'hybrid' } });
    api.plan.mockResolvedValue({ ok: true, data: { jobs: [] } });
    api.review.mockResolvedValue({ ok: true, data: null });
    api.edit.mockResolvedValue({ ok: true, data: { changed: false, statementsKept: 0, statementsChanged: 0, statementsRemoved: 0, sentencesWritten: 0, checkedStatementsAffected: 0, revision: null } });
  });

  it('puts an Edit button, named by its section, on every section of a ready lesson', () => {
    show(ready, true);
    expect(screen.getByRole('button', { name: 'Edit section 1: Boiling' })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Edit section 2: Key takeaways' })).toBeVisible();
  });

  it('opens the editor on that section with its title and text', async () => {
    show(ready, true);
    await userEvent.click(screen.getByRole('button', { name: 'Edit section 2: Key takeaways' }));
    expect(await screen.findByRole('dialog', { name: 'Edit section 2' })).toBeVisible();
    expect(screen.getByLabelText('Title')).toHaveValue('Key takeaways');
    expect(screen.getByLabelText('Text')).toHaveValue('- Pressure sets the boiling point.');
  });

  it('announces that the section was saved', async () => {
    api.edit.mockResolvedValue({ ok: true, data: { changed: true, statementsKept: 0, statementsChanged: 0, statementsRemoved: 0, sentencesWritten: 0, checkedStatementsAffected: 0, revision: null } });
    show(ready, true);
    await userEvent.click(screen.getByRole('button', { name: 'Edit section 1: Boiling' }));
    await userEvent.type(await screen.findByLabelText('Title'), ' again');
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(screen.getByRole('status', { name: 'Section editing' })).toHaveTextContent('Section saved.');
  });

  it('shows no Edit button on a recap held for recall first, and one once the recap is shown', async () => {
    const recallFirst: LearningLessonDto = { ...ready, blocks: [
      { kind: 'explanation', title: 'Boiling', body: 'Water boils when its vapour pressure meets the air pressure.', sourceIds: [] },
      { kind: 'reflection', title: 'From memory', body: 'Explain boiling in your own words.', sourceIds: [] },
      { kind: 'recap', title: 'Key takeaways', body: '- Pressure sets the boiling point.', sourceIds: [] },
    ] as LearningLessonDto['blocks'] };
    show(recallFirst, true);
    expect(screen.getByRole('button', { name: 'Edit section 1: Boiling' })).toBeVisible();
    expect(screen.queryByRole('button', { name: 'Edit section 3: Key takeaways' })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Show them now' }));
    expect(await screen.findByRole('button', { name: 'Edit section 3: Key takeaways' })).toBeVisible();
  });

  it('shows no Edit button on the draft sections of a lesson held for review', async () => {
    api.plan.mockResolvedValue({ ok: true, data: { jobs: [held] } });
    api.review.mockResolvedValue({ ok: true, data: review([unsupportedItem(1)]) });
    show(draft, false);
    await userEvent.click(screen.getByRole('button', { name: 'Phase changes' }));
    expect(await screen.findByLabelText('Draft sections')).toBeVisible();
    expect(screen.queryByRole('button', { name: /^Edit section/ })).not.toBeInTheDocument();
  });

  it('shows no Edit button on an outline lesson that is not prepared', () => {
    show({ ...draft, blocks: [] }, false);
    expect(screen.queryByRole('button', { name: /^Edit section/ })).not.toBeInTheDocument();
  });
});
