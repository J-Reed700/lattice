import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { PracticeWorkbenchPanel } from '@/features/learning/practice/PracticeWorkbenchPanel';
import type { LearningLessonDto, LearningPracticeSessionDto, LearningProgramDto } from '@/lib/bindings';


const mocks = vi.hoisted(() => ({
  getWorkspace: vi.fn(), getSession: vi.fn(), start: vi.fn(), save: vi.fn(), mode: vi.fn(), openSource: vi.fn(), tutor: vi.fn(), reveal: vi.fn(), submit: vi.fn(), acceptProposal: vi.fn(), rejectProposal: vi.fn(),
  sourceWorkspace: vi.fn(), sourceVersion: vi.fn(),
}));

vi.mock('@/lib/api', () => ({ default: {
  getLearningPracticeWorkspace: mocks.getWorkspace,
  getLearningPracticeSession: mocks.getSession,
  startLearningPracticeSession: mocks.start,
  saveLearningPracticeArtifact: mocks.save,
  changeLearningPracticeMode: mocks.mode,
  openLearningPracticeSource: mocks.openSource,
  requestLearningTutorResponse: mocks.tutor,
  revealLearningPracticeSolution: mocks.reveal,
  submitLearningPracticeAttempt: mocks.submit,
  acceptLearningPracticeProposal: mocks.acceptProposal,
  rejectLearningPracticeProposal: mocks.rejectProposal,
  getLearningSourceWorkspace: mocks.sourceWorkspace,
  getLearningSourceVersion: mocks.sourceVersion,
} }));

const ok = <T,>(data: T) => ({ ok: true as const, data });
const fail = (error: string) => ({ ok: false as const, error });
const lesson = { id: 'lesson-1', title: 'Read the evidence', preparation: 'ready', objective: 'Distinguish observation from inference.' } as LearningLessonDto;
const program = { summary: { id: 'program-1', title: 'Evidence', revision: 8 } } as LearningProgramDto;
const savedVersion = { id: 'version-1', versionNumber: 1, title: 'Field notes', excerpt: 'A saved excerpt.', acquiredAt: 1_790_000_000_000, wordCount: 4, publisher: 'Archive', resolvedUrl: null, contentSha256: 'sha', truncated: false, extractionVersion: 'v1' };
const sourceWorkspace = { programId: 'program-1', sources: [{ id: 'source-1', kind: 'pasted', origin: 'My notes', requestedUrl: null, freshnessPolicy: 'fixed', activeVersionId: 'version-1', pendingVersionId: null, revision: 1, activeVersion: savedVersion, pendingVersion: null, versions: [savedVersion], latestCheck: null, checks: [], createdAt: 1_790_000_000_000, updatedAt: 1_790_000_000_000 }] };
const baseSession = (): LearningPracticeSessionDto => ({
  summary: { id: 'attempt-1', lessonId: 'lesson-1', lessonTitle: lesson.title, status: 'active', mode: 'explore', revision: 1, artifactRevision: 0, sourceVersionIds: ['version-1'], createdAt: 1_790_000_000_000, updatedAt: 1_790_000_000_000, submittedAt: null, gradeStatus: null },
  taskPrompt: 'Explain why the observation supports one conclusion better than another.', lessonObjective: lesson.objective,
  rubric: [{ id: 'criterion-1', dimension: 'explanation', title: 'Reasoning', description: 'Connect evidence to the claim.', maxPoints: 4 }],
  artifact: { revision: 0, text: '', sha256: 'empty', updatedAt: 1_790_000_000_000 }, assistance: [], tutorTurns: [],
  proposals: [{ id: 'proposal-1', kind: 'follow_up', text: 'Compare the same evidence under a different condition.', evidenceQuote: 'the observation', tutorTurnId: 'turn-1', status: 'pending', createdAt: 1_790_000_000_000, decidedAt: null }],
  revealedSolution: null, revealedSolutionCitations: [], result: null,
});

let currentSession: ReturnType<typeof baseSession> | null;
let currentWorkspace: { programId: string; sessions: unknown[] };
const toWorkspace = () => ({ programId: 'program-1', sessions: currentWorkspace.sessions });
const updateRevision = () => { if (currentSession) currentSession.summary.revision += 1; };
const renderWorkbench = (chosenLesson = lesson, embedded = false) => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  return render(<PracticeWorkbenchPanel program={program} lesson={chosenLesson} embedded={embedded} taskKind={embedded ? 'guided' : 'independent'} />, { wrapper });
};

describe('Learning Studio grounded practice workbench', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    currentSession = baseSession();
    currentWorkspace = { programId: 'program-1', sessions: [{ ...currentSession.summary }] };
    mocks.getWorkspace.mockImplementation(async () => ok(toWorkspace()));
    mocks.getSession.mockImplementation(async () => currentSession ? ok(currentSession) : fail('Attempt not found'));
    mocks.start.mockImplementation(async (request) => {
      currentSession = baseSession();
      currentSession.summary.id = request.sessionId;
      currentSession.summary.mode = request.mode;
      currentSession.summary.taskKind = request.taskKind;
      currentSession.summary.revisesSessionId = request.revisesSessionId;
      currentWorkspace.sessions = [{ ...currentSession.summary }];
      return ok(toWorkspace());
    });
    mocks.sourceWorkspace.mockResolvedValue(ok(sourceWorkspace));
    mocks.sourceVersion.mockResolvedValue(ok({ sourceId: 'source-1', version: savedVersion, fullText: 'A saved excerpt.\n\nThe full captured source.', usage: [] }));
    mocks.save.mockImplementation(async (request) => {
      if (!currentSession) return fail('Attempt not found');
      currentSession.artifact.text = request.text;
      currentSession.artifact.revision += 1;
      currentSession.artifact.sha256 = `digest-${currentSession.artifact.revision}`;
      currentSession.summary.artifactRevision = currentSession.artifact.revision;
      updateRevision();
      return ok(toWorkspace());
    });
    mocks.mode.mockImplementation(async (request) => { if (currentSession) { currentSession.summary.mode = request.mode; updateRevision(); } return ok(toWorkspace()); });
    mocks.openSource.mockImplementation(async () => { if (currentSession) { currentSession.assistance.push({ id: 'assist-source', kind: 'source_opened', mode: currentSession.summary.mode, artifactRevision: currentSession.artifact.revision, details: {}, createdAt: Date.now() }); updateRevision(); } return ok(toWorkspace()); });
    mocks.tutor.mockImplementation(async (request) => { if (currentSession) { if (request.requestKind === 'hint') currentSession.assistance.push({ id: `hint-${currentSession.tutorTurns.length}`, kind: 'hint', mode: currentSession.summary.mode, artifactRevision: currentSession.artifact.revision, details: { hintLevel: request.hintLevel }, createdAt: Date.now() }); currentSession.tutorTurns.push({ id: `turn-${currentSession.tutorTurns.length + 1}`, prompt: request.prompt, response: 'Start by separating the observation from your interpretation.', requestKind: request.requestKind, hintLevel: request.hintLevel, citations: [{ sourceId: 'source-1', versionId: 'version-1', quote: 'A saved excerpt.' }], proposalIds: [], modelName: 'Study model', createdAt: Date.now() }); updateRevision(); } return ok(toWorkspace()); });
    mocks.reveal.mockImplementation(async () => { if (currentSession) { currentSession.revealedSolution = 'A worked explanation of the inference.'; updateRevision(); } return ok(toWorkspace()); });
    mocks.submit.mockImplementation(async () => { if (currentSession) { currentSession.summary.status = 'submitted'; currentSession.summary.submittedAt = Date.now(); currentSession.summary.gradeStatus = 'uncertain'; currentSession.result = { gradeStatus: 'uncertain', artifactRevision: currentSession.artifact.revision, artifactText: currentSession.artifact.text, rubric: currentSession.rubric, criteria: [{ criterionId: 'criterion-1', dimension: 'explanation', score: null, maxPoints: 4, observation: 'There is not enough evidence to judge the connection.', evidenceQuote: null }], evidence: [{ dimension: 'explanation', observed: false, observation: 'No complete explanation was visible.', evidenceQuote: null, assistanceKinds: [] }], modeAtSubmission: currentSession.summary.mode, assistance: currentSession.assistance, graderModel: 'Study grader', submittedAt: Date.now() }; updateRevision(); } return ok(toWorkspace()); });
    mocks.acceptProposal.mockImplementation(async () => { if (currentSession) { currentSession.proposals[0].status = 'accepted'; updateRevision(); } return ok(toWorkspace()); });
    mocks.rejectProposal.mockImplementation(async () => { if (currentSession) { currentSession.proposals[0].status = 'rejected'; updateRevision(); } return ok(toWorkspace()); });
  });

  it('keeps guided practice in the lesson, saves work before feedback, and reveals hints progressively', async () => {
    const user = userEvent.setup();
    currentSession = null; currentWorkspace.sessions = [];
    renderWorkbench(lesson, true);
    await user.click(await screen.findByRole('button', { name: 'Start guided exercise' }));
    expect(mocks.start).toHaveBeenCalledWith(expect.objectContaining({ taskKind: 'guided', mode: 'practice' }));
    await user.type(await screen.findByRole('textbox', { name: 'Your working' }), 'I separated the observed facts from my interpretation.');
    await user.click(screen.getByRole('button', { name: 'Check my reasoning' }));
    await waitFor(() => expect(mocks.tutor).toHaveBeenCalledWith(expect.objectContaining({ requestKind: 'critique' })));
    expect(mocks.save.mock.invocationCallOrder[0]).toBeLessThan(mocks.tutor.mock.invocationCallOrder[0]);
    await user.click(await screen.findByRole('button', { name: 'Give me a hint' }));
    await waitFor(() => expect(mocks.tutor).toHaveBeenCalledWith(expect.objectContaining({ hintLevel: 'orienting_question' })));
    expect(await screen.findByRole('button', { name: 'Next hint' })).toBeVisible();
    expect(screen.queryByRole('button', { name: 'Reveal solution' })).not.toBeInTheDocument();
  });

  it('revises a submitted guided response with its original attempt ID', async () => {
    const user = userEvent.setup();
    currentSession!.summary.taskKind = 'guided'; currentSession!.summary.status = 'submitted';
    currentSession!.result = { gradeStatus: 'uncertain', criteria: [], rubric: currentSession!.rubric, evidence: [], assistance: [], modeAtSubmission: 'practice', graderModel: 'fixture', submittedAt: 10, artifactRevision: 0, artifactText: 'first attempt' };
    currentWorkspace.sessions = [{ ...currentSession!.summary }];
    renderWorkbench(lesson, true);
    await user.click(await screen.findByRole('button', { name: 'Revise this response' }));
    await waitFor(() => expect(mocks.start).toHaveBeenCalledWith(expect.objectContaining({ revisesSessionId: 'attempt-1', taskKind: 'guided', mode: 'practice' })));
  });

  it('gates attempts on lesson preparation', async () => {
    renderWorkbench({ ...lesson, preparation: 'outline' } as LearningLessonDto);
    expect(await screen.findByText('Prepare this lesson first')).toBeVisible();
    expect(screen.queryByRole('button', { name: 'Start an attempt' })).not.toBeInTheDocument();
  });

  it('starts a new attempt with the chosen aid contract', async () => {
    const user = userEvent.setup();
    currentSession = null;
    currentWorkspace.sessions = [];
    renderWorkbench();
    await user.click(await screen.findByRole('radio', { name: /Demonstrate/ }));
    await user.click(screen.getByRole('button', { name: 'Start an attempt' }));
    await screen.findByRole('textbox', { name: 'Your response' });
    expect(mocks.start).toHaveBeenCalledWith(expect.objectContaining({ lessonId: 'lesson-1', mode: 'demonstrate', expectedProgramRevision: 8 }));
  });

  it('debounces response autosave and retries a lost response with the same operation ID and payload', async () => {
    const user = userEvent.setup();
    mocks.save.mockImplementationOnce(async () => fail('Connection interrupted'));
    renderWorkbench();
    const editor = await screen.findByRole('textbox', { name: 'Your response' });
    await user.type(editor, 'Evidence supports the conclusion because…');
    await waitFor(() => expect(mocks.save).toHaveBeenCalledTimes(1), { timeout: 3_000 });
    expect(await screen.findByRole('button', { name: 'Retry save' })).toBeVisible();
    const first = mocks.save.mock.calls[0][0];
    await user.click(screen.getByRole('button', { name: 'Retry save' }));
    await waitFor(() => expect(mocks.save).toHaveBeenCalledTimes(2));
    expect(mocks.save.mock.calls[1][0]).toEqual(first);
    expect(screen.getByRole('textbox', { name: 'Your response' })).toHaveValue(first.text);
  });

  it('enforces the Demonstrate aid contract and opens only frozen versions after submission', async () => {
    const user = userEvent.setup();
    currentSession!.summary.mode = 'demonstrate';
    renderWorkbench();
    expect(await screen.findByRole('textbox', { name: 'Your response' })).toBeVisible();
    expect(screen.getByRole('button', { name: /Field notes/ })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Ask tutor' })).toBeDisabled();
    expect(screen.queryByRole('button', { name: /Review reveal choice/ })).not.toBeInTheDocument();
    await user.type(screen.getByRole('textbox', { name: 'Your response' }), 'My independent response.');
    await user.click(screen.getByRole('button', { name: 'Submit response' }));
    expect(await screen.findByText(/Evidence interpretation is uncertain/)).toBeVisible();
    await user.click(screen.getByRole('button', { name: /Field notes/ }));
    expect(await screen.findByText(/The full captured source/)).toBeVisible();
    expect(mocks.openSource).not.toHaveBeenCalled();
    expect(mocks.sourceVersion).toHaveBeenCalledWith({ programId: 'program-1', sourceId: 'source-1', versionId: 'version-1' });
  });

  it('requires confirmation before revealing and opens tutor citations against their exact saved version', async () => {
    const user = userEvent.setup();
    renderWorkbench();
    await screen.findByRole('textbox', { name: 'Your response' });
    await user.click(screen.getByRole('button', { name: 'Review reveal choice' }));
    expect(screen.getByRole('dialog', { name: 'Reveal the worked solution?' })).toBeVisible();
    expect(mocks.reveal).not.toHaveBeenCalled();
    await user.click(screen.getByRole('button', { name: 'Reveal solution' }));
    expect(await screen.findByText('A worked explanation of the inference.')).toBeVisible();
    await user.type(screen.getByRole('textbox', { name: 'Ask a question or request critique' }), 'Can you explain this source?');
    await user.click(screen.getByRole('button', { name: 'Ask tutor' }));
    expect(await screen.findByText('Start by separating the observation from your interpretation.')).toBeVisible();
    const citation = screen.getByRole('button', { name: /Saved source quote · exact version/ });
    await user.click(citation);
    await waitFor(() => expect(mocks.openSource).toHaveBeenCalledWith(expect.objectContaining({ sourceId: 'source-1', versionId: 'version-1' })));
    expect(await screen.findByText(/The full captured source/)).toBeVisible();
  });

  it('shows rubric evidence and lets the learner accept or reject tutor proposals', async () => {
    const user = userEvent.setup();
    renderWorkbench();
    await screen.findByRole('textbox', { name: 'Your response' });
    const proposals = screen.getByText('Tutor proposals').parentElement!;
    await user.click(within(proposals).getByRole('button', { name: 'Accept idea' }));
    await waitFor(() => expect(mocks.acceptProposal).toHaveBeenCalledWith(expect.objectContaining({ proposalId: 'proposal-1' })));
    await user.type(screen.getByRole('textbox', { name: 'Your response' }), 'My evidence-based explanation.');
    await user.click(screen.getByRole('button', { name: 'Submit response' }));
    expect(await screen.findByText('Feedback on revision 1')).toBeVisible();
    expect(screen.getByText('There is not enough evidence to judge the connection.')).toBeInTheDocument();
  });

  it('offers a conflict reload while retaining locally edited response text', async () => {
    const user = userEvent.setup();
    mocks.save.mockImplementationOnce(async () => fail('Revision changed; reload and retry.'));
    renderWorkbench();
    const editor = await screen.findByRole('textbox', { name: 'Your response' });
    await user.type(editor, 'Keep this draft after reload.');
    expect(await screen.findByRole('button', { name: 'Reload current revision · keep my text' })).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Reload current revision · keep my text' }));
    expect(screen.getByRole('textbox', { name: 'Your response' })).toHaveValue('Keep this draft after reload.');
  });
});
