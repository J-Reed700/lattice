import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  cancelPendingBatchUrlParse,
  flushPendingBatchUrlParse,
  reattachActiveBatchUrlJob,
  registerBatchUrlCompleteHandler,
  resetBatchUrlImport,
  retryBatchUrlPoll,
  setBatchUrlText,
  startBatchUrlImportRun,
} from '@/features/files/hooks/batchUrlImportRunner';
import { useBatchUrlImportStore } from '@/features/files/stores/batchUrlImportStore';
import type { ApiClient } from '@/lib/api';
import { useToastStore } from '@/stores/toastStore';
import type { BatchJobStatus } from '@/types/api/batch';
import type { UrlPreview } from '@/types/api/web';

function createFakeApi(overrides: Partial<ApiClient> = {}): ApiClient {
  return {
    fetchUrlPreview: vi.fn(async (url: string) => ({
      ok: true as const,
      data: {
        url,
        title: `Title ${url}`,
        siteName: 'Example',
        readingTimeMinutes: 2,
        wordCount: 300,
      } as UrlPreview,
    })),
    listBatchJobs: vi.fn(async () => ({ ok: true as const, data: [] })),
    startBatchUrlImport: vi.fn(async () => ({ ok: true as const, data: 'new-job' })),
    getBatchJobStatus: vi.fn(),
    cancelBatchJob: vi.fn(async () => ({ ok: true as const, data: 1 })),
    getCorpusShape: vi.fn(async () => ({
      ok: true as const,
      data: { total: 5, byType: [], grownLast7Days: 5 } as never,
    })),
    ...overrides,
  } as unknown as ApiClient;
}

const makeStatus = (overrides: Partial<BatchJobStatus> = {}): BatchJobStatus => ({
  id: 'new-job',
  jobId: 'new-job',
  jobType: 'url_import',
  status: 'running',
  totalItems: 2,
  completedItems: 0,
  failedItems: 0,
  total_items: 2,
  completed_items: 0,
  failed_items: 0,
  progress: 0,
  createdAt: '',
  completedAt: null,
  items: [],
  ...overrides,
});

describe('batchUrlImportRunner (real runner)', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    resetBatchUrlImport();
    useToastStore.setState({ toasts: [] });
    vi.spyOn(console, 'error').mockImplementation(() => {});
    vi.spyOn(console, 'warn').mockImplementation(() => {});
  });

  afterEach(() => {
    vi.clearAllTimers();
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('starts the user pasted URLs in their own job even when an unrelated url_import job is running, and checks completion against backend totalItems', async () => {
    const oldJobId = 'old-retry-job';
    const pastedJobId = 'user-pasted-job';
    const urlNew1 = 'https://example.org/new-1';
    const urlNew2 = 'https://example.org/new-2';

    const api = createFakeApi({
      listBatchJobs: vi.fn(async () => ({
        ok: true as const,
        data: [
          {
            id: oldJobId,
            jobId: oldJobId,
            jobType: 'url_import',
            status: 'running',
            totalItems: 5,
            completedItems: 2,
            failedItems: 0,
            progress: 40,
            createdAt: '',
          },
        ],
      })),
      startBatchUrlImport: vi.fn(async () => ({ ok: true as const, data: pastedJobId })),
      getBatchJobStatus: vi
        .fn()
        // First poll: 1 of 2 finished on user's own job (even if totalItems=5 on another job, here totalItems=2)
        .mockResolvedValueOnce({
          ok: true,
          data: makeStatus({
            id: pastedJobId,
            jobId: pastedJobId,
            status: 'running',
            totalItems: 2,
            total_items: 2,
            completedItems: 1,
            failedItems: 0,
            items: [
              { itemId: '1', target: urlNew1, status: 'completed', documentId: 'doc-1' },
              { itemId: '2', target: urlNew2, status: 'running', documentId: null },
            ],
          }),
        })
        .mockResolvedValueOnce({
          ok: true,
          data: makeStatus({
            id: pastedJobId,
            jobId: pastedJobId,
            status: 'completed',
            totalItems: 2,
            total_items: 2,
            completedItems: 2,
            failedItems: 0,
            items: [
              { itemId: '1', target: urlNew1, status: 'completed', documentId: 'doc-1' },
              { itemId: '2', target: urlNew2, status: 'completed', documentId: 'doc-2' },
            ],
          }),
        }),
    });

    setBatchUrlText(`${urlNew1}\n${urlNew2}`, api);
    await vi.advanceTimersByTimeAsync(500);
    await Promise.resolve();

    await startBatchUrlImportRun(api);

    expect(api.startBatchUrlImport).toHaveBeenCalledExactlyOnceWith(
      [urlNew1, urlNew2],
      { extractArticle: true },
    );
    expect(api.getBatchJobStatus).toHaveBeenCalledWith(pastedJobId);
    expect(api.getBatchJobStatus).not.toHaveBeenCalledWith(oldJobId);
    expect(useBatchUrlImportStore.getState().isImporting).toBe(true);

    await vi.advanceTimersByTimeAsync(800);
    await Promise.resolve();

    expect(useBatchUrlImportStore.getState().isImporting).toBe(false);
    expect(useBatchUrlImportStore.getState().activeJobId).toBeNull();
    expect(useToastStore.getState().toasts).toHaveLength(1);
    expect(useToastStore.getState().toasts[0]).toMatchObject({
      type: 'success',
      title: 'Added 2 URLs',
    });

    // Remounting while old-retry-job is still running must not re-toast or overwrite the user's finished items.
    await reattachActiveBatchUrlJob(api);
    expect(api.getBatchJobStatus).not.toHaveBeenCalledWith(oldJobId);
    expect(useToastStore.getState().toasts).toHaveLength(1);
  });

  it('compares completion to backend totalItems rather than local selected.length when reattached to a larger job', async () => {
    const api = createFakeApi({
      listBatchJobs: vi.fn(async () => ({
        ok: true as const,
        data: [
          {
            id: 'large-job',
            jobId: 'large-job',
            jobType: 'url_import',
            status: 'running',
            totalItems: 4,
            completedItems: 1,
            failedItems: 0,
            progress: 25,
            createdAt: '',
          },
        ],
      })),
      getBatchJobStatus: vi
        .fn()
        // 2 of 4 processed: if selected.length (e.g. 2) were used, this would falsely stop!
        .mockResolvedValueOnce({
          ok: true,
          data: makeStatus({
            id: 'large-job',
            jobId: 'large-job',
            status: 'running',
            totalItems: 4,
            total_items: 4,
            completedItems: 2,
            failedItems: 0,
            items: [
              { itemId: '1', target: 'https://a.example', status: 'completed', documentId: 'd1' },
              { itemId: '2', target: 'https://b.example', status: 'completed', documentId: 'd2' },
              { itemId: '3', target: 'https://c.example', status: 'running', documentId: null },
              { itemId: '4', target: 'https://d.example', status: 'pending', documentId: null },
            ],
          }),
        })
        .mockResolvedValueOnce({
          ok: true,
          data: makeStatus({
            id: 'large-job',
            jobId: 'large-job',
            status: 'completed',
            totalItems: 4,
            total_items: 4,
            completedItems: 4,
            failedItems: 0,
            items: [
              { itemId: '1', target: 'https://a.example', status: 'completed', documentId: 'd1' },
              { itemId: '2', target: 'https://b.example', status: 'completed', documentId: 'd2' },
              { itemId: '3', target: 'https://c.example', status: 'completed', documentId: 'd3' },
              { itemId: '4', target: 'https://d.example', status: 'completed', documentId: 'd4' },
            ],
          }),
        }),
    });

    await reattachActiveBatchUrlJob(api);
    await Promise.resolve();

    // Still importing after 2 of 4!
    expect(useBatchUrlImportStore.getState().isImporting).toBe(true);
    expect(useBatchUrlImportStore.getState().activeJobId).toBe('large-job');
    expect(useToastStore.getState().toasts).toHaveLength(0);

    await vi.advanceTimersByTimeAsync(800);
    await Promise.resolve();

    expect(useBatchUrlImportStore.getState().isImporting).toBe(false);
    expect(useToastStore.getState().toasts).toHaveLength(1);
    expect(useToastStore.getState().toasts[0]).toMatchObject({ title: 'Added 4 URLs' });
  });

  it('continues background tracking through transient poll errors and past 180s while making progress', async () => {
    const urlA = 'https://example.org/a';
    const urlB = 'https://example.org/b';
    let callCount = 0;

    const api = createFakeApi({
      startBatchUrlImport: vi.fn(async () => ({ ok: true as const, data: 'long-job' })),
      getBatchJobStatus: vi.fn(async () => {
        callCount += 1;
        if (callCount === 1) {
          return { ok: false as const, error: 'Temporary IPC glitch' };
        }
        if (callCount < 230) {
          // Simulates steady progress every ~100s? Wait: change item status/count every 150 polls (~120s)
          const completed = callCount >= 150 ? 1 : 0;
          return {
            ok: true as const,
            data: makeStatus({
              id: 'long-job',
              jobId: 'long-job',
              status: 'running',
              totalItems: 2,
              total_items: 2,
              completedItems: completed,
              failedItems: 0,
              items: [
                {
                  itemId: 'a',
                  target: urlA,
                  status: completed === 1 ? 'completed' : 'processing',
                  documentId: completed === 1 ? 'doc-a' : null,
                },
                { itemId: 'b', target: urlB, status: 'processing', documentId: null },
              ],
            }),
          };
        }
        return {
          ok: true as const,
          data: makeStatus({
            id: 'long-job',
            jobId: 'long-job',
            status: 'completed',
            totalItems: 2,
            total_items: 2,
            completedItems: 2,
            failedItems: 0,
            items: [
              { itemId: 'a', target: urlA, status: 'completed', documentId: 'doc-a' },
              { itemId: 'b', target: urlB, status: 'completed', documentId: 'doc-b' },
            ],
          }),
        };
      }),
    });

    const onComplete = vi.fn();
    const unregister = registerBatchUrlCompleteHandler(onComplete);
    setBatchUrlText(`${urlA}\n${urlB}`, api);
    await vi.advanceTimersByTimeAsync(500);
    await Promise.resolve();

    await startBatchUrlImportRun(api);
    await vi.advanceTimersByTimeAsync(0);

    // First poll failed transiently: still importing, surfaces warning, retries automatically.
    expect(useBatchUrlImportStore.getState().isImporting).toBe(true);
    expect(useBatchUrlImportStore.getState().runError).toBe('Temporary IPC glitch');

    // Advance past 180s (230 * 800ms = 184,000ms) with progress occurring around 120s.
    for (let i = 0; i < 230; i += 1) {
      await vi.advanceTimersByTimeAsync(800);
    }

    expect(useBatchUrlImportStore.getState().isImporting).toBe(false);
    expect(useBatchUrlImportStore.getState().runError).toBeNull();
    expect(onComplete).toHaveBeenCalledExactlyOnceWith({ successful: 2, failed: 0 });
    unregister();
  });

  it('halts after consecutive poll errors and allows manual Retry status without duplicating jobs', async () => {
    const api = createFakeApi({
      startBatchUrlImport: vi.fn(async () => ({ ok: true as const, data: 'flaky-job' })),
      getBatchJobStatus: vi
        .fn()
        .mockResolvedValueOnce({ ok: false, error: 'Err 1' })
        .mockResolvedValueOnce({ ok: false, error: 'Err 2' })
        .mockResolvedValueOnce({ ok: false, error: 'Err 3' })
        .mockResolvedValueOnce({ ok: false, error: 'Err 4' })
        .mockResolvedValueOnce({ ok: false, error: 'Err 5' })
        .mockResolvedValueOnce({
          ok: true,
          data: makeStatus({
            id: 'flaky-job',
            jobId: 'flaky-job',
            status: 'completed',
            totalItems: 1,
            total_items: 1,
            completedItems: 1,
            items: [{ itemId: '1', target: 'https://example.org/x', status: 'completed', documentId: 'd1' }],
          }),
        }),
    });

    setBatchUrlText('https://example.org/x', api);
    await vi.advanceTimersByTimeAsync(500);
    await Promise.resolve();

    await startBatchUrlImportRun(api);
    for (let i = 0; i < 4; i += 1) {
      await vi.advanceTimersByTimeAsync(800);
    }

    expect(useBatchUrlImportStore.getState().isImporting).toBe(false);
    expect(useBatchUrlImportStore.getState().runError).toBe('Err 5');
    expect(useBatchUrlImportStore.getState().activeJobId).toBe('flaky-job');

    await retryBatchUrlPoll();
    await Promise.resolve();

    expect(api.startBatchUrlImport).toHaveBeenCalledOnce();
    expect(useBatchUrlImportStore.getState().isImporting).toBe(false);
    expect(useBatchUrlImportStore.getState().runError).toBeNull();
    expect(useToastStore.getState().toasts).toHaveLength(1);
  });

  it('flushes unparsed urlText on remount when typing <500ms before navigating away', async () => {
    const api = createFakeApi();
    setBatchUrlText('https://example.org/quick', api);
    // Navigate away before 500ms debounce fires.
    cancelPendingBatchUrlParse();
    await vi.advanceTimersByTimeAsync(1000);
    expect(api.fetchUrlPreview).not.toHaveBeenCalled();
    expect(useBatchUrlImportStore.getState().items).toHaveLength(0);

    // Navigate back -> flush parses immediately without requiring another edit.
    flushPendingBatchUrlParse(api);
    await Promise.resolve();
    expect(api.fetchUrlPreview).toHaveBeenCalledExactlyOnceWith('https://example.org/quick');
    expect(useBatchUrlImportStore.getState().items).toHaveLength(1);
  });
});
