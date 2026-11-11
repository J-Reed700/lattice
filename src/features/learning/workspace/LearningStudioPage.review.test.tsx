import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@/tests/interactionDom';
import { createMemoryRouter, RouterProvider } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { claimItem, review } from '@/features/learning/lessons/review/claimReviewTestData';
import { LearningStudioPage } from '@/features/learning/workspace/LearningStudioPage';
import type { LearningLessonDto, LearningModuleDto, LearningProgramDto } from '@/lib/bindings';

// "Check the sources" in a published lesson's note opens the claim review on that claim
// (design-settle-published.md): the course page carries the claim in the address and into the review screen.
vi.mock('@/features/learning/lessons/review/ClaimReviewScreen', () => ({
  ClaimReviewScreen: ({ focusClaimId, onBack }: { focusClaimId?: string | null; onBack: () => void }) => <div>
    <p>Review on claim: {focusClaimId ?? 'none'}</p><button type="button" onClick={onBack}>Leave the review</button>
  </div>,
}));
vi.mock('@/features/learning/practice/practical/PracticalWorkbenchPanel', () => ({ PracticalWorkbenchPanel: () => null }));
vi.mock('@/features/learning/curriculum/PlanPanel', () => ({ PlanPanel: () => null }));
vi.mock('@/features/learning/canvas/CanvasPanel', () => ({ default: () => null }));
vi.mock('@/features/learning/tangents/StudyTangentSelection', async (importOriginal) => ({
  ...await importOriginal<typeof import('@/features/learning/tangents/StudyTangentSelection')>(),
  StudyTangentSelection: ({ children }: { children: React.ReactNode }) => children,
  SectionTangentBar: () => null,
}));

const ok = <T,>(data: T) => ({ ok: true as const, data });
const mocks = vi.hoisted(() => ({ list: vi.fn(), get: vi.fn(), evidence: vi.fn(), review: vi.fn(), plan: vi.fn() }));
vi.mock('@/lib/api', async (importOriginal) => ({ ...(await importOriginal<typeof import('@/lib/api')>()), default: {
  listConversationSpaces: async () => ({ ok: true, data: [] }),
  listSpaceDocuments: async () => ({ ok: true, data: [] }),
  listLearningPrograms: mocks.list,
  getLearningProgram: mocks.get,
  getLearningOutlineEvidence: async () => ({ ok: true, data: null }),
  getLearningLessonEvidence: mocks.evidence,
  getLearningLessonClaimReview: mocks.review,
  getLearningPlan: mocks.plan,
  getLearningPracticeWorkspace: async (programId: string) => ({ ok: true, data: { programId, sessions: [] } }),
  getLearningMemory: async () => ({ ok: true, data: { programId: 'program-1', journalId: null, lessonNotes: [], studyDeck: null, drafts: [], acceptedCards: [], dueCount: 0 } }),
  listStudyDecks: async () => ({ ok: true, data: [] }),
  courseIndexStatus: async () => ({ ok: true, data: { state: 'ready', mode: 'hybrid' } }),
  courseIndexOpen: async () => ({ ok: true, data: null }),
  courseIndexClose: async () => ({ ok: true, data: null }),
} }));

const PAGE_WAIT = { timeout: 5000 };
const STATEMENT = 'Water boils at 100 °C on Everest.';
const lesson = { id: 'lesson-1', title: 'Start with the idea', objective: 'Explain boiling', estimatedMinutes: 20, preparation: 'ready', completed: false,
  blocks: [{ kind: 'explanation', title: 'The idea', body: 'A source-backed explanation.', sourceIds: [] }] } as unknown as LearningLessonDto;
const modules: LearningModuleDto[] = [{ id: 'module-1', title: 'Foundations', summary: 'A first step.', outcomes: ['Describe it'], lessons: [lesson] }];
const course = {
  summary: { id: 'program-1', title: 'A thoughtful course', goal: 'Learn to reason about these materials.', status: 'active', revision: 1, moduleCount: 1, lessonCount: 1, completedLessons: 0, currentLessonId: 'lesson-1', createdAt: 1 },
  priorKnowledge: 'Some', minutesPerSession: 30, modelName: 'Model', modules, sources: [],
} as unknown as LearningProgramDto;

function show(path = '/studio') {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  const router = createMemoryRouter([{ path: '/studio', element: <LearningStudioPage /> }], { initialEntries: [path] });
  render(<QueryClientProvider client={client}><RouterProvider router={router} /></QueryClientProvider>);
  return router;
}

describe('Learning Studio page: opening the claim review on a claim', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.list.mockResolvedValue(ok([course.summary]));
    mocks.get.mockResolvedValue(ok(course));
    mocks.plan.mockResolvedValue(ok({ jobs: [] }));
    mocks.review.mockResolvedValue(ok(review([claimItem({ claimId: 'claim-1', state: { state: 'unverified', at: 2, reason: 'unsupported' }, verdictKind: null })], { state: 'ready' })));
    mocks.evidence.mockResolvedValue(ok({
      policy: 'lesson-evidence-v21', checkedAt: 1, checkerModel: 'checker', retrievalMode: 'hybrid', embeddingModel: null, contentSha256: 'hash', claimCount: 1, confirmedCount: 0,
      executedExamples: 0, unexecutedLanguages: [], sourcesCurrent: true,
      teachingClaims: [{ sectionIndex: 0, contentQuote: STATEMENT, claim: STATEMENT, verdict: 'unsupported', reason: 'No passage.', supportingQuote: null, passages: [], resolution: 'not_verified', note: null, sufficiency: null, unverifiedReason: 'unsupported', claimId: 'claim-1' }],
    }));
  });

  it('puts the claim in the address and hands it to the review screen from "Check the sources"', async () => {
    const router = show();
    await userEvent.click(await screen.findByRole('button', { name: /A thoughtful course/ }, PAGE_WAIT));
    await userEvent.click(await screen.findByRole('button', { name: 'Check the sources' }, PAGE_WAIT));
    expect(await screen.findByText('Review on claim: claim-1', undefined, PAGE_WAIT)).toBeVisible();
    const params = new URLSearchParams(router.state.location.search);
    expect([params.get('review'), params.get('course'), params.get('lesson'), params.get('claim')]).toEqual(['claims', 'program-1', 'lesson-1', 'claim-1']);
  });

  it('opens the review on the claim named in the address', async () => {
    show('/studio?course=program-1&lesson=lesson-1&review=claims&claim=claim-7');
    expect(await screen.findByText('Review on claim: claim-7', undefined, PAGE_WAIT)).toBeVisible();
  });

  it('opens the review on no particular claim when the address names none, and leaving drops the claim', async () => {
    const router = show('/studio?course=program-1&lesson=lesson-1&review=claims');
    expect(await screen.findByText('Review on claim: none', undefined, PAGE_WAIT)).toBeVisible();
    router.navigate('/studio?course=program-1&lesson=lesson-1&review=claims&claim=claim-7');
    expect(await screen.findByText('Review on claim: claim-7', undefined, PAGE_WAIT)).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: 'Leave the review' }));
    await vi.waitFor(() => expect(new URLSearchParams(router.state.location.search).get('claim')).toBeNull(), PAGE_WAIT);
  });
});
