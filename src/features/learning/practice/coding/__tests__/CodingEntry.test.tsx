import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { JobDto } from '@/lib/bindings';

import { codingApi } from '../codingApi';
import { CodingEntry, type CodingBuildChoice } from '../CodingEntry';
import { codingSession } from './fixtures';

vi.mock('@/lib/api', () => {
  let count = 0;
  return { newId: () => `id-${++count}`, unwrap: <T,>(value: T) => value, default: {}, VaultAPI: {} };
});

vi.mock('../codingApi', () => ({
  codingApi: { startBuild: vi.fn(), startSession: vi.fn() },
}));

const subscribers = new Set<(job: JobDto) => void>();
vi.mock('@/features/jobs/api', () => ({
  useJobStatus: (subscriber: (job: JobDto) => void) => {
    subscribers.add(subscriber);
  },
}));

const choice: CodingBuildChoice = {
  task: 'build',
  setting: 'mock',
  language: 'python',
  difficulty: 'easy',
  budgetSeconds: 1200,
};

function job(overrides: Partial<JobDto>): JobDto {
  return {
    id: 'job-1',
    kind: 'learning.practice_build',
    subjectId: 'program-1',
    status: 'running',
    progressCurrent: 1,
    progressTotal: 3,
    progressMessage: '',
    activity: null,
    resultRef: null,
    errorCode: null,
    error: null,
    retryOfJobId: null,
    retryCount: 0,
    retryNotBefore: null,
    createdAt: 1,
    startedAt: 1,
    finishedAt: null,
    ...overrides,
  };
}

function publish(next: JobDto) {
  act(() => {
    for (const subscriber of [...subscribers]) subscriber(next);
  });
}

describe('CodingEntry', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    subscribers.clear();
    vi.mocked(codingApi.startBuild).mockResolvedValue({ jobId: 'job-1' });
    vi.mocked(codingApi.startSession).mockResolvedValue(codingSession({ sessionId: 'session-9' }));
  });

  it('starts the build with the choice and names the three steps behind a progress bar', async () => {
    render(<CodingEntry programId="program-1" choice={choice} onReady={vi.fn()} onCancel={vi.fn()} />);
    await waitFor(() =>
      expect(codingApi.startBuild).toHaveBeenCalledWith(
        expect.objectContaining({ programId: 'program-1', task: 'build', language: 'python', difficulty: 'easy' }),
      ),
    );
    expect(screen.getByRole('progressbar')).toBeInTheDocument();
    expect(screen.getByText('Writing the problem')).toBeInTheDocument();
    expect(screen.getByText('Proving it')).toBeInTheDocument();
    expect(screen.getByText('Saving')).toBeInTheDocument();
  });

  it('starts the session with the built activity and hands over its id', async () => {
    const onReady = vi.fn();
    render(<CodingEntry programId="program-1" choice={choice} onReady={onReady} onCancel={vi.fn()} />);
    await waitFor(() => expect(codingApi.startBuild).toHaveBeenCalled());

    publish(job({ status: 'completed', progressCurrent: 3, resultRef: 'activity-7' }));
    await waitFor(() => expect(onReady).toHaveBeenCalledWith('session-9'));
    expect(codingApi.startSession).toHaveBeenCalledWith(
      expect.objectContaining({ programId: 'program-1', activityId: 'activity-7', setting: 'mock', budgetSeconds: 1200 }),
    );
  });

  it('shows a failed build with Try again, which builds anew', async () => {
    const user = userEvent.setup();
    render(<CodingEntry programId="program-1" choice={choice} onReady={vi.fn()} onCancel={vi.fn()} />);
    await waitFor(() => expect(codingApi.startBuild).toHaveBeenCalledTimes(1));

    publish(job({ status: 'failed', error: 'The reference failed a case.' }));
    expect(await screen.findByText('The reference failed a case.')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Try again' }));
    await waitFor(() => expect(codingApi.startBuild).toHaveBeenCalledTimes(2));
  });

  it('cancels', async () => {
    const user = userEvent.setup();
    const onCancel = vi.fn();
    render(<CodingEntry programId="program-1" choice={choice} onReady={vi.fn()} onCancel={onCancel} />);
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(onCancel).toHaveBeenCalled();
  });

  it('retries only the session start when it fails after the build completed', async () => {
    const user = userEvent.setup();
    vi.mocked(codingApi.startSession)
      .mockRejectedValueOnce(new Error('The session could not start.'))
      .mockResolvedValue(codingSession({ sessionId: 'session-9' }));
    const onReady = vi.fn();
    render(<CodingEntry programId="program-1" choice={choice} onReady={onReady} onCancel={vi.fn()} />);
    await waitFor(() => expect(codingApi.startBuild).toHaveBeenCalledTimes(1));

    publish(job({ status: 'completed', progressCurrent: 3, resultRef: 'activity-7' }));
    expect(await screen.findByText('The session could not start.')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Try again' }));

    await waitFor(() => expect(onReady).toHaveBeenCalledWith('session-9'));
    expect(codingApi.startBuild).toHaveBeenCalledTimes(1);
    expect(codingApi.startSession).toHaveBeenCalledTimes(2);
    const [first, second] = vi.mocked(codingApi.startSession).mock.calls.map(([request]) => request);
    expect(second.operationId).toBe(first.operationId);
    expect(second.activityId).toBe('activity-7');
  });
});
