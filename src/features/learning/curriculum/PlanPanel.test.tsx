import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { PlanPanel } from '@/features/learning/curriculum/PlanPanel';

const mocks = vi.hoisted(() => ({ getPlan: vi.fn(), preview: vi.fn(), accept: vi.fn(), discard: vi.fn(), startDiagnostic: vi.fn(), submitDiagnostic: vi.fn(), skipDiagnostic: vi.fn(), cancelJob: vi.fn(), retryJob: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningPlan: mocks.getPlan,
  // The Verifier audit switch sits on the Plan tab; off, with no Verifier set.
  getLearningProgramVerifierAudit: async (programId: string) => ({ ok: true, data: {
    programId, on: false, webSearch: false, consentAt: null, consentProvider: null, verifier: null,
    destination: null, consentRequiredFor: null, blocked: null, usage: { audits: 0, inputTokens: 0, outputTokens: 0 },
  } }),
  previewLearningCurriculumRevision: mocks.preview,
  acceptLearningCurriculumRevision: mocks.accept,
  discardLearningCurriculumRevision: mocks.discard,
  startLearningDiagnostic: mocks.startDiagnostic,
  submitLearningDiagnostic: mocks.submitDiagnostic,
  skipLearningDiagnostic: mocks.skipDiagnostic,
  cancelLearningGenerationJob: mocks.cancelJob,
  retryLearningGenerationJob: mocks.retryJob,
} }));

const ok = <T,>(data: T) => ({ ok: true as const, data });
const fail = (error: string) => ({ ok: false as const, error });
const lesson = { id: 'lesson-1', title: 'Build a source-backed explanation', objective: 'Connect an observation to a conclusion.', estimatedMinutes: 25, state: 'outline', assessmentStarted: false, replacementLessonId: null };
const module = { id: 'module-1', title: 'Reading evidence', purpose: 'Practice reasoning from observations.', prerequisiteModuleIds: [], outcomeIds: ['outcome-1'], lessons: [lesson] };
const acceptedRevision = { id: 'revision-1', programId: 'program-1', revisionNumber: 4, parentRevisionId: null, reason: 'Initial plan.', modules: [module], createdAt: 1_790_000_000_000 };
const plan = (overrides: Record<string, unknown> = {}) => ({ programId: 'program-1', programRevision: 9, acceptedRevision, draftRevision: null, previewChanges: [], requiredLessonCountBefore: 1, requiredLessonCountAfter: 1, resumeLessonId: lesson.id, jobs: [{ id: 'job-1', programId: 'program-1', kind: 'lesson_preparation', payloadSha256: 'sha', baseRevisionNumber: 4, status: 'running', progressCompleted: 2, progressTotal: 5, progressMessage: 'Preparing a lesson', resultId: null, error: null, createdAt: 1_790_000_000_000, startedAt: null, finishedAt: null }], latestDiagnostic: null, ...overrides });
const diagnostic = { id: 'diagnostic-1', programId: 'program-1', status: 'active', prompts: [{ id: 'prompt-1', prompt: 'What does the passage show, and what remains uncertain?', outcomeId: 'outcome-1', sourceVersionIds: ['version-1'] }], responses: [], sourceCoverageGaps: [], createdAt: 1_790_000_000_000, submittedAt: null };
function renderPlan() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  return render(<PlanPanel programId="program-1" />, { wrapper });
}

describe('Learning Studio Plan editor', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.getPlan.mockResolvedValue(ok(plan()));
    mocks.preview.mockImplementation(async () => ok(plan({ draftRevision: { ...acceptedRevision, id: 'revision-2', revisionNumber: 5, parentRevisionId: acceptedRevision.id, reason: 'Clarify transfer practice.', modules: [{ ...module, lessons: [...module.lessons, { ...lesson, id: 'lesson-2', title: 'Compare a second case' }] }] }, previewChanges: [{ kind: 'added', lessonId: 'lesson-2', fromModuleId: null, toModuleId: module.id, description: 'Add “Compare a second case” after the current lesson.' }], requiredLessonCountBefore: 1, requiredLessonCountAfter: 2 })));
    mocks.accept.mockResolvedValue(ok(plan({ acceptedRevision: { ...acceptedRevision, revisionNumber: 5 }, draftRevision: null, requiredLessonCountBefore: 2, requiredLessonCountAfter: 2 })));
    mocks.discard.mockResolvedValue(ok(plan()));
    mocks.startDiagnostic.mockResolvedValue(ok(diagnostic));
    mocks.submitDiagnostic.mockResolvedValue(ok({ ...diagnostic, status: 'submitted' }));
    mocks.skipDiagnostic.mockResolvedValue(ok({ ...diagnostic, status: 'skipped' }));
    mocks.cancelJob.mockResolvedValue(ok({ id: 'job-1' }));
    mocks.retryJob.mockResolvedValue(ok({ id: 'job-2' }));
  });

  it('shows revision denominator, stable resume, and real job progress/actions', async () => {
    renderPlan();
    expect(await screen.findByText('Required lesson count')).toBeVisible();
    expect(screen.getByText('Current plan to draft preview')).toBeVisible();
    expect(screen.getByText(/Resume: Build a source-backed explanation/)).toBeVisible();
    // The running job is one status line; its staging and progress sit behind Details (§2.2).
    expect(screen.queryByText(/2\/5 lessons staged/)).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Details' }));
    expect(screen.getByText(/2\/5 lessons staged/)).toBeVisible();
    expect(screen.getByText('Preparing a lesson')).toBeVisible();
    const pause = screen.getByRole('button', { name: 'Pause lesson preparation' });
    await userEvent.setup().click(pause);
    await waitFor(() => expect(mocks.cancelJob).toHaveBeenCalledWith(expect.objectContaining({ jobId: 'job-1', expectedRevision: 9 })));
  });

  it('folds attempts a retry superseded into one closed list of one-line outcomes', async () => {
    const running = plan().jobs[0];
    const attempt = (id: string, status: string, retryOfJobId: string | null, createdAt: number, error: string | null) => ({
      ...running, id, status, retryOfJobId, createdAt, finishedAt: createdAt + 1000, error,
      activity: { phase: 'evidence', phaseStartedAt: 0, lastActivityAt: 0, lastCheckpointAt: null, lessonTitle: null, modelName: null, modelRunning: false, responseCharacters: 0, modelAttempt: 0, verificationPass: 2, checksCompleted: 0, checksTotal: 0, checksReused: 0, checksUnresolved: 0, checksSentBack: 0, newReferences: null, modelChecksTotal: null, deeperEvidence: null, claims: null, recentSteps: [] },
    });
    mocks.getPlan.mockResolvedValue(ok(plan({ jobs: [
      attempt('job-a', 'failed', null, 1_790_000_000_000, 'The model rejected the request.'),
      attempt('job-b', 'cancelled', 'job-a', 1_790_000_100_000, null),
      { ...running, id: 'job-c', retryOfJobId: 'job-b', createdAt: 1_790_000_200_000 },
    ] })));
    renderPlan();
    // Only the running attempt gets a status line; its progress sits behind Details (§2.2).
    await userEvent.click(await screen.findByRole('button', { name: 'Details' }));
    expect(screen.getByText('Preparing a lesson')).toBeVisible();
    expect(screen.getAllByRole('button', { name: 'Pause lesson preparation' })).toHaveLength(1);
    const earlier = screen.getByText('Earlier attempts (2)');
    const disclosure = earlier.closest('details')!;
    expect(disclosure).not.toHaveAttribute('open');
    const lines = within(disclosure).getAllByRole('listitem').map((item) => item.textContent);
    expect(lines[0]).toMatch(/^Paused at Verifying factual claims · /);
    expect(lines[1]).toMatch(/^Stopped at Verifying factual claims · .* · The model rejected the request\.$/);
  });

  it('composes an add operation, previews semantic changes, and accepts the immutable revision', async () => {
    const user = userEvent.setup();
    renderPlan();
    await user.click(await screen.findByRole('button', { name: /Add lesson/ }));
    await user.type(screen.getByLabelText('Title'), 'Compare a second case');
    await user.type(screen.getByLabelText('Objective'), 'Test whether the pattern transfers.');
    await user.click(screen.getByRole('button', { name: 'Add to preview' }));
    expect(screen.getByText(/Add lesson · Compare a second case/)).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Preview changes' }));
    expect(await screen.findByText('Add “Compare a second case” after the current lesson.')).toBeVisible();
    expect(mocks.preview).toHaveBeenCalledWith(expect.objectContaining({ expectedRevision: 9, operations: [expect.objectContaining({ kind: 'add_lesson', module_id: 'module-1', lesson: expect.objectContaining({ title: 'Compare a second case', state: 'outline' }) })] }));
    await user.click(screen.getByRole('button', { name: 'Accept revision' }));
    await waitFor(() => expect(mocks.accept).toHaveBeenCalledWith(expect.objectContaining({ revisionId: 'revision-2', expectedRevision: 9 })));
  });

  it('collects diagnostic responses and submits coverage-only results without a mastery score', async () => {
    let currentPlan = plan();
    mocks.getPlan.mockImplementation(async () => ok(currentPlan));
    mocks.startDiagnostic.mockImplementation(async () => { currentPlan = plan({ latestDiagnostic: diagnostic }); return ok(diagnostic); });
    mocks.submitDiagnostic.mockImplementation(async (request) => { currentPlan = plan({ latestDiagnostic: { ...diagnostic, status: 'submitted', responses: request.responses, sourceCoverageGaps: [{ outcomeId: 'outcome-1', outcomeTitle: 'Reason carefully', sourceVersionIds: [] }] } }); return ok(currentPlan.latestDiagnostic); });
    const user = userEvent.setup();
    renderPlan();
    await user.click(await screen.findByRole('button', { name: 'Start diagnostic' }));
    await waitFor(() => expect(mocks.startDiagnostic).toHaveBeenCalledWith(expect.objectContaining({ expectedRevision: 9 })));
    await user.type(await screen.findByRole('textbox', { name: /Response:/ }), 'It shows the pattern, but not its cause.');
    await user.click(screen.getByRole('button', { name: 'Submit diagnostic' }));
    await waitFor(() => expect(mocks.submitDiagnostic).toHaveBeenCalledWith(expect.objectContaining({ diagnosticId: 'diagnostic-1', expectedRevision: 9, responses: [{ promptId: 'prompt-1', response: 'It shows the pattern, but not its cause.' }] })));
    expect(await screen.findByText(/makes no mastery claim/)).toBeVisible();
    expect(screen.getByText(/Reason carefully/)).toBeVisible();
    expect(screen.queryByText(/mastery score: [0-9]/i)).not.toBeInTheDocument();
  });

  it('offers real retry controls after preview failure and keeps the accepted plan intact', async () => {
    mocks.preview.mockResolvedValueOnce(fail('Revision changed; reload and retry.'));
    const user = userEvent.setup();
    renderPlan();
    await user.click(await screen.findByRole('button', { name: /Add lesson/ }));
    await user.type(screen.getByLabelText('Title'), 'A second case');
    await user.type(screen.getByLabelText('Objective'), 'Compare another example.');
    await user.click(screen.getByRole('button', { name: 'Add to preview' }));
    await user.click(screen.getByRole('button', { name: 'Preview changes' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Revision changed; reload and retry.');
    expect(screen.getByText('Accepted revision 4')).toBeVisible();
  });

  it('protects completed and assessed lessons while previewing allowed plan edits', async () => {
    const completed = { ...lesson, id: 'lesson-done', title: 'Completed lesson', state: 'completed' };
    const assessed = { ...lesson, id: 'lesson-assessed', title: 'Assessment started lesson', assessmentStarted: true };
    const editable = { ...lesson, id: 'lesson-editable', title: 'Editable lesson' };
    mocks.getPlan.mockResolvedValue(ok(plan({
      acceptedRevision: { ...acceptedRevision, modules: [{ ...module, lessons: [completed, assessed, editable] }] },
      resumeLessonId: editable.id,
    })));
    mocks.preview.mockResolvedValue(ok(plan({
      draftRevision: { ...acceptedRevision, id: 'revision-2', revisionNumber: 5, parentRevisionId: acceptedRevision.id },
      previewChanges: [
        { kind: 'skipped', lessonId: editable.id, fromModuleId: module.id, toModuleId: null, description: 'Skip Editable lesson.' },
        { kind: 'prerequisite_challenged', lessonId: assessed.id, fromModuleId: module.id, toModuleId: module.id, description: 'Challenge the prerequisite for Assessment started lesson.' },
      ],
    })));

    const user = userEvent.setup();
    renderPlan();
    await screen.findByText('Assessment started lesson');

    const completedArticle = screen.getByText('Completed lesson').closest('article');
    const assessedArticle = screen.getByText('Assessment started lesson').closest('article');
    expect(completedArticle).not.toBeNull();
    expect(assessedArticle).not.toBeNull();
    for (const article of [completedArticle, assessedArticle] as HTMLElement[]) {
      expect(within(article).getByRole('button', { name: 'Edit' })).toBeDisabled();
      expect(within(article).getByRole('button', { name: /Move/ })).toBeDisabled();
      expect(within(article).getByRole('button', { name: 'Skip' })).toBeDisabled();
      expect(within(article).getByRole('button', { name: 'Replace' })).toBeDisabled();
    }
    expect(within(assessedArticle as HTMLElement).getByRole('button', { name: 'Challenge prereq' })).toBeEnabled();

    const editableArticle = screen.getByText('Editable lesson').closest('article') as HTMLElement;
    await user.click(within(editableArticle).getByRole('button', { name: 'Skip' }));
    await user.click(within(assessedArticle as HTMLElement).getByRole('button', { name: 'Challenge prereq' }));
    await user.click(screen.getByRole('button', { name: 'Preview changes' }));

    await waitFor(() => expect(mocks.preview).toHaveBeenCalledWith(expect.objectContaining({
      expectedRevision: 9,
      operations: [
        { kind: 'skip_lesson', lesson_id: editable.id },
        { kind: 'challenge_prerequisite', lesson_id: assessed.id },
      ],
    })));
    expect(await screen.findByText('Skip Editable lesson.')).toBeVisible();
    expect(screen.getByText('Challenge the prerequisite for Assessment started lesson.')).toBeVisible();
  });

  it('edits and moves a lesson through the revision editor before previewing both changes', async () => {
    const secondModule = { ...module, id: 'module-2', title: 'Transfer practice', lessons: [] };
    mocks.getPlan.mockResolvedValue(ok(plan({ acceptedRevision: { ...acceptedRevision, modules: [module, secondModule] } })));
    mocks.preview.mockResolvedValue(ok(plan({
      draftRevision: { ...acceptedRevision, id: 'revision-2', revisionNumber: 5, parentRevisionId: acceptedRevision.id },
      previewChanges: [{ kind: 'updated', lessonId: lesson.id, fromModuleId: module.id, toModuleId: secondModule.id, description: 'Update and move the lesson.' }],
    })));

    const user = userEvent.setup();
    renderPlan();
    await screen.findByText(lesson.title);
    const lessonArticle = screen.getByText(lesson.title).closest('article') as HTMLElement;

    await user.click(within(lessonArticle).getByRole('button', { name: 'Edit' }));
    const titleInput = screen.getByRole('textbox', { name: 'Title' });
    expect(titleInput).toHaveFocus();
    await user.clear(titleInput);
    await user.type(titleInput, 'Read two bodies of evidence');
    await user.clear(screen.getByRole('textbox', { name: 'Objective' }));
    await user.type(screen.getByRole('textbox', { name: 'Objective' }), 'Compare how the evidence supports each claim.');
    await user.click(screen.getByRole('button', { name: 'Add to preview' }));

    await user.click(within(lessonArticle).getByRole('button', { name: `Move ${lesson.title}` }));
    await user.selectOptions(screen.getByRole('combobox', { name: 'Module' }), 'module-2');
    await user.click(screen.getByRole('button', { name: 'Add to preview' }));
    const proposedOperations = screen.getByRole('heading', { name: 'Proposed operations' }).parentElement?.parentElement as HTMLElement;
    expect(proposedOperations).toHaveTextContent(/Edit lesson.*Build a source-backed explanation/);
    expect(proposedOperations).toHaveTextContent(/Move lesson.*Build a source-backed explanation/);

    await user.click(screen.getByRole('button', { name: 'Preview changes' }));
    await waitFor(() => expect(mocks.preview).toHaveBeenCalledWith(expect.objectContaining({
      expectedRevision: 9,
      operations: [
        { kind: 'edit_lesson', lesson_id: lesson.id, title: 'Read two bodies of evidence', objective: 'Compare how the evidence supports each claim.', estimated_minutes: lesson.estimatedMinutes },
        { kind: 'move_lesson', lesson_id: lesson.id, target_module_id: 'module-2', after_lesson_id: null },
      ],
    })));
  });

  it('closes the lesson editor on Escape or Cancel and restores focus to its opener', async () => {
    const user = userEvent.setup();
    renderPlan();
    const addLesson = await screen.findByRole('button', { name: /Add lesson/ });

    await user.click(addLesson);
    expect(screen.getByRole('dialog', { name: 'Add a lesson' })).toBeVisible();
    const titleInput = screen.getByRole('textbox', { name: 'Title' });
    expect(titleInput).toHaveFocus();
    await user.tab({ shift: true });
    expect(within(screen.getByRole('dialog')).getByRole('button', { name: 'Cancel' })).toHaveFocus();
    await user.tab();
    expect(titleInput).toHaveFocus();
    await user.keyboard('{Escape}');
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    await waitFor(() => expect(addLesson).toHaveFocus());

    await user.click(addLesson);
    await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Cancel' }));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    await waitFor(() => expect(addLesson).toHaveFocus());
  });
});
