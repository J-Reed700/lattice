import { describe, expect, it, vi } from 'vitest';

import { MAX_CONSECUTIVE_POLL_FAILURES, pollAttachmentImport } from '@/features/chat/model/attachmentImportPoll';
import type { ApiResult } from '@/types';
import type { BatchJobStatus } from '@/types/api/batch';


function job(overrides: Partial<BatchJobStatus>): BatchJobStatus {
  return {
    status: 'running',
    totalItems: 1,
    completedItems: 0,
    failedItems: 0,
    items: [],
    ...overrides,
  } as BatchJobStatus;
}

const fail: ApiResult<BatchJobStatus> = { ok: false, error: 'database is locked' };
const options = (getStatus: (id: string) => Promise<ApiResult<BatchJobStatus>>) => ({
  getStatus,
  intervalMs: 0,
  timeoutMs: 60_000,
  sleep: () => Promise.resolve(),
});

describe('pollAttachmentImport', () => {
  it('rides out a transient status error and returns the finished ids', async () => {
    const getStatus = vi
      .fn()
      .mockResolvedValueOnce(fail)
      .mockResolvedValueOnce({
        ok: true,
        data: job({ status: 'completed', completedItems: 1, items: [{ documentId: 'doc-1' }] as BatchJobStatus['items'] }),
      });
    const outcome = await pollAttachmentImport('job', options(getStatus));
    expect(outcome).toEqual({ kind: 'finished', documentIds: ['doc-1'], added: 1, failed: 0 });
  });

  it('gives up after consecutive failures instead of sending without ids', async () => {
    const getStatus = vi.fn().mockResolvedValue(fail);
    const outcome = await pollAttachmentImport('job', options(getStatus));
    expect(outcome).toEqual({ kind: 'unreachable', error: 'database is locked' });
    expect(getStatus).toHaveBeenCalledTimes(MAX_CONSECUTIVE_POLL_FAILURES);
  });

  it('reports the ids seen so far when the deadline passes mid-index', async () => {
    let t = 0;
    const getStatus = vi.fn().mockImplementation(async () => {
      t += 40_000;
      return { ok: true, data: job({ items: [{ documentId: 'doc-2' }] as BatchJobStatus['items'] }) };
    });
    const outcome = await pollAttachmentImport('job', { ...options(getStatus), now: () => t });
    expect(outcome).toEqual({ kind: 'pending', documentIds: ['doc-2'] });
  });
});
