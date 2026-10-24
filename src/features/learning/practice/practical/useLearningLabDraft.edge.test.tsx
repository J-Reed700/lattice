import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useLearningLabDraft } from '@/features/learning/practice/practical/useLearningLabDraft';
import type { LearningLabFile, LearningPracticalDraftDto } from '@/lib/bindings';

const api = vi.hoisted(() => ({ getLearningPracticalDraft: vi.fn(), saveLearningPracticalDraft: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: api }));

const ok = <T,>(data: T) => ({ ok: true as const, data });
const starter: LearningLabFile[] = [{ path: 'recipe.md', content: '# Bread' }];
const draft = (overrides: Partial<LearningPracticalDraftDto> = {}): LearningPracticalDraftDto => ({
  programId: 'program-1', activityId: 'activity-1', activityRevision: 3, draftRevision: 0, files: starter, updatedAt: null, ...overrides,
});
type Props = { activityId: string | null; starterFiles: LearningLabFile[] };

function setup(props: Partial<Props> = {}) {
  return renderHook((p: Props) => useLearningLabDraft({ programId: 'program-1', activityRevision: 3, ...p }), {
    initialProps: { activityId: 'activity-1', starterFiles: starter, ...props },
  });
}

beforeEach(() => {
  vi.resetAllMocks();
  api.getLearningPracticalDraft.mockResolvedValue(ok(draft()));
  api.saveLearningPracticalDraft.mockImplementation(async (request: { expectedDraftRevision: number; files: LearningLabFile[] }) =>
    ok(draft({ draftRevision: request.expectedDraftRevision + 1, files: request.files })));
});
afterEach(() => vi.unstubAllGlobals());

describe('useLearningLabDraft edges', () => {
  it('is idle without an activity and saves nothing', async () => {
    const { result } = setup({ activityId: null });
    expect(result.current).toMatchObject({ loadState: 'idle', saveState: 'saved', error: null, files: starter });
    act(() => result.current.setFileContent('recipe.md', 'ignored'));
    await expect(result.current.retryLoad()).resolves.toBe(false);
    await expect(result.current.keepLocalEdits()).resolves.toBe(false);
    await expect(result.current.useSavedVersion()).resolves.toBe(false);
    await expect(result.current.flush()).resolves.toBe(true);
    expect(api.getLearningPracticalDraft).not.toHaveBeenCalled();
  });

  it('refuses a draft that belongs to another activity revision, and loads again on retry', async () => {
    api.getLearningPracticalDraft.mockResolvedValueOnce(ok(draft({ activityRevision: 2 })));
    const { result } = setup();
    await waitFor(() => expect(result.current.loadState).toBe('error'));
    expect(result.current.error).toBe('The saved lab draft belongs to a different activity revision.');
    act(() => result.current.setFileContent('recipe.md', 'lost'));
    expect(result.current.files[0].content).toBe('# Bread');
    await expect(result.current.flush()).resolves.toBe(false);

    let ready = false;
    await act(async () => { ready = await result.current.retryLoad(); });
    expect(ready).toBe(true);
    expect(result.current.loadState).toBe('ready');
  });

  it('reports a load that failed outright', async () => {
    api.getLearningPracticalDraft.mockRejectedValueOnce('offline');
    const { result } = setup();
    await waitFor(() => expect(result.current.error).toBe('The lab draft could not be saved.'));
    expect(result.current.saveState).toBe('error');
  });

  it('ignores edits to unknown files or that change nothing, and adds new starter files', async () => {
    const { result, rerender } = setup();
    await waitFor(() => expect(result.current.loadState).toBe('ready'));
    act(() => result.current.setFileContent('missing.md', 'x'));
    act(() => result.current.setFileContent('recipe.md', '# Bread'));
    expect(result.current.saveState).toBe('saved');
    rerender({ activityId: 'activity-1', starterFiles: [...starter, { path: 'notes.md', content: 'Proof overnight' }] });
    expect(result.current.files.map((file) => file.path)).toEqual(['recipe.md', 'notes.md']);
  });

  it('stops on a save reply for another activity revision', async () => {
    api.saveLearningPracticalDraft.mockResolvedValueOnce(ok(draft({ activityRevision: 9, draftRevision: 1 })));
    const { result } = setup();
    await waitFor(() => expect(result.current.loadState).toBe('ready'));
    act(() => result.current.setFileContent('recipe.md', '# Rye'));
    let saved = true;
    await act(async () => { saved = await result.current.flush(); });
    expect(saved).toBe(false);
    expect(result.current).toMatchObject({ saveState: 'error', error: 'The saved lab draft reply belongs to a different activity revision.' });
  });

  it('marks a conflict when the draft skipped ahead, and keeps it until resolved', async () => {
    api.saveLearningPracticalDraft.mockResolvedValueOnce(ok(draft({ draftRevision: 5, files: [{ path: 'recipe.md', content: '# Spelt' }] })));
    const { result } = setup();
    await waitFor(() => expect(result.current.loadState).toBe('ready'));
    act(() => result.current.setFileContent('recipe.md', '# Rye'));
    await act(async () => { await result.current.flush(); });
    expect(result.current).toMatchObject({ saveState: 'conflict', error: 'This lab draft advanced to a newer revision. Reload it before saving again.' });
    act(() => result.current.setFileContent('recipe.md', '# Rye bread'));
    expect(result.current.saveState).toBe('conflict');
    await expect(result.current.flush()).resolves.toBe(false);
  });

  it('reports a failed refresh while keeping local edits or taking the saved version', async () => {
    api.saveLearningPracticalDraft.mockRejectedValueOnce(new Error('draft revision is stale'));
    const { result } = setup();
    await waitFor(() => expect(result.current.loadState).toBe('ready'));
    act(() => result.current.setFileContent('recipe.md', '# Rye'));
    await act(async () => { await result.current.flush(); });
    expect(result.current.saveState).toBe('conflict');

    api.getLearningPracticalDraft.mockRejectedValueOnce(new Error('database locked'));
    let kept = true;
    await act(async () => { kept = await result.current.keepLocalEdits(); });
    expect(kept).toBe(false);
    expect(result.current).toMatchObject({ saveState: 'conflict', error: 'database locked' });

    api.getLearningPracticalDraft.mockResolvedValueOnce(ok(draft({ activityId: 'other' })));
    let used = true;
    await act(async () => { used = await result.current.useSavedVersion(); });
    expect(used).toBe(false);
    expect(result.current.error).toBe('The saved lab draft belongs to a different activity revision.');
  });

  it('keeps local edits clean when they match the saved draft', async () => {
    api.saveLearningPracticalDraft.mockRejectedValueOnce(new Error('conflict'));
    const { result } = setup();
    await waitFor(() => expect(result.current.loadState).toBe('ready'));
    act(() => result.current.setFileContent('recipe.md', '# Rye'));
    await act(async () => { await result.current.flush(); });
    api.getLearningPracticalDraft.mockResolvedValueOnce(ok(draft({ draftRevision: 4, files: [{ path: 'recipe.md', content: '# Rye' }] })));
    await act(async () => { await result.current.keepLocalEdits(); });
    expect(result.current).toMatchObject({ saveState: 'saved', error: null });
  });

  it('asks before leaving with unsaved edits', async () => {
    const { result } = setup();
    await waitFor(() => expect(result.current.loadState).toBe('ready'));
    const clean = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(clean);
    expect(clean.defaultPrevented).toBe(false);
    act(() => result.current.setFileContent('recipe.md', '# Rye'));
    const dirty = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(dirty);
    expect(dirty.defaultPrevented).toBe(true);
  });

  it('saves pending edits when it unmounts', async () => {
    const { result, unmount } = setup();
    await waitFor(() => expect(result.current.loadState).toBe('ready'));
    act(() => result.current.setFileContent('recipe.md', '# Rye'));
    unmount();
    await waitFor(() => expect(api.saveLearningPracticalDraft).toHaveBeenCalledWith(expect.objectContaining({ files: [{ path: 'recipe.md', content: '# Rye' }] })));
  });

  it('waits for a load in progress before saving', async () => {
    let finishLoad!: (value: unknown) => void;
    api.getLearningPracticalDraft.mockReturnValueOnce(new Promise((resolve) => { finishLoad = resolve; }));
    const { result } = setup();
    act(() => result.current.setFileContent('recipe.md', '# Rye'));
    let flushed!: Promise<boolean>;
    act(() => { flushed = result.current.flush(); });
    await act(async () => { finishLoad(ok(draft())); });
    await expect(flushed).resolves.toBe(true);
    expect(api.saveLearningPracticalDraft).toHaveBeenCalledWith(expect.objectContaining({ expectedDraftRevision: 0, files: [{ path: 'recipe.md', content: '# Rye' }] }));
  });

  it('makes operation ids without crypto.randomUUID', async () => {
    vi.stubGlobal('crypto', { getRandomValues: (bytes: Uint8Array) => bytes.fill(7) });
    const { result } = setup();
    await waitFor(() => expect(result.current.loadState).toBe('ready'));
    act(() => result.current.setFileContent('recipe.md', '# Rye'));
    await act(async () => { await result.current.flush(); });
    expect(api.saveLearningPracticalDraft.mock.calls[0][0].operationId).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
  });

  it('makes operation ids with no crypto at all', async () => {
    vi.stubGlobal('crypto', undefined);
    const { result } = setup();
    await waitFor(() => expect(result.current.loadState).toBe('ready'));
    act(() => result.current.setFileContent('recipe.md', '# Rye'));
    await act(async () => { await result.current.flush(); });
    expect(api.saveLearningPracticalDraft.mock.calls[0][0].operationId).toMatch(/^[0-9a-f-]{36}$/);
  });
});
