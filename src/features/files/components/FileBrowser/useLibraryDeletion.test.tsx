import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { deferred } from '@/tests/deferred';
import type { DocumentMetadata } from '@/types/fileBrowser';

import { useLibraryDeletion } from './useLibraryDeletion';

const h = vi.hoisted(() => ({ toast: { success: vi.fn(), error: vi.fn() } }));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));

type Args = { ids: string[]; shouldStop?: () => boolean; onProgress?: (n: number) => void };

const fern = { id: 'd1', fileName: 'ferns.md' } as DocumentMetadata;

const setup = (mutateAsync: (args: Args) => Promise<{ deleted: number; errors: string[] }>, isPending = false) => {
  const clearSelection = vi.fn();
  const deleteDocuments = { mutateAsync: vi.fn(mutateAsync), isPending } as never;
  const hook = renderHook(() => useLibraryDeletion({ deleteDocuments, clearSelection }));
  return { ...hook, clearSelection, mutate: (deleteDocuments as { mutateAsync: ReturnType<typeof vi.fn> }).mutateAsync };
};

describe('useLibraryDeletion', () => {
  beforeEach(() => vi.clearAllMocks());

  describe('single delete', () => {
    it('waits for a confirm, then deletes the one document', async () => {
      const { result, mutate } = setup(async () => ({ deleted: 1, errors: [] }));
      await act(() => result.current.handleDelete(fern));
      expect(result.current.pendingDeleteDoc).toBe(fern);
      expect(mutate).not.toHaveBeenCalled();
      await act(() => result.current.executeDelete());
      expect(mutate).toHaveBeenCalledWith({ ids: ['d1'] });
      expect(h.toast.success).toHaveBeenCalledWith('Document deleted');
      expect(result.current.pendingDeleteDoc).toBeNull();
    });

    it('does nothing without a pending document', async () => {
      const { result, mutate } = setup(async () => ({ deleted: 0, errors: [] }));
      await act(() => result.current.executeDelete());
      expect(mutate).not.toHaveBeenCalled();
    });

    it('throws the first error so the dialog can show it, keeping the document pending', async () => {
      const { result } = setup(async () => ({ deleted: 0, errors: ['file is locked'] }));
      act(() => result.current.setPendingDeleteDoc(fern));
      await act(async () => {
        await expect(result.current.executeDelete()).rejects.toThrow('file is locked');
      });
      expect(result.current.pendingDeleteDoc).toBe(fern);
      expect(h.toast.success).not.toHaveBeenCalled();
    });
  });

  describe('bulk delete', () => {
    it('does nothing without a pending bulk delete', async () => {
      const { result, mutate } = setup(async () => ({ deleted: 0, errors: [] }));
      await act(() => result.current.executeBulkDelete());
      expect(mutate).not.toHaveBeenCalled();
    });

    it('does nothing while a delete is already running', async () => {
      const { result, mutate } = setup(async () => ({ deleted: 0, errors: [] }), true);
      expect(result.current.isDeleting).toBe(true);
      act(() => result.current.setPendingBulkDelete({ ids: ['a'], count: 1 }));
      await act(() => result.current.executeBulkDelete());
      expect(mutate).not.toHaveBeenCalled();
    });

    it('reports progress, then success, and clears the selection', async () => {
      const pending = deferred<{ deleted: number; errors: string[] }>();
      let args!: Args;
      const { result, clearSelection } = setup((a) => { args = a; return pending.promise; });
      act(() => result.current.setPendingBulkDelete({ ids: ['a', 'b', 'c'], count: 3 }));
      let run!: Promise<void>;
      act(() => { run = result.current.executeBulkDelete(); });
      expect(args.ids).toEqual(['a', 'b', 'c']);
      act(() => args.onProgress?.(2));
      expect(result.current.deleteProgress).toBe(2);
      expect(args.shouldStop?.()).toBe(false);
      result.current.bulkDeleteCancelled.current = true;
      expect(args.shouldStop?.()).toBe(true);
      await act(async () => { pending.resolve({ deleted: 3, errors: [] }); await run; });
      expect(h.toast.success).toHaveBeenCalledWith('3 documents deleted');
      expect(clearSelection).toHaveBeenCalled();
      expect(result.current.pendingBulkDelete).toBeNull();
    });

    it('reports partial failures, listing at most three', async () => {
      const { result, clearSelection } = setup(async () => ({ deleted: 1, errors: ['a', 'b', 'c', 'd'] }));
      act(() => result.current.setPendingBulkDelete({ ids: ['1', '2', '3', '4', '5'], count: 5 }));
      await act(() => result.current.executeBulkDelete());
      expect(h.toast.success).toHaveBeenCalledWith('1 document deleted');
      expect(h.toast.error).toHaveBeenCalledWith('Failed to delete 4 documents', { message: 'a, b, c…' });
      expect(clearSelection).toHaveBeenCalled();
    });

    it('keeps the selection when nothing was deleted', async () => {
      const { result, clearSelection } = setup(async () => ({ deleted: 0, errors: ['only'] }));
      act(() => result.current.setPendingBulkDelete({ ids: ['1'], count: 1 }));
      await act(() => result.current.executeBulkDelete());
      expect(h.toast.error).toHaveBeenCalledWith('Failed to delete 1 document', { message: 'only' });
      expect(h.toast.success).not.toHaveBeenCalled();
      expect(clearSelection).not.toHaveBeenCalled();
    });

    it('reports a delete that stopped and closes the confirm', async () => {
      const { result } = setup(async () => { throw new Error('backend crashed'); });
      act(() => result.current.setPendingBulkDelete({ ids: ['1'], count: 1 }));
      await act(() => result.current.executeBulkDelete());
      expect(h.toast.error).toHaveBeenCalledWith('Deletion stopped', { message: 'backend crashed' });
      expect(result.current.pendingBulkDelete).toBeNull();
    });
  });
});
