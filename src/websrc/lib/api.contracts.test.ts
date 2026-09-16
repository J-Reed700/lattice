import { invoke } from '@tauri-apps/api/core';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { BatchJobStatusDto, IndexingActivity, TagDto } from './bindings';

const { VaultAPI } = await vi.importActual<typeof import('./api')>('./api');
const ipc = vi.mocked(invoke);

describe('generated IPC contracts and UI adapters', () => {
  beforeEach(() => ipc.mockReset());

  it('retains the canonical document and failure for each import item', async () => {
    const response: BatchJobStatusDto = {
      jobId: 'batch-1', jobType: 'file_import', status: 'completed', totalItems: 2,
      completedItems: 1, failedItems: 1, createdAt: '2026-09-15T00:00:00Z',
      completedAt: '2026-09-15T00:01:00Z', items: [
        { itemId: 'item-1', target: '/manual.pdf', status: 'completed', documentId: 'doc-1' },
        { itemId: 'item-2', target: '/failed.pdf', status: 'failed', documentId: null, errorMessage: 'No readable text' },
      ],
    };
    ipc.mockResolvedValueOnce(response);
    expect(await VaultAPI.getBatchJobStatus('batch-1')).toMatchObject({ ok: true, data: {
      id: 'batch-1', progress: 100, completedAt: response.completedAt,
      items: [{ documentId: 'doc-1' }, { errorMessage: 'No readable text', target: '/failed.pdf' }],
    } });
    expect(ipc).toHaveBeenCalledWith('plugin:batch|get_batch_status', { request: { jobId: 'batch-1' } });
  });

  it('unwraps the cancellation count returned by the plugin', async () => {
    ipc.mockResolvedValueOnce({ cancelledCount: 3 });
    expect(await VaultAPI.cancelBatchJob('batch-1')).toEqual({ ok: true, data: 3 });
    expect(ipc).toHaveBeenCalledWith('plugin:batch|cancel_batch', { request: { jobId: 'batch-1' } });
  });

  it('wraps the actual tag array for the document view', async () => {
    const tags: TagDto[] = [{ id: 'tag-1', name: 'MPEP', color: null, description: null }];
    ipc.mockResolvedValueOnce(tags);
    expect(await VaultAPI.getDocumentTags({ documentId: 'doc-1' })).toEqual({ ok: true, data: { documentId: 'doc-1', tags } });
  });

  it('sends the exact nested Rust request keys when removing a tag', async () => {
    ipc.mockResolvedValueOnce(null);
    await VaultAPI.removeTagFromDocument({ documentId: 'doc-1', tagId: 'tag-1' });
    expect(ipc).toHaveBeenCalledWith('plugin:tags|remove_tag_from_document', { request: { document_id: 'doc-1', tag_id: 'tag-1' } });
  });

  it('retains camelCase file paths from indexing activity responses', async () => {
    const activity: IndexingActivity = { id: 'activity-1', action: 'index', filePath: '/manual.pdf', status: 'completed', timestamp: '2026-09-15', details: null };
    ipc.mockResolvedValueOnce([activity]);
    expect(await VaultAPI.getIndexingActivities(10)).toEqual({ ok: true, data: [activity] });
  });

  it('surfaces a database failure instead of returning an empty inventory', async () => {
    ipc.mockRejectedValueOnce(new Error('database is locked'));
    const result = await VaultAPI.listAllDocuments(100);
    expect(result).toMatchObject({ ok: false, error: 'database is locked' });
    expect(ipc).toHaveBeenCalledTimes(1);
  });

  it('sends the required tagged search mode to semantic search', async () => {
    ipc.mockResolvedValueOnce([]).mockResolvedValueOnce([]);
    await VaultAPI.searchSemantic('manual', 5);
    expect(ipc).toHaveBeenNthCalledWith(1, 'plugin:search|semantic_search', {
      request: { query: 'manual', limit: 5, threshold: null, mode: { type: 'vector' } },
    });
  });

  it.each([{ unexpectedResults: [] }, [{ document: 'stale schema' }]])('rejects malformed search responses instead of claiming no matches: %j', async response => {
    ipc.mockResolvedValueOnce(response);
    expect(await VaultAPI.searchSemantic('manual')).toEqual({ ok: false, error: 'Semantic search failed: invalid search response' });
    expect(ipc).toHaveBeenCalledTimes(1);
  });
});
