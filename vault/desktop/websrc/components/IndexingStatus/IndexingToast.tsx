/**
 * IndexingToast - Display indexing status notifications
 *
 * Purpose: Show user-friendly toast messages for file indexing operations.
 * Displays appropriate icons and messages based on indexing status.
 *
 * Status types:
 * - "indexed" / "imported": File successfully imported to library
 * - "already_indexed": File already exists in library (duplicate)
 * - "updated": File content changed and re-indexed
 * - "error": Indexing failed with error message
 *
 * Spec: ZEN_FILE_ARCHITECTURE_SOLUTION.md - Spec 6
 */

import { toast } from '@/stores/toastStore';

export interface IndexingResult {
  status: 'indexed' | 'imported' | 'already_indexed' | 'updated' | 'error';
  fileName: string;
  message?: string;
}

/**
 * Display a toast notification for indexing results
 *
 * @param result - The indexing result with status and file information
 *
 * @example
 * ```typescript
 * // Show success for new file
 * showIndexingToast({
 *   status: 'indexed',
 *   fileName: 'document.pdf'
 * });
 *
 * // Show duplicate message
 * showIndexingToast({
 *   status: 'already_indexed',
 *   fileName: 'existing.txt'
 * });
 *
 * // Show error
 * showIndexingToast({
 *   status: 'error',
 *   fileName: 'invalid.pdf',
 *   message: 'Unsupported file format'
 * });
 * ```
 */
export function showIndexingToast(result: IndexingResult): void {
  const { status, fileName, message } = result;

  switch (status) {
    case 'indexed':
    case 'imported':
      toast.success(`✅ ${fileName}`, {
        message: 'File imported successfully',
      });
      break;

    case 'already_indexed':
      toast.info(`ℹ️ ${fileName}`, {
        message: 'This file is already in your library',
      });
      break;

    case 'updated':
      toast.success(`🔄 ${fileName}`, {
        message: 'File updated with new content',
      });
      break;

    case 'error':
      toast.error(`❌ ${fileName}`, {
        message: message || 'Failed to index file',
      });
      break;

    default:
      // Fallback for unknown status
      toast.info(fileName, {
        message: `Status: ${status}`,
      });
  }
}

/**
 * Display a batch indexing summary toast
 *
 * @param totalFiles - Total number of files processed
 * @param successCount - Number of successfully indexed files
 * @param duplicateCount - Number of duplicate files skipped
 * @param errorCount - Number of files that failed
 *
 * @example
 * ```typescript
 * showBatchIndexingToast(10, 7, 2, 1);
 * // Shows: "7 files imported, 2 duplicates, 1 failed"
 * ```
 */
export function showBatchIndexingToast(
  totalFiles: number,
  successCount: number,
  duplicateCount: number,
  errorCount: number
): void {
  const parts: string[] = [];

  if (successCount > 0) {
    parts.push(`${successCount} imported`);
  }

  if (duplicateCount > 0) {
    parts.push(`${duplicateCount} duplicate${duplicateCount > 1 ? 's' : ''}`);
  }

  if (errorCount > 0) {
    parts.push(`${errorCount} failed`);
  }

  const summary = parts.join(', ');

  if (errorCount === totalFiles) {
    // All failed
    toast.error('Batch import failed', {
      message: `All ${totalFiles} files failed to import`,
    });
  } else if (errorCount === 0 && duplicateCount === 0) {
    // All succeeded
    toast.success('Batch import complete', {
      message: `${successCount} file${successCount > 1 ? 's' : ''} imported successfully`,
    });
  } else {
    // Mixed results
    toast.info('Batch import complete', {
      message: summary,
    });
  }
}
