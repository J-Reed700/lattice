import { invoke } from '@tauri-apps/api/core';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { COMMAND_PLUGINS, type CommandName } from '@/shared/ipc/routes.generated';
import { diagnostics } from '@/utils/diagnostics';

import { documentApi } from './documents';

const route = (command: CommandName) => `plugin:${COMMAND_PLUGINS[command]}|${command}`;

type Case = [label: string, call: () => Promise<unknown>, command: CommandName, args: Record<string, unknown>];

const passThrough: Case[] = [
  ['startIndexing recursive by default', () => documentApi.startIndexing('/notes/herbarium'), 'index_directory', { path: '/notes/herbarium', recursive: true, spaceId: undefined }],
  ['startIndexing flat into a space', () => documentApi.startIndexing('/notes/herbarium', false, 's1'), 'index_directory', { path: '/notes/herbarium', recursive: false, spaceId: 's1' }],
  ['indexFile', () => documentApi.indexFile('/notes/ferns.md', 's1'), 'index_file', { path: '/notes/ferns.md', spaceId: 's1' }],
  ['reindexFile', () => documentApi.reindexFile('/notes/ferns.md'), 'reindex_file', { path: '/notes/ferns.md' }],
  ['removeIndexedFile', () => documentApi.removeIndexedFile('/notes/ferns.md'), 'remove_indexed_file', { path: '/notes/ferns.md' }],
  ['deleteDocument', () => documentApi.deleteDocument('d1'), 'delete_document', { documentId: 'd1' }],
  ['renameDocument', () => documentApi.renameDocument('d1', 'Mosses.md'), 'rename_document', { documentId: 'd1', newName: 'Mosses.md' }],
  ['cancelIndexing', () => documentApi.cancelIndexing(), 'cancel_indexing', {}],
  ['pauseIndexing', () => documentApi.pauseIndexing(), 'pause_indexing', {}],
  ['resumeIndexing', () => documentApi.resumeIndexing(), 'resume_indexing', {}],
  ['clearIndexingFailure', () => documentApi.clearIndexingFailure('/notes/bad.pdf'), 'clear_indexing_failure', { path: '/notes/bad.pdf' }],
  ['getIndexingStatus', () => documentApi.getIndexingStatus(), 'get_indexing_status', {}],
  ['listIndexedFiles', () => documentApi.listIndexedFiles(50), 'list_indexed_files', { limit: 50 }],
  ['listCustomCollections', () => documentApi.listCustomCollections(), 'list_custom_collections', {}],
  ['createCustomCollection', () => documentApi.createCustomCollection({ name: 'Recipes' } as never), 'create_custom_collection', { request: { name: 'Recipes' } }],
  ['renameCustomCollection', () => documentApi.renameCustomCollection('k1', 'Soups'), 'rename_custom_collection', { collectionId: 'k1', name: 'Soups' }],
  ['moveCustomCollection to the top level', () => documentApi.moveCustomCollection('k1', null), 'move_custom_collection', { collectionId: 'k1', parentId: null }],
  ['deleteCustomCollection', () => documentApi.deleteCustomCollection('k1'), 'delete_custom_collection', { collectionId: 'k1' }],
  ['addDocumentsToCustomCollection', () => documentApi.addDocumentsToCustomCollection('k1', ['d1']), 'add_documents_to_custom_collection', { collectionId: 'k1', documentIds: ['d1'] }],
  ['removeDocumentsFromCustomCollection', () => documentApi.removeDocumentsFromCustomCollection('k1', ['d1']), 'remove_documents_from_custom_collection', { collectionId: 'k1', documentIds: ['d1'] }],
  ['getIndexedFolders', () => documentApi.getIndexedFolders(), 'get_indexed_folders', {}],
  ['getIndexingActivities', () => documentApi.getIndexingActivities(10), 'get_indexing_activities', { limit: 10 }],
  ['openFile', () => documentApi.openFile('/notes/ferns.md'), 'open_file', { path: '/notes/ferns.md' }],
  ['openFileById', () => documentApi.openFileById('d1'), 'open_file_by_id', { fileId: 'd1' }],
  ['getFilePathById', () => documentApi.getFilePathById('d1'), 'get_file_path_by_id', { fileId: 'd1' }],
  ['getRecentDocuments', () => documentApi.getRecentDocuments(5), 'get_recent_documents', { limit: 5 }],
  ['listAllDocuments', () => documentApi.listAllDocuments(500), 'list_all_documents', { limit: 500 }],
  ['getDocument', () => documentApi.getDocument('d1'), 'get_document', { documentId: 'd1' }],
  ['readFileContent', () => documentApi.readFileContent('/notes/ferns.md'), 'read_file_content', { path: '/notes/ferns.md' }],
  ['getFileMetadata', () => documentApi.getFileMetadata('/notes/ferns.md'), 'get_file_metadata', { path: '/notes/ferns.md' }],
  ['showInFolder', () => documentApi.showInFolder('/notes/ferns.md'), 'show_in_folder', { path: '/notes/ferns.md' }],
  ['getCorpusShape', () => documentApi.getCorpusShape(), 'get_corpus_shape', {}],
  ['listConversationsCitingDocument', () => documentApi.listConversationsCitingDocument('d1', 3), 'list_conversations_citing_document', { documentId: 'd1', limit: 3 }],
];

describe('documentApi', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    diagnostics.clear();
  });

  it.each(passThrough)('%s invokes its command through the registering plugin', async (_label, call, command, args) => {
    vi.mocked(invoke).mockResolvedValueOnce('payload');
    await expect(call()).resolves.toEqual({ ok: true, data: 'payload' });
    expect(invoke).toHaveBeenCalledWith(route(command), args);
  });

  describe('getIndexProgress', () => {
    it('maps the snake_case progress into a snapshot', async () => {
      vi.mocked(invoke).mockResolvedValueOnce({
        status: 'processing', total_files: 8, files_processed: 2, failed: 1,
        current_file: '/notes/ferns.md', percent_complete: 30, paused: true,
        failures: [{ path: '/notes/bad.pdf', error: 'encrypted' }],
      });
      const result = await documentApi.getIndexProgress();
      expect(invoke).toHaveBeenCalledWith(route('get_index_progress'), {});
      expect(result).toEqual({
        ok: true,
        data: {
          totalFiles: 8, processed: 2, failed: 1, currentFile: '/notes/ferns.md',
          status: 'processing', percentage: 30, paused: true,
          failures: [{ path: '/notes/bad.pdf', error: 'encrypted' }],
        },
      });
    });

    it('derives the percentage when the backend omits it and fills defaults', async () => {
      vi.mocked(invoke).mockResolvedValueOnce({ status: 'scanning', total_files: 4, files_processed: 1 });
      const result = await documentApi.getIndexProgress();
      expect(result).toEqual({
        ok: true,
        data: {
          totalFiles: 4, processed: 1, failed: 0, currentFile: undefined,
          status: 'scanning', percentage: 25, paused: false, failures: [],
        },
      });
    });

    it('reports zero percent with no files rather than dividing by zero', async () => {
      vi.mocked(invoke).mockResolvedValueOnce({ status: 'idle' });
      const result = await documentApi.getIndexProgress();
      expect(result).toMatchObject({ ok: true, data: { totalFiles: 0, processed: 0, percentage: 0 } });
    });

    it('rejects a status the renderer does not know', async () => {
      vi.mocked(invoke).mockResolvedValueOnce({ status: 'melting' });
      await expect(documentApi.getIndexProgress()).rejects.toThrow('Unknown indexing status: melting');
    });

    it('passes a failed call through untouched', async () => {
      vi.mocked(invoke).mockRejectedValueOnce('indexer offline');
      await expect(documentApi.getIndexProgress()).resolves.toMatchObject({ ok: false, error: 'indexer offline' });
    });
  });

  describe('getIndexingStats', () => {
    it('keeps camelCase counts', async () => {
      vi.mocked(invoke).mockResolvedValueOnce({ indexedDocuments: 12, totalChunks: 340 });
      await expect(documentApi.getIndexingStats()).resolves.toEqual({ ok: true, data: { indexedDocuments: 12, totalChunks: 340 } });
    });

    it('accepts snake_case counts from older payloads', async () => {
      vi.mocked(invoke).mockResolvedValueOnce({ indexed_documents: 3, total_chunks: 9 });
      await expect(documentApi.getIndexingStats()).resolves.toEqual({ ok: true, data: { indexedDocuments: 3, totalChunks: 9 } });
    });

    it('counts missing values as zero', async () => {
      vi.mocked(invoke).mockResolvedValueOnce({});
      await expect(documentApi.getIndexingStats()).resolves.toEqual({ ok: true, data: { indexedDocuments: 0, totalChunks: 0 } });
    });

    it('passes a failed call through untouched', async () => {
      vi.mocked(invoke).mockRejectedValueOnce('db locked');
      await expect(documentApi.getIndexingStats()).resolves.toMatchObject({ ok: false, error: 'db locked' });
    });
  });

  describe('readFileBytes', () => {
    it('turns the byte array into a Uint8Array', async () => {
      vi.mocked(invoke).mockResolvedValueOnce([104, 105]);
      const result = await documentApi.readFileBytes('/notes/hi.txt');
      expect(invoke).toHaveBeenCalledWith(route('read_file_bytes'), { path: '/notes/hi.txt' });
      expect(result.ok && Array.from(result.data)).toEqual([104, 105]);
      expect(result.ok && result.data).toBeInstanceOf(Uint8Array);
    });

    it('passes a failed read through', async () => {
      vi.mocked(invoke).mockRejectedValueOnce('no such file');
      await expect(documentApi.readFileBytes('/gone')).resolves.toMatchObject({ ok: false, error: 'no such file' });
    });
  });
});
