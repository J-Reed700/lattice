import { invoke } from '@tauri-apps/api/core';
import { beforeEach, expect, it, vi } from 'vitest';

import { diagnostics } from '../utils/diagnostics';

const { VaultAPI, isUnknownCommandError } = await vi.importActual<typeof import('./api')>('./api');

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  diagnostics.clear();
});

it('returns a backend "not found" error from the canonical route without trying fallbacks', async () => {
  vi.mocked(invoke).mockRejectedValueOnce('Space not found: x');
  const result = await VaultAPI.searchSemantic('q');
  expect(result).toMatchObject({ ok: false, error: 'Space not found: x' });
  expect(invoke).toHaveBeenCalledTimes(1);
  expect(diagnostics.getSnapshot()).toHaveLength(1);
});

it('retries on Tauri unknown-command text and reports the first error when every route is unknown', async () => {
  vi.mocked(invoke)
    .mockRejectedValueOnce('plugin search not found')
    .mockRejectedValueOnce('Command search.search_semantic not found')
    .mockRejectedValueOnce('Command search_semantic not found');
  const result = await VaultAPI.searchSemantic('q');
  expect(invoke).toHaveBeenCalledTimes(3);
  expect(result).toMatchObject({ ok: false, error: 'plugin search not found' });
  expect(diagnostics.getSnapshot()).toHaveLength(1);
});

it('recognises only the exact unknown-command shapes', () => {
  expect(isUnknownCommandError('Command index_file not found')).toBe(true);
  expect(isUnknownCommandError('myplugin.cmd not allowed. Command not found')).toBe(true);
  expect(isUnknownCommandError('Space not found: x')).toBe(false);
  expect(isUnknownCommandError('Conversation not found')).toBe(false);
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
