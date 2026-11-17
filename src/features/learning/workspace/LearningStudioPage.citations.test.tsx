import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@/tests/interactionDom';
import { createMemoryRouter, RouterProvider } from 'react-router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { LearningStudioPage } from '@/features/learning/workspace/LearningStudioPage';
import { useReaderStore } from '@/features/reading/stores/readerStore';
import type { LearningLessonDto, LearningModuleDto, LearningProgramDto } from '@/lib/bindings';
import { cites, said } from '@/tests/replyFixtures';

// Without layout the reader has no room beside the lesson, so it opens as the overlay it falls back to.
vi.mock('@/features/reading/components/FilePreviewModal', () => ({
  FilePreviewModal: ({ source }: { source: { fileName: string } | null }) => <div data-testid="reader-overlay">{source?.fileName}</div>,
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
const mocks = vi.hoisted(() => ({ list: vi.fn(), get: vi.fn(), evidence: vi.fn(), review: vi.fn(), plan: vi.fn(), version: vi.fn() }));
vi.mock('@/lib/api', async (importOriginal) => ({ ...(await importOriginal<typeof import('@/lib/api')>()), default: {
  listConversationSpaces: async () => ({ ok: true, data: [] }),
  listSpaceDocuments: async () => ({ ok: true, data: [] }),
  listLearningPrograms: mocks.list,
  getLearningProgram: mocks.get,
  getLearningOutlineEvidence: async () => ({ ok: true, data: null }),
  getLearningLessonEvidence: mocks.evidence,
  getLearningLessonClaimReview: mocks.review,
  getLearningSourceVersion: mocks.version,
  getLearningPlan: mocks.plan,
  prepareLearningAhead: () => new Promise(() => {}),
  getLearningPracticeWorkspace: async (programId: string) => ({ ok: true, data: { programId, sessions: [] } }),
  getLearningMemory: async () => ({ ok: true, data: { programId: 'program-1', journalId: null, lessonNotes: [], studyDeck: null, drafts: [], acceptedCards: [], dueCount: 0 } }),
  listStudyDecks: async () => ({ ok: true, data: [] }),
  courseIndexStatus: async () => ({ ok: true, data: { state: 'ready', mode: 'hybrid' } }),
  courseIndexOpen: async () => ({ ok: true, data: null }),
  courseIndexClose: async () => ({ ok: true, data: null }),
} }));

const PAGE_WAIT = { timeout: 5000 };
const SENTENCE = 'A source-backed explanation of boiling.';
const lesson = { id: 'lesson-1', title: 'Start with the idea', objective: 'Explain boiling', estimatedMinutes: 20, preparation: 'ready', completed: false,
  blocks: [{ kind: 'explanation', title: 'The idea', body: SENTENCE, sourceIds: ['source-1'] }] } as unknown as LearningLessonDto;
const modules: LearningModuleDto[] = [{ id: 'module-1', title: 'Foundations', summary: 'A first step.', outcomes: ['Describe it'], lessons: [lesson] }];
const course = {
  summary: { id: 'program-1', title: 'A thoughtful course', goal: 'Learn to reason about these materials.', status: 'active', revision: 1, moduleCount: 1, lessonCount: 1, completedLessons: 0, currentLessonId: 'lesson-1', createdAt: 1 },
  priorKnowledge: 'Some', minutesPerSession: 30, modelName: 'Model', modules, sources: [],
} as unknown as LearningProgramDto;

function show() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  const router = createMemoryRouter([{ path: '/studio', element: <LearningStudioPage /> }], { initialEntries: ['/studio'] });
  render(<QueryClientProvider client={client}><RouterProvider router={router} /></QueryClientProvider>);
}

describe('Learning Studio page: a lesson’s citations open the source reader', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    useReaderStore.setState({ session: null, resolvedLocations: new Map() });
    mocks.list.mockResolvedValue(ok([course.summary]));
    mocks.get.mockResolvedValue(ok(course));
    mocks.plan.mockResolvedValue(ok({ jobs: [] }));
    mocks.review.mockResolvedValue(ok(null));
    mocks.version.mockResolvedValue({ ok: false, error: 'The saved text is unavailable.' });
    mocks.evidence.mockResolvedValue(ok({
      policy: 'lesson-evidence-v22', checkedAt: 1, checkerModel: 'checker', retrievalMode: 'hybrid', embeddingModel: null, contentSha256: 'hash', claimCount: 0, confirmedCount: 0,
      executedExamples: 0, unexecutedLanguages: [], sourcesCurrent: true, teachingClaims: [],
      citations: [{ number: 1, sourceId: 'source-1', passage: { sourceVersionId: 'version-1', title: 'Field guide', url: null, text: 'A carefully chosen passage.', startByte: 0, endByte: 27, retrievalKind: 'hybrid', trust: null } }],
      sections: [{ sectionIndex: 0, pieces: [said(SENTENCE), cites(1)] }],
    }));
  });
  afterEach(() => useReaderStore.setState({ session: null, resolvedLocations: new Map() }));

  it('shows the source reader for a chip clicked on the course page, and closes it on leaving the course', async () => {
    show();
    await userEvent.click(await screen.findByRole('button', { name: /A thoughtful course/ }, PAGE_WAIT));
    await waitFor(() => expect(document.querySelector('[data-cite="1"]')).not.toBeNull(), PAGE_WAIT);
    expect(screen.queryByTestId('reader-overlay')).not.toBeInTheDocument();

    fireEvent.click(document.querySelector('[data-cite="1"]')!);
    expect(await screen.findByTestId('reader-overlay')).toHaveTextContent('Field guide');
    expect(useReaderStore.getState().session?.ownerKey).toBe('lesson:lesson-1:0');

    await userEvent.click(screen.getByRole('button', { name: /All programs/ }));
    await waitFor(() => expect(useReaderStore.getState().session).toBeNull());
    expect(screen.queryByTestId('reader-overlay')).not.toBeInTheDocument();
  }, 3 * PAGE_WAIT.timeout);

  it('closes the source reader when the lesson’s text leaves the screen', async () => {
    show();
    await userEvent.click(await screen.findByRole('button', { name: /A thoughtful course/ }, PAGE_WAIT));
    const openChip = async () => {
      await waitFor(() => expect(document.querySelector('[data-cite="1"]')).not.toBeNull(), PAGE_WAIT);
      fireEvent.click(document.querySelector('[data-cite="1"]')!);
      expect(await screen.findByTestId('reader-overlay')).toBeInTheDocument();
      expect(useReaderStore.getState().session).not.toBeNull();
    };

    await openChip();
    await userEvent.click(screen.getByRole('tab', { name: /Sources/ }));
    await waitFor(() => expect(useReaderStore.getState().session).toBeNull(), PAGE_WAIT);
    expect(screen.queryByTestId('reader-overlay')).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole('tab', { name: /Lessons/ }));
    await openChip();
    await userEvent.click(screen.getByRole('button', { name: 'Start with the idea', expanded: true }));
    await waitFor(() => expect(useReaderStore.getState().session).toBeNull(), PAGE_WAIT);
    expect(screen.queryByTestId('reader-overlay')).not.toBeInTheDocument();
  }, 3 * PAGE_WAIT.timeout);
});
