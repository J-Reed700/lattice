import React from 'react';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { LearningPracticalDraftDto } from '@/lib/bindings';
import { flushPendingSaves } from '@/lib/pendingSaves';
import { useLearningLabDraft } from './useLearningLabDraft';

const api = vi.hoisted(() => ({ getLearningPracticalDraft: vi.fn(), saveLearningPracticalDraft: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: api }));

const ok = <T,>(data: T) => ({ ok: true as const, data });
const starter = [{ path: 'main.py', content: 'print("starter")' }];
function draft(activityId = 'activity-1', files = starter, draftRevision = 0, activityRevision = 3): LearningPracticalDraftDto {
  return { programId: 'program-1', activityId, activityRevision, draftRevision, files, updatedAt: draftRevision ? 10 : null };
}
function Harness({ activityId = 'activity-1', activityRevision = 3 }: { activityId?: string | null; activityRevision?: number }) {
  const hook = useLearningLabDraft({ programId: 'program-1', activityId, activityRevision, starterFiles: starter });
  return <div>
    <output>{hook.loadState}</output><output>{hook.saveState}</output>
    <textarea aria-label="code" value={hook.files[0]?.content ?? ''} onChange={(event) => hook.setFileContent('main.py', event.target.value)} />
    <button onClick={() => void hook.flush()}>Flush</button>
    <button onClick={() => void hook.keepLocalEdits()}>Keep local</button>
    <button onClick={() => void hook.useSavedVersion()}>Use saved</button>
    {hook.error && <div role="alert">{hook.error}</div>}
  </div>;
}

describe('useLearningLabDraft', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    api.getLearningPracticalDraft.mockImplementation(async ({ activityId, activityRevision }: { activityId: string; activityRevision: number }) => ok(draft(activityId, starter, 0, activityRevision)));
    api.saveLearningPracticalDraft.mockImplementation(async (request: { activityId: string; activityRevision: number; expectedDraftRevision: number; files: typeof starter }) => ok(draft(request.activityId, request.files, request.expectedDraftRevision + 1, request.activityRevision)));
  });

  it('restores backend edits after remount and keeps program/activity/revision scopes separate', async () => {
    const view = render(<Harness />);
    await waitFor(() => expect(screen.getByText('ready')).toBeInTheDocument());
    fireEvent.change(screen.getByLabelText('code'), { target: { value: 'saved learner code' } });
    await act(async () => { expect(await flushPendingSaves()).toBe(true); });
    expect(api.saveLearningPracticalDraft).toHaveBeenCalledWith(expect.objectContaining({ activityId: 'activity-1', activityRevision: 3, files: [{ path: 'main.py', content: 'saved learner code' }] }));
    view.unmount();
    api.getLearningPracticalDraft.mockResolvedValueOnce(ok(draft('activity-1', [{ path: 'main.py', content: 'saved learner code' }], 1, 3)));
    render(<Harness />);
    await waitFor(() => expect(screen.getByLabelText('code')).toHaveValue('saved learner code'));
  });

  it('serializes edits made during an in-flight save using the returned revision', async () => {
    let resolveFirst!: (value: ReturnType<typeof ok<LearningPracticalDraftDto>>) => void;
    api.saveLearningPracticalDraft.mockImplementationOnce((request: { activityId: string; activityRevision: number; expectedDraftRevision: number; files: typeof starter }) => new Promise((resolve) => {
      resolveFirst = resolve;
      void request;
    }));
    render(<Harness />);
    await waitFor(() => expect(screen.getByText('ready')).toBeInTheDocument());
    fireEvent.change(screen.getByLabelText('code'), { target: { value: 'first edit' } });
    await waitFor(() => expect(api.saveLearningPracticalDraft).toHaveBeenCalledTimes(1));
    fireEvent.change(screen.getByLabelText('code'), { target: { value: 'newer edit' } });
    await act(async () => { resolveFirst(ok(draft('activity-1', [{ path: 'main.py', content: 'first edit' }], 1, 3))); });
    await waitFor(() => expect(api.saveLearningPracticalDraft).toHaveBeenCalledTimes(2));
    expect(api.saveLearningPracticalDraft.mock.calls[1][0]).toEqual(expect.objectContaining({ expectedDraftRevision: 1, files: [{ path: 'main.py', content: 'newer edit' }] }));
  });

  it('retains a failed operation unchanged for response-loss retry and blocks pending-save navigation', async () => {
    api.saveLearningPracticalDraft.mockRejectedValueOnce(new Error('temporary database failure')).mockRejectedValueOnce(new Error('temporary database failure'));
    render(<Harness />);
    await waitFor(() => expect(screen.getByText('ready')).toBeInTheDocument());
    fireEvent.change(screen.getByLabelText('code'), { target: { value: 'retry me' } });
    await waitFor(() => expect(screen.getByText('error')).toBeInTheDocument());
    const firstRequest = api.saveLearningPracticalDraft.mock.calls[0][0];
    expect(await flushPendingSaves()).toBe(false);
    fireEvent.click(screen.getByRole('button', { name: 'Flush' }));
    await waitFor(() => expect(api.saveLearningPracticalDraft).toHaveBeenCalledTimes(3));
    expect(api.saveLearningPracticalDraft.mock.calls[1][0]).toEqual(firstRequest);
    expect(api.saveLearningPracticalDraft.mock.calls[2][0]).toEqual(firstRequest);
  });

  it('does not let an initial load response overwrite a learner edit made while loading', async () => {
    let resolveLoad!: (value: ReturnType<typeof ok<LearningPracticalDraftDto>>) => void;
    api.getLearningPracticalDraft.mockImplementationOnce(() => new Promise((resolve) => { resolveLoad = resolve; }));
    render(<Harness />);
    fireEvent.change(screen.getByLabelText('code'), { target: { value: 'typed while loading' } });
    await act(async () => { resolveLoad(ok(draft('activity-1', [{ path: 'main.py', content: 'older saved version' }], 2))); });
    expect(screen.getByLabelText('code')).toHaveValue('typed while loading');
  });
});
