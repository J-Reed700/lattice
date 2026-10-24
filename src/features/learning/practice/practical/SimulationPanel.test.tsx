import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { citationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import type { LearningPracticalActivityDto, LearningSimulationSessionDto } from '@/lib/bindings';

import { SimulationPanel } from './SimulationPanel';

const api = vi.hoisted(() => ({ startLearningSimulation: vi.fn(), sendLearningSimulationTurn: vi.fn(), finishLearningSimulation: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: api }));
vi.mock('@/components/TiptapEditor', () => ({ TiptapViewer: ({ content }: { content: string }) => <div>{content}</div> }));

const activity = { id: 'activity-1', revision: 2 } as LearningPracticalActivityDto;
const session = (overrides: Partial<LearningSimulationSessionDto> = {}): LearningSimulationSessionDto => ({
  id: 'sim-1', programId: 'course-1', activityId: 'activity-1', activityRevision: 2, practiceSessionId: null,
  learnerRole: 'Museum guide', counterpartRole: 'Visitor', status: 'active', revision: 3, turns: [],
  createdAt: 1, updatedAt: 1, submittedAt: null, ...overrides,
} as LearningSimulationSessionDto);

function renderPanel(current: LearningSimulationSessionDto | null) {
  const onSession = vi.fn();
  const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  const view = render(<SimulationPanel programId="course-1" activity={activity} session={current} onSession={onSession} />, { wrapper });
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
    fireEvent.click(screen.getByRole('button', { name: 'Retry same operation' }));
    await waitFor(() => expect(onSession).toHaveBeenCalled());
    const [first, second] = api.startLearningSimulation.mock.calls.map(([request]) => request);
    expect(second.operationId).toBe(first.operationId);
    expect(second.sessionId).toBe(first.sessionId);
  });

  it('starts a new operation when the roles change after a failure', async () => {
    api.startLearningSimulation.mockRejectedValueOnce('odd').mockResolvedValue({ ok: true, data: session() });
    renderPanel(null);
    fireEvent.click(screen.getByRole('button', { name: /Start simulation/ }));
    expect(await screen.findByRole('alert')).toHaveTextContent('The request could not be completed.');
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
    expect(turns.map((turn) => turn.querySelector('div')?.textContent)).toEqual(['Museum guide', 'Visitor', 'Coach']);
    expect(screen.getByText('“Paint has worn away.”')).toBeInTheDocument();
    act(() => citationDisplayStore.getState().setVisible(false));
    expect(screen.queryByText('“Paint has worn away.”')).not.toBeInTheDocument();
  });

  it('sends a reply with Cmd+Enter, clears it, and keeps it after a failure', async () => {
    api.sendLearningSimulationTurn.mockResolvedValueOnce({ ok: false, error: 'conflict' }).mockResolvedValue({ ok: true, data: session({ revision: 4 }) });
    const { onSession } = renderPanel(session());
    expect(screen.getByText('Your first response will begin the conversation.')).toBeInTheDocument();
    const field = screen.getByLabelText('Your response');
    expect(screen.getByRole('button', { name: 'Send simulation turn' })).toBeDisabled();
    fireEvent.change(field, { target: { value: '  The paint has faded.  ' } });
    fireEvent.keyDown(field, { key: 'Enter' });
    expect(api.sendLearningSimulationTurn).not.toHaveBeenCalled();
    fireEvent.keyDown(field, { key: 'Enter', metaKey: true });
    expect(await screen.findByRole('alert')).toHaveTextContent('conflict');
    expect(field).toHaveValue('  The paint has faded.  ');
    fireEvent.click(screen.getByRole('button', { name: 'Retry same operation' }));
    await waitFor(() => expect(onSession).toHaveBeenCalledWith(session({ revision: 4 })));
    const [first, second] = api.sendLearningSimulationTurn.mock.calls.map(([request]) => request);
    expect(second).toEqual(first);
    expect(first).toMatchObject({ sessionId: 'sim-1', expectedRevision: 3, content: 'The paint has faded.' });
    expect(screen.getByLabelText('Your response')).toHaveValue('');
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('finishes a simulation, retrying the same request after a failure', async () => {
    api.finishLearningSimulation.mockResolvedValueOnce({ ok: false, error: 'grader offline' }).mockResolvedValue({ ok: true, data: session({ status: 'completed' as never }) });
    const { onSession } = renderPanel(session());
    fireEvent.click(screen.getByRole('button', { name: 'Finish simulation' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('grader offline');
    fireEvent.click(screen.getByRole('button', { name: 'Retry same operation' }));
    await waitFor(() => expect(onSession).toHaveBeenCalled());
    const [first, second] = api.finishLearningSimulation.mock.calls.map(([request]) => request);
    expect(second.operationId).toBe(first.operationId);
  });

  it('offers no reply or finish once the simulation is over', () => {
    renderPanel(session({ status: 'completed' as never }));
    expect(screen.getByText(/· completed/)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Finish simulation' })).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Your response')).not.toBeInTheDocument();
  });
});
