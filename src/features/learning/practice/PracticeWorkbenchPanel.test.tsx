import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import '@/tests/interactionDom';

import { LEARNER_TEXT_MAX_CHARS } from '@/features/learning/model/learnerTextLimit';
import { PracticeWorkbenchPanel } from '@/features/learning/practice/PracticeWorkbenchPanel';
import { ReaderPane } from '@/features/reading/components/ReaderPane';
import { useCitationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import { useReaderStore } from '@/features/reading/stores/readerStore';
import type { LearningLessonDto, LearningPracticeSessionDto, LearningProgramDto } from '@/lib/bindings';


const mocks = vi.hoisted(() => ({
  getWorkspace: vi.fn(), getSession: vi.fn(), start: vi.fn(), save: vi.fn(), mode: vi.fn(), openSource: vi.fn(), tutor: vi.fn(), reveal: vi.fn(), submit: vi.fn(), acceptProposal: vi.fn(), rejectProposal: vi.fn(),
  sourceWorkspace: vi.fn(), sourceVersion: vi.fn(), saveDraft: vi.fn(),
}));

// The docked reader's body asks the conversations store which chat it sits beside; none here.
vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: (selector: (_state: unknown) => unknown) =>
    selector({ activeConversationId: null, conversations: [], spaces: [], selectedSpaceId: null, messageVerification: new Map(), loadConversationLinkedDocuments: vi.fn() }),
}));
vi.mock('@/features/voice/hooks/useVoiceConversation', () => ({ useVoiceConversation: () => ({ phase: 'off', toggle: vi.fn() }) }));
vi.mock('@/lib/api', async (importOriginal) => ({ ...(await importOriginal<typeof import('@/lib/api')>()), default: {
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
  saveLearningCardDraft: mocks.saveDraft,
} }));

const reviewCard = vi.hoisted(() => vi.fn());
vi.mock('@/features/learning/practice/shared/SaveAsReviewCard', () => ({
  SaveAsReviewCard: (props: Record<string, unknown>) => {
    reviewCard(props);
    return <div data-testid="review-card" />;
  },
}));

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
const renderWorkbench = (chosenLesson = lesson, embedded = false, attemptId?: string | null) => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}><MemoryRouter>{children}</MemoryRouter></QueryClientProvider>;
  return render(<><PracticeWorkbenchPanel program={program} lesson={chosenLesson} embedded={embedded} attemptId={attemptId} taskKind={embedded ? 'guided' : 'independent'} /><ReaderPane rowWidth={1728} availableWidth={1468} resetKey="practice" /></>, { wrapper });
};

describe('Learning Studio grounded practice workbench', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    useCitationDisplayStore.setState({ visible: true });
    useReaderStore.setState({ session: null, resolvedLocations: new Map() });
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
    mocks.tutor.mockImplementation(async (request) => { if (currentSession) { if (request.requestKind === 'hint') currentSession.assistance.push({ id: `hint-${currentSession.tutorTurns.length}`, kind: 'hint', mode: currentSession.summary.mode, artifactRevision: currentSession.artifact.revision, details: { hintLevel: request.hintLevel }, createdAt: Date.now() }); currentSession.tutorTurns.push({ id: `turn-${currentSession.tutorTurns.length + 1}`, prompt: request.prompt, response: 'Start by separating the observation from your interpretation.', requestKind: request.requestKind, hintLevel: request.hintLevel, citations: [{ number: 1, sourceId: 'source-1', versionId: 'version-1', quote: 'A saved excerpt.', support: { relation: 'quotes', anchors: [{ exact: 'A saved excerpt.', span: { start: 0, end: 16 } }] } }], proposalIds: [], modelName: 'Study model', createdAt: Date.now() }); updateRevision(); } return ok(toWorkspace()); });
    mocks.reveal.mockImplementation(async () => { if (currentSession) { currentSession.revealedSolution = 'A worked explanation of the inference.'; updateRevision(); } return ok(toWorkspace()); });
    mocks.submit.mockImplementation(async () => { if (currentSession) { currentSession.summary.status = 'submitted'; currentSession.summary.submittedAt = Date.now(); currentSession.summary.gradeStatus = 'uncertain'; currentSession.result = { gradeStatus: 'uncertain', artifactRevision: currentSession.artifact.revision, artifactText: currentSession.artifact.text, rubric: currentSession.rubric, criteria: [{ criterionId: 'criterion-1', dimension: 'explanation', score: null, maxPoints: 4, observation: 'There is not enough evidence to judge the connection.', evidenceQuote: null }], evidence: [{ dimension: 'explanation', observed: false, observation: 'No complete explanation was visible.', evidenceQuote: null, assistanceKinds: [] }], modeAtSubmission: currentSession.summary.mode, assistance: currentSession.assistance, graderModel: 'Study grader', submittedAt: Date.now() }; updateRevision(); } return ok(toWorkspace()); });
    mocks.acceptProposal.mockImplementation(async () => { if (currentSession) { currentSession.proposals[0].status = 'accepted'; updateRevision(); } return ok(toWorkspace()); });
    mocks.rejectProposal.mockImplementation(async () => { if (currentSession) { currentSession.proposals[0].status = 'rejected'; updateRevision(); } return ok(toWorkspace()); });
  });

  it('keeps guided practice in the lesson, saves work before feedback, and reveals hints progressively', async () => {
    const user = userEvent.setup();
    currentSession = null; currentWorkspace.sessions = [];
    renderWorkbench(lesson, true);
    await user.click(await screen.findByRole('button', { name: 'Start guided attempt' }));
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

  it('labels the lesson guided attempt as a region with its start button', async () => {
    currentSession = null; currentWorkspace.sessions = [];
    renderWorkbench(lesson, true);
    const guided = await screen.findByRole('region', { name: 'Guided attempt' });
    expect(within(guided).getByRole('button', { name: 'Start guided attempt' })).toBeVisible();
  });

  it('keeps the guided answer editable while an autosave is in flight', async () => {
    const user = userEvent.setup();
    currentSession!.summary.taskKind = 'guided';
    currentWorkspace.sessions = [{ ...currentSession!.summary }];
    const saved = mocks.save.getMockImplementation()!;
    let release!: () => void;
    mocks.save.mockImplementationOnce((request) => new Promise((resolve) => { release = () => resolve(saved(request)); }));
    renderWorkbench(lesson, true);
    const answer = await screen.findByRole('textbox', { name: 'Your working' });
    await user.type(answer, 'First thought.');
    await waitFor(() => expect(mocks.save).toHaveBeenCalledTimes(1));
    expect(answer).toBeEnabled();
    await user.type(answer, ' Second thought.');
    expect(answer).toHaveValue('First thought. Second thought.');
    release();
    await waitFor(() => expect(mocks.save).toHaveBeenCalledTimes(2));
    expect(mocks.save.mock.calls[1][0]).toMatchObject({ text: 'First thought. Second thought.' });
  });

  it('shows one alert with Try again for a failed guided request, and no raw retry label', async () => {
    const user = userEvent.setup();
    currentSession!.summary.taskKind = 'guided';
    currentWorkspace.sessions = [{ ...currentSession!.summary }];
    mocks.tutor.mockImplementationOnce(async () => fail('The tutor could not answer.'));
    renderWorkbench(lesson, true);
    await user.type(await screen.findByRole('textbox', { name: 'Your working' }), 'My first step.');
    await user.click(screen.getByRole('button', { name: 'Check my reasoning' }));
    expect(await screen.findAllByRole('alert')).toHaveLength(1);
    expect(screen.getByRole('button', { name: 'Try again' })).toBeVisible();
    expect(screen.queryByRole('button', { name: /^Retry / })).not.toBeInTheDocument();
  });

  it('offers one reload after a guided save conflict', async () => {
    const user = userEvent.setup();
    currentSession!.summary.taskKind = 'guided';
    currentWorkspace.sessions = [{ ...currentSession!.summary }];
    mocks.save.mockImplementationOnce(async () => ({ ok: false, error: 'Stale.', details: { code: 'CONFLICT', message: 'Stale.' } }));
    renderWorkbench(lesson, true);
    await user.type(await screen.findByRole('textbox', { name: 'Your working' }), 'My first step.');
    expect(await screen.findAllByRole('button', { name: 'Reload the saved attempt · keep my text' })).toHaveLength(1);
  });

  it('does not call its own submit another action', async () => {
    const user = userEvent.setup();
    currentSession!.summary.taskKind = 'guided';
    currentWorkspace.sessions = [{ ...currentSession!.summary }];
    let release!: () => void;
    const submitted = mocks.submit.getMockImplementation()!;
    mocks.submit.mockImplementationOnce((request) => new Promise((resolve) => { release = () => resolve(submitted(request)); }));
    renderWorkbench(lesson, true);
    await user.type(await screen.findByRole('textbox', { name: 'Your working' }), 'My first step.');
    await user.click(screen.getByRole('button', { name: 'Submit guided response' }));
    await waitFor(() => expect(mocks.submit).toHaveBeenCalledTimes(1));
    expect(screen.queryByText('Another action is in progress.')).not.toBeInTheDocument();
    release();
  });

  it('opens exact citations in embedded guided replies and hides them without changing the response', async () => {
    const user = userEvent.setup();
    currentSession!.summary.taskKind = 'guided';
    currentSession!.tutorTurns = [{
      id: 'turn-cited',
      prompt: 'Check my work.',
      response: 'Separate the observation from the inference.',
      requestKind: 'critique',
      hintLevel: null,
      citations: [{
        number: 1,
        sourceId: 'source-1',
        versionId: 'version-1',
        quote: 'A saved excerpt.',
        support: { relation: 'quotes', anchors: [{ exact: 'A saved excerpt.', span: null }] },
      }],
      proposalIds: [],
      modelName: 'Study model',
      createdAt: Date.now(),
    }];
    currentWorkspace.sessions = [{ ...currentSession!.summary }];
    renderWorkbench(lesson, true);

    await user.click(await screen.findByRole('button', { name: /^Citation 1/ }));
    await user.click(await screen.findByRole('button', { name: 'View source' }));
    await waitFor(() => expect(mocks.openSource).toHaveBeenCalledWith(expect.objectContaining({ sourceId: 'source-1', versionId: 'version-1' })));
    const reader = await screen.findByRole('complementary', { name: 'Source reader' });
    expect(await within(reader).findByText(/The full captured source/)).toBeVisible();
    expect(mocks.sourceVersion).toHaveBeenCalledWith({ programId: 'program-1', sourceId: 'source-1', versionId: 'version-1' });
    expect(useReaderStore.getState().session).toMatchObject({ ownerKey: 'practice:attempt-1', citing: true });

    act(() => useCitationDisplayStore.setState({ visible: false }));
    expect(screen.queryByRole('button', { name: /^Citation 1/ })).not.toBeInTheDocument();
    expect(screen.queryByRole('complementary', { name: 'Source reader' })).not.toBeInTheDocument();
    expect(screen.queryByText(/The full captured source/)).not.toBeInTheDocument();
    expect(screen.getByText('Separate the observation from the inference.')).toBeVisible();

    act(() => useCitationDisplayStore.setState({ visible: true }));
    expect(screen.getByRole('button', { name: /^Citation 1/ })).toBeVisible();
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

  it('opens the attempt it is given, not the latest one', async () => {
    const earlier = baseSession();
    earlier.summary.id = 'attempt-earlier';
    earlier.summary.updatedAt = 1;
    earlier.artifact.text = 'The earlier attempt text.';
    currentSession!.summary.updatedAt = 99;
    currentSession!.artifact.text = 'The latest attempt text.';
    currentWorkspace.sessions = [{ ...currentSession!.summary }, { ...earlier.summary }];
    const latest = currentSession!;
    mocks.getSession.mockImplementation(async (id: string) => ok(id === 'attempt-earlier' ? earlier : latest));
    renderWorkbench(lesson, false, 'attempt-earlier');
    expect(await screen.findByDisplayValue('The earlier attempt text.')).toBeVisible();
  });

  it('works through it in its own words, not by trying the exercise', async () => {
    currentSession = null; currentWorkspace.sessions = [];
    renderWorkbench(lesson, true);
    expect(await screen.findByText(/Work through it in your own words\./)).toBeVisible();
  });

  it('has one response box and a tutor box with its own name during an attempt', async () => {
    renderWorkbench();
    await screen.findByRole('textbox', { name: 'Your response' });
    expect(screen.getAllByRole('textbox', { name: 'Your response' })).toHaveLength(1);
    expect(screen.getAllByRole('textbox', { name: 'Ask a question or request critique' })).toHaveLength(1);
  });

  it('names the help in the learner words, not the mode names', async () => {
    renderWorkbench();
    await screen.findByRole('textbox', { name: 'Your response' });
    const group = screen.getByRole('group', { name: 'Help' });
    expect(within(group).getAllByRole('button').map((button) => button.textContent)).toEqual(['Full help', 'Hints only', 'No help']);
    expect(screen.getByText(/help used is recorded/)).toBeVisible();
    expect(screen.queryByText(/Aid contract|aid is recorded|Explore|Demonstrate/)).not.toBeInTheDocument();
  });

  it('starts a new attempt with the chosen help', async () => {
    const user = userEvent.setup();
    currentSession = null;
    currentWorkspace.sessions = [];
    renderWorkbench();
    await user.click(await screen.findByRole('radio', { name: /No help/ }));
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
    expect(await screen.findByRole('button', { name: 'Try again' })).toBeVisible();
    const first = mocks.save.mock.calls[0][0];
    await user.click(screen.getByRole('button', { name: 'Try again' }));
    await waitFor(() => expect(mocks.save).toHaveBeenCalledTimes(2));
    expect(mocks.save.mock.calls[1][0]).toEqual(first);
    expect(screen.getByRole('textbox', { name: 'Your response' })).toHaveValue(first.text);
  });

  it('lets a guided answer run as long as a chat message may', async () => {
    currentSession!.summary.taskKind = 'guided';
    currentWorkspace.sessions = [{ ...currentSession!.summary }];
    renderWorkbench(lesson, true);
    expect(await screen.findByRole('textbox', { name: 'Your working' })).toHaveAttribute('maxlength', String(LEARNER_TEXT_MAX_CHARS));
  });

  it('enforces No help and opens only frozen versions after submission', async () => {
    const user = userEvent.setup();
    currentSession!.summary.mode = 'demonstrate';
    currentSession!.tutorTurns = [{ id: 'turn-earlier', prompt: 'Earlier.', response: 'An earlier reply.', requestKind: 'question', hintLevel: null, citations: [{ number: 1, sourceId: 'source-1', versionId: 'version-1', quote: 'A saved excerpt.', support: { relation: 'quotes', anchors: [{ exact: 'A saved excerpt.', span: { start: 0, end: 16 } }] } }], proposalIds: [], modelName: 'Study model', createdAt: Date.now() }];
    renderWorkbench();
    expect(await screen.findByRole('textbox', { name: 'Your response' })).toBeVisible();
    expect(screen.getByRole('button', { name: /^Field notes/ })).toBeDisabled();
    expect(screen.getAllByText('Sources are closed with No help.')).toHaveLength(1);
    await user.click(screen.getByRole('button', { name: /^Citation 1/ }));
    expect(await screen.findAllByText('Sources are closed with No help.')).toHaveLength(2);
    expect(screen.queryByRole('button', { name: 'View source' })).not.toBeInTheDocument();
    const thread = screen.getByRole('region', { name: 'Tutor conversation' });
    expect(within(thread).getByRole('button', { name: 'Send' })).toBeDisabled();
    expect(screen.getByText('The tutor is closed with No help.')).toBeVisible();
    expect(screen.queryByRole('button', { name: /Review reveal choice/ })).not.toBeInTheDocument();
    await user.type(screen.getByRole('textbox', { name: 'Your response' }), 'My independent response.');
    await user.click(screen.getByRole('button', { name: 'Submit response' }));
    expect(await screen.findByText(/Evidence interpretation is uncertain/)).toBeVisible();
    await user.click(screen.getByRole('button', { name: /^Field notes/ }));
    const reader = await screen.findByRole('complementary', { name: 'Source reader' });
    expect(await within(reader).findByText(/The full captured source/)).toBeVisible();
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
    const thread = screen.getByRole('region', { name: 'Tutor conversation' });
    await user.type(within(thread).getByRole('textbox'), 'Can you explain this source?');
    await user.click(within(thread).getByRole('button', { name: 'Send' }));
    expect(await screen.findByText('Start by separating the observation from your interpretation.')).toBeVisible();
    let finishOpening!: () => void;
    const recorded = mocks.openSource.getMockImplementation()!;
    mocks.openSource.mockImplementationOnce(() => new Promise((resolve) => { finishOpening = () => resolve(recorded()); }));
    await user.click(within(thread).getByRole('button', { name: /^Citation 1/ }));
    await user.click(await screen.findByRole('button', { name: 'View source' }));
    // The opening is recorded on the server before the reader shows anything, and nothing else runs meanwhile.
    expect(await screen.findByText('Another workbench action is in progress.')).toBeVisible();
    expect(screen.queryByRole('complementary', { name: 'Source reader' })).not.toBeInTheDocument();
    await act(async () => finishOpening());
    await waitFor(() => expect(mocks.openSource).toHaveBeenCalledWith(expect.objectContaining({ sourceId: 'source-1', versionId: 'version-1' })));
    const reader = await screen.findByRole('complementary', { name: 'Source reader' });
    expect(await within(reader).findByText(/The full captured source/)).toBeVisible();
    expect(mocks.sourceVersion).toHaveBeenCalledWith({ programId: 'program-1', sourceId: 'source-1', versionId: 'version-1' });
    expect(useReaderStore.getState().session).toMatchObject({ ownerKey: 'practice:attempt-1', citing: true });
  });

  it('renders generated practice content and saved sources as structured rich text', async () => {
    const user = userEvent.setup();
    currentSession!.taskPrompt = '## Compare evidence\n\n- Record the observation\n- Separate the inference';
    currentSession!.tutorTurns = [{ id: 'turn-rich', prompt: 'What should I inspect?', response: '### Next step\n\nUse the `saved source` before drawing a conclusion.', requestKind: 'question', hintLevel: null, citations: [], proposalIds: [], modelName: 'Study model', createdAt: Date.now() }];
    currentSession!.revealedSolution = '### Worked steps\n\n1. State the observation.\n2. Bound the conclusion.';
    mocks.sourceVersion.mockResolvedValue(ok({
      sourceId: 'source-1',
      version: { ...savedVersion, extractionVersion: 'web_reference_v1', wordCount: 12 },
      fullText: '# Field notes\n\n| Claim | Status\n\n| Pattern repeats | Supported',
      usage: [],
    }));

    renderWorkbench();
    expect(await screen.findByRole('heading', { name: 'Compare evidence' })).toBeVisible();
    expect(screen.getByRole('heading', { name: 'Next step' })).toBeVisible();
    const workedSteps = screen.getByRole('heading', { name: 'Worked steps' });
    expect(workedSteps).toBeVisible();
    expect(workedSteps.closest('.tiptap-viewer')?.querySelectorAll('ol li')).toHaveLength(2);

    await user.click(screen.getByRole('button', { name: /^Field notes/ }));
    const reader = await screen.findByRole('complementary', { name: 'Source reader' });
    expect(await within(reader).findByText(/Pattern repeats/)).toBeVisible();
    // The docked reader is given the saved version's formatted text, and draws its table as one.
    expect(useReaderStore.getState().session?.citations[0]?.webSnapshot?.text).toMatch(/Claim[\s\S]*Pattern repeats[\s\S]*Supported/);
    const table = within(reader).getByRole('table');
    expect(within(table).getAllByRole('columnheader').map((cell) => cell.textContent)).toEqual(['Claim', 'Status']);
    expect(within(table).getAllByRole('cell').map((cell) => cell.textContent)).toEqual(['Pattern repeats', 'Supported']);
    expect(within(reader).queryByText(/\|/)).not.toBeInTheDocument();
  });

  it('records a source opened from the attempt rail and keeps it open when citations are hidden', async () => {
    const user = userEvent.setup();
    renderWorkbench();
    await user.click(await screen.findByRole('button', { name: /^Field notes/ }));
    await waitFor(() => expect(mocks.openSource).toHaveBeenCalledWith(expect.objectContaining({ sourceId: 'source-1', versionId: 'version-1' })));
    const reader = await screen.findByRole('complementary', { name: 'Source reader' });
    expect(await within(reader).findByText(/The full captured source/)).toBeVisible();
    expect(mocks.sourceVersion).toHaveBeenCalledWith({ programId: 'program-1', sourceId: 'source-1', versionId: 'version-1' });
    expect(useReaderStore.getState().session).toMatchObject({ ownerKey: 'practice:attempt-1', citing: false });

    act(() => useCitationDisplayStore.setState({ visible: false }));
    expect(screen.getByRole('complementary', { name: 'Source reader' })).toBeVisible();
    expect(useReaderStore.getState().session).not.toBeNull();
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

  it('turns an accepted misconception into a recall card through the shared review card', async () => {
    const user = userEvent.setup();
    currentSession!.proposals[0] = { ...currentSession!.proposals[0], kind: 'misconception', text: 'An observation already proves the inference.' };
    renderWorkbench();
    await screen.findByRole('textbox', { name: 'Your response' });
    expect(screen.queryByTestId('review-card')).not.toBeInTheDocument();
    const proposals = screen.getByText('Tutor proposals').parentElement!;
    await user.click(within(proposals).getByRole('button', { name: 'Accept idea' }));
    expect(await screen.findByTestId('review-card')).toBeVisible();
    expect(reviewCard).toHaveBeenCalledWith(expect.objectContaining({
      programId: 'program-1',
      front: 'What is wrong with this idea, and what is right instead? “An observation already proves the inference.”',
    }));
  });

  it('names the help levels Full help, Hints only and No help when starting an attempt', async () => {
    currentSession = null;
    currentWorkspace.sessions = [];
    renderWorkbench();
    expect(await screen.findByRole('radio', { name: /Full help/ })).toBeVisible();
    expect(screen.getByRole('radio', { name: /Hints only/ })).toBeVisible();
    expect(screen.getByRole('radio', { name: /No help/ })).toBeVisible();
  });

  it('offers a conflict reload while retaining locally edited response text', async () => {
    const user = userEvent.setup();
    mocks.save.mockImplementationOnce(async () => ({ ok: false as const, error: 'Revision changed; reload and retry.', details: { code: 'CONFLICT', message: 'Revision changed; reload and retry.' } }));
    renderWorkbench();
    const editor = await screen.findByRole('textbox', { name: 'Your response' });
    await user.type(editor, 'Keep this draft after reload.');
    expect(await screen.findByRole('button', { name: 'Reload the saved attempt · keep my text' })).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Reload the saved attempt · keep my text' }));
    expect(screen.getByRole('textbox', { name: 'Your response' })).toHaveValue('Keep this draft after reload.');
  });
});
