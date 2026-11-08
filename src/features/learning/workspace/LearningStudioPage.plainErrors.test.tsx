import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@/tests/interactionDom';
import { createMemoryRouter, RouterProvider } from 'react-router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { LearningStudioPage } from '@/features/learning/workspace/LearningStudioPage';
import { queryClient as activityClient } from '@/lib/queryClient';
import { STUDY_ACTIVITY_KEY } from '@/lib/studyActivity';
import { makeAppSettings } from '@/tests/fixtures/appSettings';
import { ok, program } from '@/tests/learningStudioFixtures';
import { expectPlainError, findAlertWith, PLAIN_ERROR_CASES } from '@/tests/plainErrors';

/**
 * A failed outline and a failed lesson preparation reach the learner as a plain
 * sentence, with the backend's own text under Details. The IPC layer carries
 * the structured error beside the short message; the course hooks must not
 * drop it.
 */

vi.mock('@/features/learning/practice/practical/PracticalWorkbenchPanel', () => ({ PracticalWorkbenchPanel: () => <p>Practical workbench</p> }));
vi.mock('@/features/learning/curriculum/PlanPanel', () => ({ PlanPanel: () => <div>Plan view</div> }));
vi.mock('@/features/learning/canvas/CanvasPanel', () => ({ default: () => <div>Canvas view</div> }));
vi.mock('@/features/learning/portability/PortabilityPanel', () => ({ PortabilityPanel: () => null }));
vi.mock('@/features/learning/portability/ImportCourseFlow', () => ({ ImportCourseFlow: () => null }));
vi.mock('@/features/learning/drills/CourseDrillsPanel', () => ({ CourseDrillsPanel: () => null }));

const mocks = vi.hoisted(() => ({
  list: vi.fn(), get: vi.fn(), generate: vi.fn(), prepare: vi.fn(), plan: vi.fn(), memory: vi.fn(), settings: vi.fn(),
}));
vi.mock('@/lib/pendingSaves', async (importOriginal) => ({
  ...await importOriginal<typeof import('@/lib/pendingSaves')>(),
  flushPendingSaves: async () => true,
}));
vi.mock('@/lib/api', () => ({ default: {
  listConversationSpaces: async () => ({ ok: true, data: [] }),
  listSpaceDocuments: async () => ({ ok: true, data: [] }),
  listLearningPrograms: mocks.list,
  getLearningProgram: mocks.get,
  getLearningOutlineEvidence: async () => ({ ok: true, data: null }),
  getLearningLessonEvidence: async () => ({ ok: true, data: null }),
  getLearningLessonClaimReview: async () => ({ ok: true, data: null }),
  generateLearningProgram: mocks.generate,
  prepareLearningLesson: mocks.prepare,
  getLearningPlan: mocks.plan,
  getLearningPracticeWorkspace: async (programId: string) => ({ ok: true, data: { programId, sessions: [] } }),
  getLearningMemory: mocks.memory,
  listStudyDecks: async () => ({ ok: true, data: [] }),
  courseIndexOpen: async () => ({ ok: true, data: undefined }),
  courseIndexClose: async () => ({ ok: true, data: undefined }),
  // A Learning model is set, so the builder has no model notice of its own to confuse the alerts.
  getSettings: mocks.settings,
  getDownloadedModels: async () => ({ ok: true, data: [] }),
} }));

function show(path: string) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  const router = createMemoryRouter([{ path: '/studio', element: <LearningStudioPage /> }], { initialEntries: [path] });
  return render(<QueryClientProvider client={client}><RouterProvider router={router} /></QueryClientProvider>);
}

describe('Plain errors while a course is built or a lesson is prepared', () => {
  afterEach(() => activityClient.removeQueries({ queryKey: STUDY_ACTIVITY_KEY }));
  beforeEach(() => {
    vi.resetAllMocks();
    const base = makeAppSettings();
    mocks.settings.mockResolvedValue(ok({ ...base, llm: { ...base.llm, provider: 'anthropic', model: 'claude-test-chat', learningModel: { kind: 'cloud', provider: 'anthropic', model: 'claude-test-chat' } } }));
    mocks.list.mockResolvedValue(ok([]));
    mocks.plan.mockResolvedValue(ok({ programId: 'program-1', jobs: [] }));
    mocks.memory.mockResolvedValue(ok({ programId: 'program-1', journalId: null, lessonNotes: [], studyDeck: null, drafts: [], acceptedCards: [], dueCount: 0 }));
  });

  describe.each(PLAIN_ERROR_CASES)('when $name', (errorCase) => {
    it('a failed outline says so in a plain sentence, with the backend text under Details', async () => {
      mocks.generate.mockResolvedValue(errorCase.failure);
      show('/studio?new=1');
      await userEvent.type(await screen.findByLabelText('Your goal'), 'Learn statistics well enough to run an experiment');
      await userEvent.click(screen.getByRole('button', { name: 'Create outline' }));
      const alert = await findAlertWith(errorCase.headline);
      expect(within(alert).queryByText(/The outline could not be generated/)).not.toBeInTheDocument();
      await expectPlainError(alert, errorCase);
      expect(screen.getByLabelText('Your goal')).toHaveValue('Learn statistics well enough to run an experiment');
    });

    it('a failed lesson preparation says so in a plain sentence, with the backend text under Details', async () => {
      const active = program('active');
      mocks.list.mockResolvedValue(ok([active.summary]));
      mocks.get.mockResolvedValue(ok(active));
      mocks.prepare.mockResolvedValue(errorCase.failure);
      show('/studio');
      await userEvent.click(await screen.findByRole('button', { name: /A thoughtful course/ }, { timeout: 5000 }));
      const card = (await screen.findByRole('button', { name: /^Start with the idea/ })).closest('article');
      expect(card).not.toBeNull();
      await userEvent.click(within(card as HTMLElement).getByRole('button', { name: 'Prepare lesson' }));
      await waitFor(() => expect(mocks.prepare).toHaveBeenCalledOnce());
      const alert = await findAlertWith(errorCase.headline);
      expect(within(alert).queryByText(/Lesson preparation failed/)).not.toBeInTheDocument();
      await expectPlainError(alert, errorCase);
    });
  });

  it('shows no error for an outline that is written', async () => {
    const draft = program();
    mocks.generate.mockResolvedValue(ok(draft));
    mocks.get.mockResolvedValue(ok(draft));
    show('/studio?new=1');
    await userEvent.type(await screen.findByLabelText('Your goal'), 'Understand these ideas and put them to use');
    await userEvent.click(screen.getByRole('button', { name: 'Create outline' }));
    expect(await screen.findByRole('textbox', { name: 'Program title' })).toHaveValue('A thoughtful course');
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });
});
