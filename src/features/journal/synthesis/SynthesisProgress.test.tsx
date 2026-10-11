import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, useLocation } from 'react-router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { JOB_STATUS_EVENT, type JobDto } from '@/features/jobs/api';
import { VaultAPI } from '@/lib/api';
import type { JournalSynthesisDto } from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import { jobFixture } from '@/tests/fixtures/jobs';

import { synthesisKeys } from './api';
import { useSynthesisPanel } from './synthesisPanel';
import { SynthesisProgress } from './SynthesisProgress';

const mocks = vi.hoisted(() => ({ listeners: new Map<string, Set<(event: { payload: unknown }) => void>>() }));
vi.mock('@/lib/api', () => {
  const api = { quickCapture: vi.fn(), listWorkspaceNotes: vi.fn(), updateWorkspaceNote: vi.fn() };
  return { VaultAPI: api, default: api };
});
vi.mock('@/shared/ipc/transport', () => ({ apiCall: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((name: string, handler: (event: { payload: unknown }) => void) => {
    const handlers = mocks.listeners.get(name) ?? new Set();
    handlers.add(handler);
    mocks.listeners.set(name, handlers);
    return Promise.resolve(() => handlers.delete(handler));
  }),
}));

const START = 1_000_000;
const result = { synthesis: 'What the threads decided.', entryCount: 3, chunkCount: 2, scope: 'deck', conversationIds: ['chat'], citations: [], sources: [] };

function synthesis(job: Partial<JobDto> = {}, overrides: Partial<JournalSynthesisDto> = {}): JournalSynthesisDto {
  return {
    job: jobFixture({ id: 'synthesis', kind: 'journal.synthesis', subjectId: 'capture', status: 'running', progressTotal: 100, createdAt: START, startedAt: START, ...job }),
    title: 'Research discussion', heading: 'Research discussion', destination: { kind: 'capture' }, conversationIds: ['chat'],
    activity: { stage: 'reading', entryCount: 3, chunkIndex: 1, chunkCount: 2 },
    ...overrides,
  };
}

function Location() {
  const location = useLocation();
  return <span data-testid="location">{location.pathname}{location.search}</span>;
}

let client: QueryClient;
function mount(syntheses: JournalSynthesisDto[]) {
  client.setQueryData(synthesisKeys.list, syntheses);
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  return render(<MemoryRouter initialEntries={['/chat']}><Location /><SynthesisProgress /></MemoryRouter>, { wrapper });
}

function publish(job: JobDto) {
  act(() => {
    for (const handler of mocks.listeners.get(JOB_STATUS_EVENT) ?? []) handler({ payload: job });
  });
}

const calls = (command: string) => vi.mocked(apiCall).mock.calls.filter(([name]) => name === command);

describe('synthesis progress', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.listeners.clear();
    client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    useSynthesisPanel.setState({ minimized: false, autoSave: [], saving: {}, saved: null });
    vi.mocked(VaultAPI.quickCapture).mockResolvedValue({ ok: true, data: { noteId: 'note 1', noteTitle: 'Research notes', created: true } });
    vi.mocked(apiCall).mockImplementation(async (command: string) => {
      switch (command) {
        case 'get_journal_synthesis_result': return { ok: true, data: result };
        case 'mark_journal_synthesis_applied': return { ok: true, data: true };
        case 'retry_journal_synthesis': return { ok: true, data: synthesis({ id: 'retry', retryOfJobId: 'synthesis', status: 'pending', progressMessage: 'Retry queued' }) };
        case 'dismiss_journal_synthesis': return { ok: true, data: null };
        default: return { ok: true, data: [] };
      }
    });
  });
  afterEach(() => vi.useRealTimers());

  it("follows the job's progress and keeps the timer out of live announcements", async () => {
    vi.useFakeTimers({ now: START, toFake: ['Date', 'setInterval', 'clearInterval'] });
    mount([synthesis()]);
    expect(screen.getByText('Reviewing part 1 of 2')).toBeInTheDocument();
    act(() => vi.advanceTimersByTime(65_000));
    expect(screen.getByText('1m 05s elapsed · 3 entries')).toBeInTheDocument();
    expect(screen.getByText(/Still working/)).toBeInTheDocument();
    expect(screen.getByRole('status')).toHaveTextContent('Reviewing part 1 of 2 · Research discussion');
    expect(screen.getByRole('status')).not.toHaveTextContent('1m');
    await waitFor(() => expect(mocks.listeners.get(JOB_STATUS_EVENT)?.size).toBe(1));
    publish({ ...synthesis().job, activity: { stage: 'writing', entryCount: 3, chunkIndex: null, chunkCount: 2 } });
    await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('Writing your synthesis'));
  });

  it('saves a synthesis started here as soon as its job finishes', async () => {
    useSynthesisPanel.setState({ autoSave: ['synthesis'] });
    mount([synthesis()]);
    await waitFor(() => expect(mocks.listeners.get(JOB_STATUS_EVENT)?.size).toBe(1));

    publish({ ...synthesis().job, status: 'completed', resultRef: 'capture', finishedAt: START + 5_000 });

    await waitFor(() => expect(screen.getByText('Research notes')).toBeInTheDocument());
    expect(VaultAPI.quickCapture).toHaveBeenCalledOnce();
    expect(calls('mark_journal_synthesis_applied')).toEqual([['mark_journal_synthesis_applied', { jobId: 'synthesis' }]]);
    fireEvent.click(screen.getByRole('button', { name: 'Open journal page' }));
    expect(screen.getByTestId('location')).toHaveTextContent('/journals?noteId=note+1');
    expect(screen.queryByRole('complementary', { name: 'Conversation synthesis' })).not.toBeInTheDocument();
  });

  it('shows a synthesis that finished while the app was closed and saves it when asked', async () => {
    mount([synthesis({ status: 'completed', resultRef: 'capture', finishedAt: START + 5_000 })]);
    expect(screen.getByRole('heading', { name: 'Synthesis ready' })).toBeInTheDocument();
    expect(VaultAPI.quickCapture).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: 'Save to Journal' }));

    await waitFor(() => expect(screen.getByText('Research notes')).toBeInTheDocument());
    expect(VaultAPI.quickCapture).toHaveBeenCalledOnce();
    expect(calls('mark_journal_synthesis_applied')).toHaveLength(1);
    expect(client.getQueryData(synthesisKeys.list)).toEqual([]);
  });

  it('can be minimized and shown again', () => {
    mount([synthesis()]);
    fireEvent.click(screen.getByRole('button', { name: 'Keep working' }));
    expect(screen.getByRole('button', { name: /Show synthesis progress/ })).toHaveFocus();
    fireEvent.click(screen.getByRole('button', { name: /Show synthesis progress/ }));
    expect(screen.getByRole('button', { name: 'Minimize synthesis progress' })).toHaveFocus();
  });

  it('explains a save failure and offers to save again', async () => {
    vi.mocked(VaultAPI.quickCapture).mockResolvedValueOnce({ ok: false, error: 'Disk full' });
    mount([synthesis({ status: 'completed', resultRef: 'capture', finishedAt: START + 5_000 })]);
    fireEvent.click(screen.getByRole('button', { name: 'Save to Journal' }));
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Disk full'));
    expect(screen.getByRole('alert')).toHaveTextContent('kept here while you retry saving');
    fireEvent.click(screen.getByRole('button', { name: 'Retry saving' }));
    await waitFor(() => expect(screen.getByText('Research notes')).toBeInTheDocument());
  });

  it('explains a failed synthesis and offers a new attempt or dismissal', async () => {
    mount([synthesis({ status: 'failed', error: 'Model unavailable', finishedAt: START + 5_000 })]);
    expect(screen.getByRole('alert')).toHaveTextContent('Model unavailable');
    fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    await waitFor(() => expect(calls('retry_journal_synthesis')).toEqual([['retry_journal_synthesis', { jobId: 'synthesis' }]]));
    await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('Retry queued'));
    expect(useSynthesisPanel.getState().autoSave).toEqual(['retry']);
  });
});
