import { invoke } from '@tauri-apps/api/core';
import { beforeEach, expect, it, vi } from 'vitest';

import { diagnostics } from '../utils/diagnostics';

const { VaultAPI } = await vi.importActual<typeof import('./api')>('./api');

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  diagnostics.clear();
});

it('returns a backend "not found" error as the answer', async () => {
  vi.mocked(invoke).mockRejectedValueOnce('Space not found: x');
  const result = await VaultAPI.searchSemantic('q');
  expect(result).toMatchObject({ ok: false, error: 'Space not found: x' });
  expect(invoke).toHaveBeenCalledTimes(1);
  expect(diagnostics.getSnapshot()).toHaveLength(1);
});

it('invokes each command through the plugin the generator says registers it', async () => {
  vi.mocked(invoke).mockResolvedValueOnce([]);
  await VaultAPI.listDownloads();
  expect(invoke).toHaveBeenCalledWith('plugin:download|list_downloads', {});
});

it('reports a Tauri unknown-command rejection as the failure, with no second route', async () => {
  vi.mocked(invoke).mockRejectedValueOnce('plugin search not found');
  const result = await VaultAPI.searchSemantic('q');
  expect(invoke).toHaveBeenCalledTimes(1);
  expect(result).toMatchObject({ ok: false, error: 'plugin search not found', details: { code: 'UNKNOWN' } });
  expect(diagnostics.getSnapshot()).toHaveLength(1);
});

it('sends source snapshots and conversation links with a journal capture', async () => {
  vi.mocked(invoke).mockResolvedValueOnce({ noteId: 'note-1', noteTitle: 'Today', created: false });
  const sources = [{
    documentId: 'doc', chunkId: 'passage', content: 'Original passage', score: 1,
    path: null, position: null, fileName: 'Report', filePath: '/report.pdf', mimeType: 'application/pdf',
    category: 'Document', fileSizeBytes: 12, modifiedAt: '', citationId: 3, pageNumber: 4,
  }];
  await VaultAPI.quickCapture('Claim [3]', sources, ['conversation-1']);
  expect(invoke).toHaveBeenCalledWith('plugin:dailynotes|quick_capture', {
    content: 'Claim [3]', sources, conversationIds: ['conversation-1'],
  });
});

it('keeps one function per facade member, so members are stable as hook dependencies', () => {
  expect(VaultAPI.listDownloads).toBe(VaultAPI.listDownloads);
});

it('is not mistaken for a promise when returned from an async function', async () => {
  const facade = await Promise.resolve(VaultAPI);
  expect(facade).toBe(VaultAPI);
});
