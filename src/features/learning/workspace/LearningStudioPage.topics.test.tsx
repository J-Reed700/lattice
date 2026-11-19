import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { emptyPlan } from '@/features/learning/curriculum/coursePlan/coursePlanFixtures';
import { quizOverview } from '@/features/learning/quizzes/quizTestData';
import { LearningStudioPage } from '@/features/learning/workspace/LearningStudioPage';
import { makeAppSettings } from '@/tests/fixtures/appSettings';
import { ok, program } from '@/tests/learningStudioFixtures';

const mocks = vi.hoisted(() => ({ list: vi.fn(), get: vi.fn(), generate: vi.fn(), repair: vi.fn(), coursePlan: vi.fn(), planCourse: vi.fn() }));
vi.mock('@/lib/pendingSaves', async (importOriginal) => ({
  ...await importOriginal<typeof import('@/lib/pendingSaves')>(),
  flushPendingSaves: async () => true,
}));
vi.mock('@/lib/api', () => ({ default: {
  listConversationSpaces: async () => ({ ok: true, data: [] }),
  listSpaceDocuments: async () => ({ ok: true, data: [] }),
  listLearningPrograms: mocks.list,
  getLearningProgram: mocks.get,
  generateLearningProgram: mocks.generate,
  repairLearningOutline: mocks.repair,
  getLearningCoursePlan: mocks.coursePlan,
  planLearningCourse: mocks.planCourse,
  getLearningOutlineEvidence: async () => ({ ok: true, data: null }),
  getLearningLessonEvidence: async () => ({ ok: true, data: null }),
  getLearningLessonClaimReview: async () => ({ ok: true, data: null }),
  getLearningPlan: async () => ({ ok: true, data: { programId: 'program-1', jobs: [] } }),
  getLearningQuizOverview: async () => ({ ok: true, data: quizOverview() }),
  getLearningMemory: async () => ({ ok: true, data: { programId: 'program-1', journalId: null, lessonNotes: [], studyDeck: null, drafts: [], acceptedCards: [], dueCount: 0 } }),
  listStudyDecks: async () => ({ ok: true, data: [] }),
  courseIndexOpen: async () => ({ ok: true, data: undefined }),
  courseIndexClose: async () => ({ ok: true, data: undefined }),
  getSettings: async () => ({ ok: true, data: (() => { const base = makeAppSettings(); return { ...base, llm: { ...base.llm, learningModel: { kind: 'cloud', provider: 'anthropic', model: 'claude-test' } } }; })() }),
  getDownloadedModels: async () => ({ ok: true, data: [] }),
} }));

function show(path: string) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  const router = createMemoryRouter([{ path: '/studio', element: <LearningStudioPage /> }], { initialEntries: [path] });
  return render(<QueryClientProvider client={client}><RouterProvider router={router} /></QueryClientProvider>);
}

describe('planning topics after the outline', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    localStorage.clear();
    mocks.coursePlan.mockResolvedValue(ok(emptyPlan('planning')));
    mocks.planCourse.mockResolvedValue(ok(emptyPlan('planning')));
  });

  it('plans the topics of the course once its outline is generated', async () => {
    const user = userEvent.setup();
    const draft = program();
    mocks.list.mockResolvedValue(ok([]));
    mocks.get.mockResolvedValue(ok(draft));
    mocks.generate.mockResolvedValue(ok(draft));
    show('/studio?new=1');
    await user.type(await screen.findByLabelText('Your goal'), 'Understand these ideas and put them to use');
    await user.click(screen.getByRole('button', { name: 'Create outline' }));
    await waitFor(() => expect(mocks.generate).toHaveBeenCalled());
    await waitFor(() => expect(mocks.planCourse).toHaveBeenCalledWith({ programId: 'program-1' }));
  });

  it('does not plan when the outline fails', async () => {
    const user = userEvent.setup();
    mocks.list.mockResolvedValue(ok([]));
    mocks.generate.mockResolvedValue({ ok: false, error: 'The model stopped.' });
    show('/studio?new=1');
    await user.type(await screen.findByLabelText('Your goal'), 'Understand these ideas and put them to use');
    await user.click(screen.getByRole('button', { name: 'Create outline' }));
    await screen.findByRole('alert');
    expect(mocks.planCourse).not.toHaveBeenCalled();
  });

  it('plans the topics again once an outline repair succeeds', async () => {
    const user = userEvent.setup();
    const draft = program();
    draft.summary.revision = 4;
    draft.outlineReview = { status: 'needs_repair', repairPasses: 1, updatedAt: 0, contentHash: 'draft-hash', note: 'Draft saved.', citationsVerified: 0, citationsRemoved: 0, referencesGathered: 0, issues: [] } as unknown as typeof draft.outlineReview;
    const fixed = structuredClone(draft);
    fixed.summary.revision = 8;
    mocks.list.mockResolvedValue(ok([draft.summary]));
    mocks.get.mockResolvedValue(ok(draft));
    mocks.repair.mockResolvedValue(ok(fixed));
    show('/studio');
    await user.click(await screen.findByRole('button', { name: /A thoughtful course/ }));
    await user.click(await screen.findByRole('button', { name: 'Repair remaining issues' }));
    await waitFor(() => expect(mocks.repair).toHaveBeenCalled());
    await waitFor(() => expect(mocks.planCourse).toHaveBeenCalledWith({ programId: 'program-1' }));
  });
});
