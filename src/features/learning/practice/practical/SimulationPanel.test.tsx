import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { LEARNER_TEXT_MAX_CHARS } from '@/features/learning/model/learnerTextLimit';
import { citationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import type { LearningPracticalActivityDto, LearningSimulationSessionDto } from '@/lib/bindings';

import { questionLabel } from './practicalWorkbenchHelpers';
import { SimulationPanel } from './SimulationPanel';

const api = vi.hoisted(() => ({ startLearningSimulation: vi.fn(), sendLearningSimulationTurn: vi.fn(), finishLearningSimulation: vi.fn(), getLearningSourceVersion: vi.fn() }));
vi.mock('@/lib/api', async (importOriginal) => ({ ...(await importOriginal<typeof import('@/lib/api')>()), default: api }));
vi.mock('@/features/voice/hooks/useVoiceConversation', () => ({ useVoiceConversation: () => ({ phase: 'off', toggle: vi.fn() }) }));
vi.mock('@/features/speech/hooks/useSpeech', () => ({ useSpeaker: () => vi.fn() }));
vi.mock('@/components/TiptapEditor', () => ({ TiptapViewer: ({ content }: { content: string }) => <div>{content}</div> }));

const activity = { id: 'activity-1', revision: 2 } as LearningPracticalActivityDto;
const session = (overrides: Partial<LearningSimulationSessionDto> = {}): LearningSimulationSessionDto => ({
  id: 'sim-1', programId: 'course-1', activityId: 'activity-1', activityRevision: 2, practiceSessionId: null,
  learnerRole: 'Museum guide', counterpartRole: 'Visitor', status: 'active', revision: 3, turns: [],
  createdAt: 1, updatedAt: 1, submittedAt: null, ...overrides,
} as LearningSimulationSessionDto);

function renderPanel(current: LearningSimulationSessionDto | null, currentActivity: LearningPracticalActivityDto = activity) {
  const onSession = vi.fn();
  const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  const view = render(<SimulationPanel programId="course-1" activity={currentActivity} session={current} onSession={onSession} />, { wrapper });
  return { ...view, onSession };
}

beforeEach(() => {
  for (const fn of Object.values(api)) fn.mockReset();
  act(() => citationDisplayStore.getState().setVisible(true));
});

describe('SimulationPanel', () => {
  it('starts a simulation with the roles given', async () => {
    api.startLearningSimulation.mockResolvedValue({ ok: true, data: session() });
    const { onSession } = renderPanel(null);
    expect(screen.getByText(/activity revision 2/)).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText('Your role'), { target: { value: ' Museum guide ' } });
    fireEvent.change(screen.getByLabelText('Counterpart role'), { target: { value: 'Visitor' } });
    fireEvent.click(screen.getByRole('button', { name: /Start simulation/ }));
    await waitFor(() => expect(onSession).toHaveBeenCalledWith(session()));
    expect(onSession).toHaveBeenCalledTimes(1);
    expect(api.startLearningSimulation).toHaveBeenCalledWith(expect.objectContaining({
      programId: 'course-1', activityId: 'activity-1', expectedActivityRevision: 2, learnerRole: 'Museum guide', counterpartRole: 'Visitor',
    }));
  });

  it('needs both roles to start', () => {
    renderPanel(null);
    fireEvent.change(screen.getByLabelText('Counterpart role'), { target: { value: '  ' } });
    expect(screen.getByRole('button', { name: /Start simulation/ })).toBeDisabled();
  });

  it('retries a failed start with the same operation', async () => {
    api.startLearningSimulation.mockResolvedValueOnce({ ok: false, error: 'model busy' }).mockResolvedValueOnce({ ok: true, data: session() });
    const { onSession } = renderPanel(null);
    fireEvent.click(screen.getByRole('button', { name: /Start simulation/ }));
    expect(await screen.findByRole('alert')).toHaveTextContent('model busy');
    fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    await waitFor(() => expect(onSession).toHaveBeenCalled());
    const [first, second] = api.startLearningSimulation.mock.calls.map(([request]) => request);
    expect(second.operationId).toBe(first.operationId);
    expect(second.sessionId).toBe(first.sessionId);
  });

  it('starts a new operation when the roles change after a failure', async () => {
    api.startLearningSimulation.mockRejectedValueOnce('odd').mockResolvedValue({ ok: true, data: session() });
    renderPanel(null);
    fireEvent.click(screen.getByRole('button', { name: /Start simulation/ }));
    expect(await screen.findByRole('alert')).toHaveTextContent('odd');
    fireEvent.change(screen.getByLabelText('Your role'), { target: { value: 'Curator' } });
    fireEvent.click(screen.getByRole('button', { name: /Start simulation/ }));
    await waitFor(() => expect(api.startLearningSimulation).toHaveBeenCalledTimes(2));
    expect(api.startLearningSimulation.mock.calls[1][0].learnerRole).toBe('Curator');
  });

  it('shows the turns with each speaker and their citations', () => {
    renderPanel(session({
      turns: [
        { id: 't1', speaker: 'learner', content: 'Welcome to the Roman gallery.', citations: [] },
        { id: 't2', speaker: 'counterpart', content: 'Why are the statues white?', citations: [{ sourceId: 's1', quote: 'Paint has worn away.' }] },
        { id: 't3', speaker: 'coach', content: 'Mention the original pigments.', citations: [] },
      ] as never,
    }));
    const turns = screen.getAllByRole('article');
    expect(turns.map((turn) => turn.querySelector('p')?.textContent)).toEqual(['Museum guide', 'Visitor', 'Coach']);
    expect(screen.getByText('“Paint has worn away.”')).toBeInTheDocument();
    act(() => citationDisplayStore.getState().setVisible(false));
    expect(screen.queryByText('“Paint has worn away.”')).not.toBeInTheDocument();
  });

  it('opens the exact cited source after an interview without fetching its live page', async () => {
    api.getLearningSourceVersion.mockResolvedValue({ ok: true, data: {
      sourceId: 'spec', fullText: 'The captured specification explains repeated effects.', usage: [],
      version: { id: 'spec-v2', versionNumber: 2, title: 'Retry semantics', publisher: 'Specification publisher', resolvedUrl: 'https://spec.example/retries', extractionVersion: 'test', wordCount: 8 },
    } });
    renderPanel(session({ status: 'submitted', turns: [{ id: 'feedback', speaker: 'coach', content: 'Compare the intended effects.', citations: [{ sourceId: 'spec', versionId: 'spec-v2', quote: 'repeated effects' }] }] as never }), { ...activity, kind: 'interview', practiceMode: 'demonstrate' });
    expect(api.getLearningSourceVersion).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: 'View saved source' }));
    expect(await screen.findByText('Retry semantics')).toBeInTheDocument();
    expect(api.getLearningSourceVersion).toHaveBeenCalledWith({ programId: 'course-1', sourceId: 'spec', versionId: 'spec-v2' });
    expect(screen.getByText('https://spec.example/retries')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Page' })).not.toBeInTheDocument();
  });

  it('sends a reply with Cmd+Enter, clears it, and keeps it after a failure', async () => {
    api.sendLearningSimulationTurn.mockResolvedValueOnce({ ok: false, error: 'conflict' }).mockResolvedValue({ ok: true, data: session({ revision: 4 }) });
    const { onSession } = renderPanel(session());
    expect(screen.getByText('Your first response will begin the conversation.')).toBeInTheDocument();
    const field = screen.getByLabelText('Your response');
    expect(screen.getByRole('button', { name: 'Send' })).toBeDisabled();
    fireEvent.change(field, { target: { value: '  The paint has faded.  ' } });
    fireEvent.keyDown(field, { key: 'Enter' });
    expect(api.sendLearningSimulationTurn).not.toHaveBeenCalled();
    fireEvent.keyDown(field, { key: 'Enter', metaKey: true });
    expect(await screen.findByRole('alert')).toHaveTextContent('conflict');
    expect(field).toHaveValue('  The paint has faded.  ');
    fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    await waitFor(() => expect(onSession).toHaveBeenCalledWith(session({ revision: 4 })));
    const [first, second] = api.sendLearningSimulationTurn.mock.calls.map(([request]) => request);
    expect(second).toEqual(first);
    expect(first).toMatchObject({ sessionId: 'sim-1', expectedRevision: 3, content: 'The paint has faded.' });
    expect(screen.getByLabelText('Your response')).toHaveValue('');
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('lets a response run as long as a chat message may', () => {
    renderPanel(session());
    expect(screen.getByLabelText('Your response')).toHaveAttribute('maxlength', String(LEARNER_TEXT_MAX_CHARS));
  });

  it('finishes a simulation, retrying the same request after a failure', async () => {
    api.finishLearningSimulation.mockResolvedValueOnce({ ok: false, error: 'grader offline' }).mockResolvedValue({ ok: true, data: session({ status: 'completed' as never }) });
    const { onSession } = renderPanel(session({ turns: [{ id: 'answer', speaker: 'learner', content: 'My answer', citations: [] }] as never }));
    fireEvent.click(screen.getByRole('button', { name: 'Finish simulation' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('grader offline');
    fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    await waitFor(() => expect(onSession).toHaveBeenCalled());
    const [first, second] = api.finishLearningSimulation.mock.calls.map(([request]) => request);
    expect(second.operationId).toBe(first.operationId);
  });

  it('brings the newest turn into view inside the conversation log', () => {
    const turn = (id: string, speaker: string) => ({ id, speaker, content: `Turn ${id}`, citations: [] });
    const rects = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (this: HTMLElement) {
      const top = this.getAttribute('role') === 'log' ? 100 : this.textContent?.includes('Turn 3') ? 700 : 0;
      return { top } as DOMRect;
    });
    const view = renderPanel(session({ turns: [turn('1', 'counterpart'), turn('2', 'learner')] as never }));
    const log = screen.getByRole('log', { name: 'Simulation turns' });
    log.scrollTop = 0;
    view.rerender(<SimulationPanel programId="course-1" activity={activity} session={session({ revision: 4, turns: [turn('1', 'counterpart'), turn('2', 'learner'), turn('3', 'counterpart')] as never })} onSession={view.onSession} />);
    expect(log.scrollTop).toBe(588);
    rects.mockRestore();
  });

  it('labels quick prompts with the question’s words, not its markdown', () => {
    expect(questionLabel('Assume this trace:\n```text\nretry=42\n```\nHow would you prevent duplicates?', 80))
      .toBe('Assume this trace: How would you prevent duplicates?');
    expect(questionLabel('Explain observability.\n![Untrusted image](https://example.test/p.png)')).toBe('Explain observability.');
    expect(questionLabel('How would you design reliable request processing, part 1?')).toBe('How would you design reliable…');
  });

  it('offers no reply or finish once the simulation is over', () => {
    renderPanel(session({ status: 'submitted' }));
    expect(screen.getByText(/· finished/)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Finish simulation' })).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Your response')).not.toBeInTheDocument();
  });

  it('pre-fills learner and counterpart roles from activity interviewPlan', () => {
    const interviewActivity: LearningPracticalActivityDto = {
      ...activity,
      interviewPlan: {
        companyName: 'Stripe',
        targetRole: 'Staff Infrastructure Engineer',
        roundKind: 'system_design',
        companySummary: 'Summary of Stripe',
        learnerRole: 'Staff Candidate',
        counterpartRole: 'Principal Architect',
        researchedSourceVersionIds: ['v1'],
        questions: [],
      },
    };
    renderPanel(null, interviewActivity);
    expect(screen.getByLabelText('Your role')).toHaveValue('Staff Candidate');
    expect(screen.getByLabelText('Counterpart role')).toHaveValue('Principal Architect');
  });

  it('renders quick question prompt chips and populates the message draft', () => {
    const interviewActivity: LearningPracticalActivityDto = {
      ...activity,
      interviewPlan: {
        companyName: 'Stripe',
        targetRole: 'Staff Engineer',
        roundKind: 'system_design',
        companySummary: 'Summary of Stripe',
        learnerRole: 'Candidate',
        counterpartRole: 'Interviewer',
        questions: [
          {
            id: 'q1',
            round: 'system_design',
            question: 'How would you design an idempotency key store?',
            companyRationale: 'Core to Stripe APIs',
            followUpProbes: ['Cache invalidation?'],
            evaluationSignals: ['Understands distributed locks'],
          },
        ],
      },
    };
    renderPanel(session(), interviewActivity);
    const chip = screen.getByRole('button', { name: /How would you design/i });
    expect(chip).toBeInTheDocument();
    fireEvent.click(chip);
    expect(screen.getByLabelText('Your response')).toHaveValue("Please ask me this practice question: How would you design an idempotency key store?");
  });

  it('prevents duplicate keyboard sends and finishing while a reply is pending', async () => {
    let release!: (value: unknown) => void;
    api.sendLearningSimulationTurn.mockImplementation(() => new Promise((resolve) => { release = resolve; }));
    const { onSession } = renderPanel(session({ turns: [{ id: 'answer', speaker: 'learner', content: 'Prior answer', citations: [] }] as never }));
    const field = screen.getByLabelText('Your response');
    fireEvent.change(field, { target: { value: 'Here is my reasoning.' } });
    fireEvent.keyDown(field, { key: 'Enter', metaKey: true });
    fireEvent.keyDown(field, { key: 'Enter', metaKey: true });
    await waitFor(() => expect(api.sendLearningSimulationTurn).toHaveBeenCalledTimes(1));
    expect(screen.getByRole('button', { name: 'Finish simulation' })).toBeDisabled();
    expect(field).toHaveAttribute('readonly');
    expect(screen.getByRole('status')).toHaveTextContent('responding');
    await act(async () => release({ ok: true, data: session({ revision: 4 }) }));
    await waitFor(() => expect(onSession).toHaveBeenCalledTimes(1));
  });

  it('stops the answer timer while the interviewer replies and drops it while finishing', async () => {
    let release!: (value: unknown) => void;
    api.sendLearningSimulationTurn.mockImplementation(() => new Promise((resolve) => { release = resolve; }));
    api.finishLearningSimulation.mockImplementation(() => new Promise(() => {}));
    renderPanel(session({ turns: [
      { id: 'q1', speaker: 'counterpart', content: 'Tell me about a hard bug.', citations: [] },
      { id: 'a1', speaker: 'learner', content: 'A race in the job queue.', citations: [] },
      { id: 'q2', speaker: 'counterpart', content: 'How did you find it?', citations: [] },
    ] as never }), { ...activity, kind: 'interview' });
    expect(screen.getByText(/Answering now/)).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText('Your response'), { target: { value: 'Logs, then a repro.' } });
    fireEvent.click(screen.getByRole('button', { name: 'Send' }));
    expect(await screen.findByText(/Answer sent/)).toBeInTheDocument();
    await act(async () => release({ ok: false, error: 'response lost' }));
    expect(await screen.findByText(/Answering now/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Finish interview' }));
    await waitFor(() => expect(api.finishLearningSimulation).toHaveBeenCalledTimes(1));
    expect(screen.queryByText(/Answering now|Answer sent/)).not.toBeInTheDocument();
  });

  it('requires a learner answer before finishing and uses interview defaults', () => {
    const view = renderPanel(null, { ...activity, kind: 'interview' });
    expect(screen.getByLabelText('Your role')).toHaveValue('Candidate');
    expect(screen.getByLabelText('Counterpart role')).toHaveValue('Interviewer');
    view.rerender(<SimulationPanel programId="course-1" activity={activity} session={session()} onSession={view.onSession} />);
    expect(screen.getByRole('button', { name: 'Finish simulation' })).toBeDisabled();
  });

  it('offers a reload of the simulation, not Try again, when the backend reports a conflict', async () => {
    api.sendLearningSimulationTurn.mockResolvedValueOnce({ ok: false, error: 'Another window changed this.', details: { code: 'CONFLICT' } });
    renderPanel(session());
    fireEvent.change(screen.getByLabelText('Your response'), { target: { value: 'My answer.' } });
    fireEvent.click(screen.getByRole('button', { name: 'Send' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Another window changed this.');
    expect(screen.queryByRole('button', { name: 'Try again' })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Reload simulation' }));
    await waitFor(() => expect(screen.queryByRole('alert')).not.toBeInTheDocument());
    expect(screen.getByLabelText('Your response')).toHaveValue('My answer.');
  });

  it('treats a message that merely says "conflict" as a plain error with Try again', async () => {
    api.sendLearningSimulationTurn.mockResolvedValueOnce({ ok: false, error: 'Changed elsewhere (conflict).' });
    renderPanel(session());
    fireEvent.change(screen.getByLabelText('Your response'), { target: { value: 'My answer.' } });
    fireEvent.click(screen.getByRole('button', { name: 'Send' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Changed elsewhere');
    expect(screen.getByRole('button', { name: 'Try again' })).toBeVisible();
    expect(screen.queryByRole('button', { name: 'Reload simulation' })).not.toBeInTheDocument();
  });

  it('retries the failed turn exactly and preserves a newer draft', async () => {
    api.sendLearningSimulationTurn.mockResolvedValueOnce({ ok: false, error: 'response lost' })
      .mockResolvedValueOnce({ ok: true, data: session({ revision: 4 }) });
    renderPanel(session());
    const field = screen.getByLabelText('Your response');
    fireEvent.change(field, { target: { value: 'The original answer.' } });
    fireEvent.click(screen.getByRole('button', { name: 'Send' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('response lost');
    fireEvent.change(field, { target: { value: 'A newer draft I want to keep.' } });
    fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    await waitFor(() => expect(api.sendLearningSimulationTurn).toHaveBeenCalledTimes(2));
    expect(api.sendLearningSimulationTurn.mock.calls[1][0]).toEqual(api.sendLearningSimulationTurn.mock.calls[0][0]);
    await waitFor(() => expect(screen.queryByRole('alert')).not.toBeInTheDocument());
    expect(field).toHaveValue('A newer draft I want to keep.');
  });

  it('withholds question selection during unassisted demonstration', () => {
    const interviewActivity = {
      ...activity, practiceMode: 'demonstrate', interviewPlan: {
        companyName: 'Acme', targetRole: 'Engineer', roundKind: 'full_loop', companySummary: 'Private preparation',
        learnerRole: 'Candidate', counterpartRole: 'Interviewer', questions: [{
          id: 'q1', round: 'behavioral', question: 'Unseen question?', companyRationale: 'Rationale',
          followUpProbes: ['Probe'], evaluationSignals: ['Signal'],
        }],
      },
    } as LearningPracticalActivityDto;
    renderPanel(session(), interviewActivity);
    expect(screen.queryByLabelText('Tailored interview quick prompts')).not.toBeInTheDocument();
    expect(screen.queryByText('Unseen question?')).not.toBeInTheDocument();
  });
});
