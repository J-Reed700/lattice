import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { LearningLessonDto, LearningModuleDto, LearningProgramDto, LearningProgramSummaryDto, LearningOutlineProgressDto } from '@/lib/bindings';
import type { DocumentMetadata } from '@/types/fileBrowser';

import { LearningStudioPage } from './LearningStudioPage';

vi.mock('./PracticalWorkbenchPanel', () => ({ PracticalWorkbenchPanel: () => <label>Lab scratchpad<input aria-label="Lab scratchpad" /></label> }));
vi.mock('./PlanPanel', () => ({ PlanPanel: () => <div data-testid="plan-panel">Plan view</div> }));
vi.mock('./CanvasPanel', () => ({ default: () => <div data-testid="canvas-panel">Canvas view</div> }));

const mocks = vi.hoisted(() => ({
  list: vi.fn(), get: vi.fn(), generate: vi.fn(), cancel: vi.fn(), accept: vi.fn(), prepare: vi.fn(), complete: vi.fn(), submit: vi.fn(),
  documents: vi.fn(), plan: vi.fn(), memory: vi.fn(), flush: vi.fn(), decks: vi.fn(), deck: vi.fn(),
}));
vi.mock('@/lib/pendingSaves', async (importOriginal) => ({
  ...await importOriginal<typeof import('@/lib/pendingSaves')>(),
  flushPendingSaves: mocks.flush,
}));
vi.mock('@/lib/api', () => ({ default: {
  listConversationSpaces: async () => ({ ok: true, data: [] }),
  listSpaceDocuments: async () => ({ ok: true, data: mocks.documents().map((doc: { id: string; fileName: string }) => ({ documentId: doc.id, fileName: doc.fileName, category: null, modifiedAt: null })) }),
  listLearningPrograms: mocks.list,
  getLearningProgram: mocks.get,
  generateLearningProgram: mocks.generate,
  cancelLearningOutline: mocks.cancel,
  acceptLearningProgram: mocks.accept,
  prepareLearningLesson: mocks.prepare,
  completeLearningLesson: mocks.complete,
  submitLearningAttempt: mocks.submit,
  getLearningPlan: mocks.plan,
  getLearningPracticeWorkspace: async (programId: string) => ({ ok: true, data: { programId, sessions: [] } }),
  getLearningAssessmentWorkspace: async (programId: string) => ({ ok: true, data: { programId, outcomes: [], forms: [], blueprints: [], evidence: [], followUps: [] } }),
  getLearningMemory: mocks.memory,
  // Flashcards share the landing page; an unconfigured mock means "no decks".
  listStudyDecks: (...args: unknown[]) => mocks.decks(...args) ?? Promise.resolve({ ok: true, data: [] }),
  getStudyDeck: mocks.deck,
} }));


const source = { id: 'source-1', title: 'Field guide', url: 'https://example.org/guide', excerpt: 'A carefully chosen passage.', acquiredAt: 1_790_000_000_000 };
const quizQuestion = { id: 'quiz-1', kind: 'quiz' as const, prompt: 'Which principle applies?', options: ['First principle', 'Second principle'], sourceIds: ['source-1'] };
const practiceQuestion = { id: 'practice-1', kind: 'practice' as const, prompt: 'Apply the concept to this case.', options: ['Apply it', 'Ignore it'], sourceIds: ['source-1'] };
const testQuestion = { id: 'test-1', kind: 'test' as const, prompt: 'Which choice follows the model?', options: ['A', 'B'], sourceIds: ['source-1'] };

function lesson(id: string, title: string, ready = false): LearningLessonDto {
  return {
    id, title, objective: ready ? `Explain ${title.toLowerCase()}` : '', estimatedMinutes: 20,
    preparation: ready ? 'ready' : 'outline', completed: false,
    blocks: ready ? [
      { kind: 'explanation', title: 'The idea', body: 'A source-backed explanation.', sourceIds: ['source-1'] },
      { kind: 'worked_example', title: 'Worked through', body: 'A short worked example.', sourceIds: ['source-1'] },
    ] : [],
    questions: ready ? [quizQuestion, practiceQuestion, testQuestion] : [],
  };
}

function module(id: string, title: string, lessons: LearningLessonDto[]): LearningModuleDto {
  return { id, title, summary: `${title} gives the course a useful next step.`, outcomes: [`Describe ${title}`, 'Apply the core idea'], lessons };
}

function program(status: 'draft' | 'active' = 'draft', firstReady = false): LearningProgramDto {
  const modules = [
    module('module-1', 'Foundations', [lesson('lesson-1', 'Start with the idea', firstReady), lesson('lesson-2', 'Read a worked example')]),
    module('module-2', 'Putting it together', [lesson('lesson-3', 'Compare approaches'), lesson('lesson-4', 'Make a decision')]),
  ];
  const summary: LearningProgramSummaryDto = {
    id: 'program-1', title: 'A thoughtful course', goal: 'Learn to reason about these materials and use them well.',
    status, revision: status === 'active' ? 1 : 0, moduleCount: 2, lessonCount: 4, completedLessons: 0,
    currentLessonId: 'lesson-1', createdAt: 1_790_000_000_000,
  };
  return { summary, priorKnowledge: 'Some experience', minutesPerSession: 30, modelName: 'Selected model', modules, sources: [source], attempts: [] };
}

function summary(value: LearningProgramDto): LearningProgramSummaryDto { return value.summary; }
function ok<T>(data: T) { return { ok: true as const, data }; }
function fail(error: string) { return { ok: false as const, error }; }

function show(path = '/studio') {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  const router = createMemoryRouter([{ path: '/studio', element: <LearningStudioPage /> }], { initialEntries: [path] });
  return render(<QueryClientProvider client={client}><RouterProvider router={router} /></QueryClientProvider>);
}

describe('Learning Studio program workflow', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.documents.mockReturnValue([{ id: 'document-1', fileName: 'Field guide.pdf', filePath: '/Field guide.pdf', fileType: 'pdf', category: 'document', language: 'en', modifiedAt: '', indexedAt: '', wordCount: 12 } satisfies DocumentMetadata]);
    mocks.memory.mockResolvedValue(ok({ programId: 'program-1', journalId: null, lessonNotes: [], studyDeck: null, drafts: [], acceptedCards: [], dueCount: 0, schedulerVersion: 'expanding_v1' }));
    mocks.flush.mockResolvedValue(true);
  });

  it('shows model progress, cancels pending generation, and retains inputs for a fresh retry', async () => {
    const user = userEvent.setup();
    mocks.list.mockResolvedValue(ok([]));
    let finish!: (value: ReturnType<typeof fail>) => void;
    let update!: (value: LearningOutlineProgressDto) => void;
    mocks.generate.mockImplementation((_request, options) => {
      update = options.onProgress;
      return new Promise((resolve) => { finish = resolve; });
    });
    mocks.cancel.mockImplementation(async () => { finish(fail('Course generation cancelled. Your inputs are kept.')); return ok(true); });
    show('/studio?new=1');
    await user.type(await screen.findByLabelText('Your goal'), 'Learn Rust ownership');
    await user.click(screen.getByRole('radio', { name: /Deep dive/ }));
    await user.click(screen.getByRole('button', { name: 'Create outline' }));
    const first = mocks.generate.mock.calls[0][1].requestId;
    await act(async () => update({ stage: 'reviewing', elapsedSeconds: 180, stageSeconds: 20, responseCharacters: 500, modelName: 'Test model' }));
    expect(screen.getByRole('status')).toHaveTextContent('Reviewing the teaching plan');
    expect(screen.getByText(/500 characters received/)).toBeVisible();
    expect(screen.getByRole('button', { name: 'Cancel generation' })).toBeEnabled();
    await user.click(screen.getByRole('button', { name: 'Cancel generation' }));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Create outline' })).toBeEnabled());
    expect(mocks.cancel).toHaveBeenCalledWith(first);
    expect(screen.getByLabelText('Your goal')).toHaveValue('Learn Rust ownership');
    expect(screen.getByRole('radio', { name: /Deep dive/ })).toBeChecked();
    const oldUpdate = update;
    await user.click(screen.getByRole('button', { name: 'Create outline' }));
    expect(mocks.generate.mock.calls[1][1].requestId).not.toBe(first);
    await act(async () => oldUpdate({ stage: 'repairing', elapsedSeconds: 200, stageSeconds: 0, responseCharacters: 0, modelName: 'Old model' }));
    expect(screen.getByRole('status')).toHaveTextContent('Starting your course');
    await act(async () => finish(fail('Course generation timed out while writing the outline. Your inputs are kept.')));
    expect(await screen.findByRole('alert')).toHaveTextContent('timed out while writing the outline');
    expect(screen.getByLabelText('Your goal')).toHaveValue('Learn Rust ownership');
  });

  it('opens the builder from the new route, generates a source-backed draft, accepts it, and prepares a lesson after browsing modules', async () => {
    const user = userEvent.setup();
    const draft = program();
    const active = program('active');
    const prepared = structuredClone(active);
    prepared.summary.revision = 2;
    prepared.modules[1].lessons[0] = lesson('lesson-3', 'Compare approaches', true);
    mocks.list.mockResolvedValue(ok([]));
    mocks.generate.mockResolvedValue(ok(draft));
    mocks.accept.mockResolvedValue(ok(active));
    mocks.prepare.mockResolvedValue(ok(prepared));
    mocks.get.mockImplementation(async () => ok(draft));

    show('/studio?new=1');
    expect(await screen.findByRole('heading', { name: 'What would you like to learn?' })).toBeVisible();
    await user.type(screen.getByLabelText('Your goal'), 'Understand these ideas and put them to use');
    await user.click(screen.getByRole('button', { name: 'Add optional materials' }));
    await user.click(await screen.findByRole('checkbox', { name: /Field guide\.pdf/ }));
    await user.click(screen.getByRole('button', { name: 'Create outline' }));
    expect(await screen.findByRole('textbox', { name: 'Program title' })).toHaveValue('A thoughtful course');
    expect(mocks.generate).toHaveBeenCalledWith({ goal: 'Understand these ideas and put them to use', priorKnowledge: '', minutesPerSession: 30, documentIds: ['document-1'], sourceUrls: [], courseDepth: 'course' }, expect.objectContaining({ requestId: expect.any(String), onProgress: expect.any(Function) }));
    expect(screen.getByText('Draft outline')).toBeVisible();
    const moduleWorkspace = screen.getByRole('navigation', { name: 'Module workspace' });
    expect(within(moduleWorkspace).getByRole('tab', { name: /^Lessons$/ })).toBeVisible();
    expect(within(moduleWorkspace).getByRole('tab', { name: /^Sources$/ })).toBeVisible();
    expect(within(moduleWorkspace).getAllByRole('tab')).toHaveLength(2);
    expect(within(moduleWorkspace).queryByRole('menuitem', { name: /^Plan$/ })).not.toBeInTheDocument();
    expect(screen.queryByTestId('learning-plan-panel')).not.toBeInTheDocument();
    expect(mocks.plan).not.toHaveBeenCalled();
    expect(mocks.memory).not.toHaveBeenCalled();
    await user.clear(screen.getByRole('textbox', { name: 'Program title' }));
    await user.type(screen.getByRole('textbox', { name: 'Program title' }), 'My considered plan');
    await user.click(screen.getByRole('button', { name: 'Accept program' }));
    await waitFor(() => expect(mocks.accept).toHaveBeenCalledWith({ programId: 'program-1', expectedRevision: 0, title: 'My considered plan' }));
    await user.selectOptions(screen.getByLabelText('Module'), 'module-2');
    expect(screen.getAllByText(/Putting it together gives the course/).some((element) => element.closest('[hidden]') === null)).toBe(true);
    expect(mocks.complete).not.toHaveBeenCalled();
    const compareCard = screen.getByRole('button', { name: /Compare approaches.*Outline/ }).closest('article');
    expect(compareCard).not.toBeNull();
    await user.click(within(compareCard as HTMLElement).getByRole('button', { name: 'Prepare lesson' }));
    await waitFor(() => expect(mocks.prepare).toHaveBeenCalledWith({ programId: 'program-1', lessonId: 'lesson-3', expectedRevision: 1 }));
    expect(screen.getByLabelText('Module')).toBeVisible();
  });

  it('validates a quiz, preserves answers through a failed submit, and retries with the same attempt id', async () => {
    const user = userEvent.setup();
    const active = program('active', true);
    const successful = structuredClone(active);
    successful.attempts = [{
      id: 'saved-attempt', moduleId: 'module-1', lessonId: 'lesson-1', kind: 'quiz', correct: 1, total: 1,
      submittedAt: 1_790_000_000_000,
      results: [{ questionId: 'quiz-1', prompt: quizQuestion.prompt, options: quizQuestion.options, selectedIndex: 0, correctIndex: 0, explanation: 'The source explains this principle.', sourceIds: ['source-1'] }],
    }];
    mocks.list.mockResolvedValue(ok([summary(active)]));
    mocks.get.mockResolvedValue(ok(active));
    mocks.submit.mockResolvedValueOnce(fail('Connection interrupted')).mockResolvedValueOnce(ok(successful));
    show();
    await user.click(await screen.findByRole('button', { name: /A thoughtful course/ }));
    await user.click(screen.getByRole('tab', { name: 'Practice' }));
    await user.click(screen.getByRole('tab', { name: 'Quick checks' }));
    await user.click(screen.getByRole('button', { name: /^quiz$/ }));
    expect(screen.getByRole('button', { name: 'Submit answers' })).toBeDisabled();
    expect(screen.queryByText('The source explains this principle.')).not.toBeInTheDocument();
    await user.click(screen.getByRole('radio', { name: 'First principle' }));
    await user.click(screen.getByRole('button', { name: 'Submit answers' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Connection interrupted');
    expect(screen.getByRole('radio', { name: 'First principle' })).toBeChecked();
    const firstAttemptId = mocks.submit.mock.calls[0][0].attemptId;
    await user.click(screen.getByRole('button', { name: 'Submit answers' }));
    await waitFor(() => expect(mocks.submit).toHaveBeenCalledTimes(2));
    expect(mocks.submit.mock.calls[1][0].attemptId).toBe(firstAttemptId);
    expect(mocks.submit.mock.calls[0][0]).toMatchObject({ kind: 'quiz', moduleId: 'module-1', lessonId: 'lesson-1', answers: [{ questionId: 'quiz-1', selectedIndex: 0 }] });
    await user.click(screen.getByRole('button', { name: 'History (1)' }));
    expect(await screen.findByText('Model-authored answer key')).toBeVisible();
    expect(screen.getByText('The source explains this principle.')).toBeVisible();
  });

  it('keeps a practical session mounted when switching workspace groups and exposes More as a keyboard menu', async () => {
    const user = userEvent.setup();
    const active = program('active', true);
    mocks.list.mockResolvedValue(ok([summary(active)]));
    mocks.get.mockResolvedValue(ok(active));
    mocks.plan.mockResolvedValue(ok({ programId: 'program-1', blocks: [] }));
    show();
    await user.click(await screen.findByRole('button', { name: /A thoughtful course/ }));
    const mainNav = screen.getByRole('tablist', { name: 'Program workspace' });
    expect(within(mainNav).getAllByRole('tab')).toHaveLength(5);
    await user.click(within(mainNav).getByRole('tab', { name: 'Practice' }));
    await user.click(screen.getByRole('tab', { name: 'Code & simulations' }));
    const lab = screen.getByRole('textbox', { name: 'Lab scratchpad' });
    await user.type(lab, 'keep this work');
    expect(screen.getByLabelText('Lesson context')).toHaveValue('lesson-1');
    await user.selectOptions(screen.getByLabelText('Lesson context'), '');
    await waitFor(() => expect(screen.getByLabelText('Lesson context')).toHaveValue(''));
    expect(screen.getByText('No lesson selected')).toBeVisible();
    await user.selectOptions(screen.getByLabelText('Lesson context'), 'lesson-1');
    await user.click(within(mainNav).getByRole('tab', { name: 'Recall' }));
    await user.click(within(mainNav).getByRole('tab', { name: 'Practice' }));
    await user.click(screen.getByRole('tab', { name: 'Code & simulations' }));
    expect(screen.getByRole('textbox', { name: 'Lab scratchpad' })).toHaveValue('keep this work');

    const more = screen.getByRole('button', { name: 'More' });
    await user.click(more);
    expect(screen.getByRole('menuitem', { name: 'Plan' })).toHaveFocus();
    await user.keyboard('{Escape}');
    expect(more).toHaveFocus();
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    await user.click(more);
    await user.click(screen.getByRole('menuitem', { name: 'Plan' }));
    expect(more).toHaveAttribute('aria-current', 'page');
    await waitFor(() => expect(screen.getByRole('region', { name: 'Program workspace content' })).toHaveFocus());
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
  });

  it('keeps the chosen lesson context when the save-before-switch step is delayed', async () => {
    const user = userEvent.setup();
    const active = program('active', true);
    mocks.list.mockResolvedValue(ok([summary(active)]));
    mocks.get.mockResolvedValue(ok(active));
    let resolveFlush!: (result: boolean) => void;
    mocks.flush.mockImplementationOnce(() => new Promise<boolean>((resolve) => { resolveFlush = resolve; }));
    show();
    await user.click(await screen.findByRole('button', { name: /A thoughtful course/ }));
    const lessonSelect = screen.getByLabelText('Lesson context');
    await user.selectOptions(lessonSelect, 'lesson-2');
    expect(lessonSelect).toBeDisabled();
    resolveFlush(true);
    await waitFor(() => expect(lessonSelect).toHaveValue('lesson-2'));
  });

  it('opens Canvas from More only after saves succeed and keeps the current section when a save fails', async () => {
    const user = userEvent.setup();
    const active = program('active', true);
    mocks.list.mockResolvedValue(ok([summary(active)]));
    mocks.get.mockResolvedValue(ok(active));
    show();
    await user.click(await screen.findByRole('button', { name: /A thoughtful course/ }));
    const more = screen.getByRole('button', { name: 'More' });
    await user.click(more);
    await user.click(screen.getByRole('menuitem', { name: 'Canvas' }));
    expect(await screen.findByTestId('canvas-panel')).toBeVisible();
    expect(more).toHaveAttribute('aria-current', 'page');
    await waitFor(() => expect(screen.getByRole('region', { name: 'Program workspace content' })).toHaveFocus());

    await user.click(more);
    mocks.flush.mockResolvedValueOnce(false);
    await user.click(screen.getByRole('menuitem', { name: 'Plan' }));
    expect(screen.getByTestId('canvas-panel')).toBeVisible();
    expect(screen.queryByTestId('plan-panel')).not.toBeInTheDocument();
    expect(more).toHaveAttribute('aria-current', 'page');
    expect(screen.getByRole('menu')).toBeVisible();
  });

  it('shows a recoverable program-list error and retries the load', async () => {
    const user = userEvent.setup();
    const active = program('active');
    mocks.list.mockResolvedValueOnce(fail('Database unavailable')).mockResolvedValueOnce(ok([summary(active)]));
    show();
    expect(await screen.findByRole('heading', { name: 'Programs are unavailable' })).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Retry' }));
    expect(await screen.findByRole('button', { name: /A thoughtful course/ })).toBeVisible();
    expect(mocks.list).toHaveBeenCalledTimes(2);
  });

  it('lists flashcard decks below programs and opens one by its deck link', async () => {
    const user = userEvent.setup();
    mocks.list.mockResolvedValue(ok([]));
    const deck = { id: 'deck-1', title: 'Cell biology', focus: 'mitochondria', studyGoal: '', modelName: 'test', createdAt: 0, cards: [] };
    mocks.decks.mockResolvedValue(ok([{ ...deck, cardCount: 3, dueCount: 1, quizAttempts: 0, quizCorrect: 0 }]));
    mocks.deck.mockResolvedValue(ok(deck));
    show();
    expect(await screen.findByRole('heading', { name: 'Flashcards' })).toBeVisible();
    await user.click(await screen.findByRole('button', { name: /Cell biology/ }));
    expect(await screen.findByRole('heading', { name: 'Cell biology' })).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Studio' }));
    expect(await screen.findByRole('heading', { name: 'Flashcards' })).toBeVisible();
  });

  it('opens a deck straight from a /studio?deck= link', async () => {
    mocks.list.mockResolvedValue(ok([]));
    mocks.deck.mockResolvedValue(ok({ id: 'deck-1', title: 'Cell biology', focus: '', studyGoal: '', modelName: 'test', createdAt: 0, cards: [] }));
    show('/studio?deck=deck-1');
    expect(await screen.findByRole('heading', { name: 'Cell biology' })).toBeVisible();
    expect(mocks.deck).toHaveBeenCalledWith('deck-1');
  });
});
