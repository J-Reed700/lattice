import { type FC, useState, useCallback, useEffect, useMemo, useRef, type DragEvent } from 'react';

import { getCurrentWindow } from '@tauri-apps/api/window';
import { X, ArrowUp, ArrowDown } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { IconButton } from '@/components/ui/IconButton';
import { SettingsRow, settingsFieldClass } from '@/components/ui/SettingsSection';
import { useIndexing } from '@/hooks/useIndexing';
import { useToast } from '@/hooks/useToast';
import VaultAPI from '@/lib/api';
import type { SourceGroup } from '@/lib/bindings';
import { getErrorMessage } from '@/lib/errorUtils';
import { cn } from '@/lib/utils';
import { useConversationsStore } from '@/stores/conversationsStore';
import { useFileBrowserStore } from '@/stores/fileBrowserStore';
import type { CustomCollection } from '@/types/fileBrowser';
import {
  filterIndexablePaths,
  validateIndexablePath,
} from '@/utils/indexingFileValidation';
import { handleAsyncEvent } from '@/utils/promiseHandlers';

import { LibraryFilePicker } from './LibraryFilePicker';
import { RelatedSourceOptions } from './RelatedSourceOptions';

interface FileItem {
  path: string;
  name: string;
  size: number;
  type: string;
  status: 'pending' | 'queued' | 'importing' | 'success' | 'error';
  errorMessage?: string;
  jobId?: string;
}

interface BatchFileImportProps {
  onImportComplete?: (results: { successful: number; failed: number }) => void;
  onClose?: () => void;
  onReviewFailures?: () => void;
}

const isRetryableStatus = (status: FileItem['status']): boolean =>
  status === 'pending' || status === 'error';

export const BatchFileImport: FC<BatchFileImportProps> = ({
  onImportComplete,
  onClose,
  onReviewFailures,
}) => {
  const { startBatchImport, getOperation, operations, historyError } = useIndexing();
  const { toast } = useToast();
  // `useToast()` hands back a fresh object every render; depending on it would
  // recreate `addFilePaths` and re-register the drag-drop listener forever.
  const toastRef = useRef(toast);
  toastRef.current = toast;
  const selectedConversationSpaceId = useConversationsStore((state) => state.selectedSpaceId);
  const customCollections = useFileBrowserStore((state) => state.customCollections);
  const addDocumentsToCustomCollection = useFileBrowserStore(
    (state) => state.addDocumentsToCustomCollection
  );
  const [sourceGroup, setSourceGroup] = useState<SourceGroup | null>(null);
  const [files, setFiles] = useState<FileItem[]>([]);
  const [isImporting, setIsImporting] = useState(false);
  const [currentJobId, setCurrentJobId] = useState<string | null>(null);
  const [isDragging, setIsDragging] = useState(false);
  const [spaces, setSpaces] = useState<
    Array<{
      id: string;
      name: string;
      icon: string | null;
      description: string | null;
      isArchived: boolean;
    }>
  >([]);
  const [selectedSpaceId, setSelectedSpaceId] = useState('');
  const [selectedCollectionId, setSelectedCollectionId] = useState('');
  const [isLoadingSpaces, setIsLoadingSpaces] = useState(false);
  const [hasCustomizedSpaceScope, setHasCustomizedSpaceScope] = useState(false);
  const restoredImport = useRef(false);

  const selectedCount = files.filter((file) => isRetryableStatus(file.status) && !file.jobId).length;
  const canImport = selectedCount > 0 && selectedCount <= 100 && !isImporting && (!sourceGroup || Boolean(sourceGroup.title.trim()));
  const manualCollections = useMemo(() => {
    const byId = new Map(customCollections.map((collection) => [collection.id, collection]));
    const buildLabel = (collection: CustomCollection): string => {
      const labels = [collection.name];
      const seen = new Set<string>([collection.id]);
      let parentId = collection.parentId;
      while (parentId) {
        if (seen.has(parentId)) break;
        const parent = byId.get(parentId);
        if (!parent) break;
        labels.unshift(parent.name);
        seen.add(parentId);
        parentId = parent.parentId;
      }
      return labels.join(' / ');
    };

    return customCollections
      .filter((collection) => collection.kind === 'manual')
      .map((collection) => ({
        id: collection.id,
        label: buildLabel(collection),
      }))
      .sort((a, b) => a.label.localeCompare(b.label));
  }, [customCollections]);
  const buildFileItem = useCallback((path: string): FileItem => {
    const name = path.split(/[\\/]/).pop() || path;
    const extension = name.split('.').pop()?.toLowerCase();

    let type = 'application/octet-stream';
    if (extension === 'pdf') type = 'application/pdf';
    else if (extension === 'docx') {
      type = 'application/vnd.openxmlformats-officedocument.wordprocessingml.document';
    } else if (extension === 'rtf') {
      type = 'application/rtf';
    } else if (extension === 'odt') {
      type = 'application/vnd.oasis.opendocument.text';
    } else if (extension === 'xlsx') {
      type = 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet';
    } else if (extension === 'pptx') {
      type = 'application/vnd.openxmlformats-officedocument.presentationml.presentation';
    } else if (extension === 'txt') type = 'text/plain';
    else if (extension === 'md' || extension === 'markdown') type = 'text/markdown';
    else if (extension === 'html' || extension === 'htm') type = 'text/html';
    else if (extension === 'csv') type = 'text/csv';
    else if (extension === 'tsv') type = 'text/tab-separated-values';
    else if (extension === 'json') type = 'application/json';
    else if (extension === 'xml') type = 'application/xml';

    return {
      path,
      name,
      size: 0,
      type,
      status: 'pending',
    };
  }, []);

  const addFilePaths = useCallback((paths: string[]) => {
    setFiles((prev) => {
      const existing = new Set(prev.map((file) => file.path));
      const { accepted, rejected } = filterIndexablePaths(paths);

      const dedupedAccepted = accepted.filter((path) => {
        if (existing.has(path)) return false;
        existing.add(path);
        return true;
      });
      const next = [...prev, ...dedupedAccepted.map(buildFileItem)];

      if (rejected.length > 0) {
        const rejectedNames = rejected
          .map(({ path }) => path.split(/[\\/]/).pop() || path)
          .slice(0, 3);
        const summary =
          rejected.length > 3
            ? `${rejectedNames.join(', ')} and ${rejected.length - 3} more`
            : rejectedNames.join(', ');
        toastRef.current.error(`Unsupported file type(s): ${summary}`);
      }

      return next;
    });
  }, [buildFileItem]);

  const handleFileSelect = useCallback(async () => {
    try {
      const selectedPaths = await VaultAPI.selectMultipleFiles();
      addFilePaths(selectedPaths);
    } catch (error) {
      toastRef.current.error(`Couldn't choose files: ${getErrorMessage(error)}`);
    }
  }, [addFilePaths]);

  const handleDragOver = useCallback((e: DragEvent) => {
    e.preventDefault();
    setIsDragging(true);
  }, []);

  const handleDragLeave = useCallback((e: DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
  }, []);

  const handleDrop = useCallback(async (e: DragEvent) => {
    e.preventDefault();
    setIsDragging(false);

    const droppedPaths: string[] = [];
    if (e.dataTransfer?.files?.length) {
      for (const file of Array.from(e.dataTransfer.files)) {
        const filePath = (file as File & { path?: string }).path;
        if (filePath) {
          droppedPaths.push(filePath);
        } else {
          const validation = validateIndexablePath(file.name);
          if (!validation.ok) {
            toastRef.current.error(`Unsupported file type: ${file.name}`);
          } else {
            toastRef.current.error(
              `Dropped file "${file.name}" is missing a filesystem path. Please use the file picker.`
            );
          }
        }
      }
    }

    if (droppedPaths.length > 0) {
      addFilePaths(droppedPaths);
    }
  }, [addFilePaths]);

  useEffect(() => {
    let mounted = true;
    let unlisten: (() => void) | null = null;

    Promise.resolve().then(() => getCurrentWindow()
      .onDragDropEvent((event) => {
        if (!mounted) return;

        if (event.payload.type === 'over') {
          setIsDragging(true);
          return;
        }

        if (event.payload.type === 'drop') {
          setIsDragging(false);
          if (event.payload.paths?.length) {
            addFilePaths(event.payload.paths);
          }
          return;
        }

        if (event.payload.type === 'leave') {
          setIsDragging(false);
        }
      }))
      .then((unlistenFn) => {
        unlisten = unlistenFn;
      })
      .catch((error) => {
        console.error('[BatchFileImport] Failed to listen for file drops:', error);
      });

    return () => {
      mounted = false;
      unlisten?.();
    };
  }, [addFilePaths]);

  useEffect(() => {
    let cancelled = false;

    const loadSpaces = async () => {
      setIsLoadingSpaces(true);
      const result = await VaultAPI.listConversationSpaces();
      if (cancelled) return;

      if (result.ok) {
        setSpaces(
          result.data.map((space) => ({
            id: space.id,
            name: space.name,
            icon: space.icon ?? null,
            description: space.description ?? null,
            isArchived: Boolean(space.isArchived),
          }))
        );
      } else {
        setSpaces([]);
      }

      setIsLoadingSpaces(false);
    };

    void loadSpaces();

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (hasCustomizedSpaceScope) {
      return;
    }
    if (!selectedSpaceId && selectedConversationSpaceId) {
      setSelectedSpaceId(selectedConversationSpaceId);
    }
  }, [hasCustomizedSpaceScope, selectedConversationSpaceId, selectedSpaceId]);

  const handleRemoveFile = useCallback((path: string) => {
    setFiles((prev) => prev.filter((f) => f.path !== path));
  }, []);

  const handleStartImport = useCallback(async () => {
    if (!canImport) return;

    try {
      setIsImporting(true);
      const retryableFiles = files.filter((file) => isRetryableStatus(file.status) && !file.jobId);
      const filePaths = retryableFiles.map((file) => file.path);
      const result = sourceGroup
        ? await startBatchImport(filePaths, selectedSpaceId || undefined, { sourceGroup })
        : await startBatchImport(filePaths, selectedSpaceId || undefined);

      if (result.ok && result.data) {
        setCurrentJobId(result.data);

        setFiles((prev) =>
          prev.map((f) =>
            isRetryableStatus(f.status) && !f.jobId ? { ...f, jobId: result.data, status: 'queued' as const, errorMessage: undefined } : f
          )
        );
      } else {
        const detail: unknown = !result.ok ? result.details?.details : undefined;
        throw new Error(typeof detail === 'string' && detail.trim() ? detail : result.ok ? 'Import did not return a job ID' : result.error);
      }
    } catch (error) {
      console.error('[BatchFileImport] Import failed:', error);
      const errorMsg = getErrorMessage(error);
      setIsImporting(false);
      setFiles((prev) =>
        prev.map((f) => f.jobId || f.status === 'success' ? f : {
          ...f,
          status: 'error' as const,
          errorMessage: errorMsg,
        })
      );
    }
  }, [
    canImport,
    files,
    selectedSpaceId,
    sourceGroup,
    startBatchImport,
  ]);

  const startNewImport = useCallback(() => {
    // History owns submitted jobs. Dismissing this preview must not immediately
    // restore the same failed job or reuse its source identity and positions.
    restoredImport.current = true;
    setFiles(current => current.filter(file => !file.jobId));
    setCurrentJobId(null);
    setSourceGroup(null);
  }, []);

  // Cancel import
  const handleCancel = useCallback(() => {
    if (onClose) {
      onClose();
    } else {
      startNewImport();
      setFiles([]);
    }
  }, [onClose, startNewImport]);

  // Restore failures as well as active work after navigation or a restart.
  useEffect(() => {
    if (restoredImport.current || currentJobId || isImporting || files.length > 0) return;
    const active = Array.from(operations.values()).find((operation) =>
      (['pending', 'processing'].includes(operation.status) || operation.failedFiles > 0) && operation.items?.length
    );
    if (!active?.items) return;
    restoredImport.current = true;
    setFiles(active.items.map((item) => ({ ...buildFileItem(item.target || item.url || ''), jobId: active.id })));
    setCurrentJobId(active.id);
    setIsImporting(true);
  }, [operations, currentJobId, isImporting, files.length, buildFileItem]);

  // Watch operation progress and completion
  useEffect(() => {
    if (!currentJobId) return;

    const operation = getOperation(currentJobId);
    if (!operation) return;


    if (operation.items && operation.items.length > 0) {
      setFiles((prev) =>
        prev.map((file) => {
          const item = operation.items?.find(
            (entry) => (entry.target ?? entry.url) === file.path
          );
          if (!item) return file;

          const normalized = item.status.toLowerCase();
          const nextStatus: FileItem['status'] =
            normalized === 'completed'
              ? 'success'
              : normalized === 'failed'
                ? 'error'
                : normalized === 'pending'
                  ? 'queued'
                  : 'importing';

          return {
            ...file,
            status: nextStatus,
            errorMessage: item.errorMessage || (nextStatus === 'error' ? 'Import failed' : undefined),
          };
        })
      );
    } else {
      // Aggregate counts cannot identify which files succeeded. Only mark all
      // successful when the entire job succeeded; otherwise retain uncertainty.
      setFiles((prev) => {
        if (operation.status !== 'completed' || operation.failedFiles > 0 || operation.successfulFiles !== operation.totalFiles) return prev;
        const updated = [...prev];
        const processedCount = operation.processedFiles;

        for (let i = 0; i < processedCount && i < updated.length; i++) {
          if (['importing', 'queued'].includes(updated[i].status)) {
            updated[i] = { ...updated[i], status: 'success' };
          }
        }

        return updated;
      });
    }

    if (['completed', 'error'].includes(operation.status) && selectedCollectionId) {
      const documentIds = operation.items?.filter(item => item.status === 'completed' && item.documentId)
        .map(item => item.documentId as string) ?? [];
      if (documentIds.length) addDocumentsToCustomCollection(selectedCollectionId, documentIds);
    }

    if (operation.status === 'completed') {
      console.log('[BatchFileImport] Job complete:', {
        status: operation.status,
        successful: operation.successfulFiles,
        failed: operation.failedFiles,
      });

      setIsImporting(false);
      setCurrentJobId(null);

      onImportComplete?.({
        successful: operation.successfulFiles,
        failed: operation.failedFiles,
      });
    } else if (operation.status === 'error') {
      console.error('[BatchFileImport] Job failed:', operation.error);

      setIsImporting(false);
      setCurrentJobId(null);

      // Mark all remaining as error
      setFiles((prev) =>
        prev.map((f) => ({
          ...f,
          status: ['importing', 'queued'].includes(f.status) ? 'error' as const : f.status,
          errorMessage: ['importing', 'queued'].includes(f.status) ? operation.error || 'Import failed' : f.errorMessage,
        }))
      );

      onImportComplete?.({
        successful: operation.successfulFiles,
        failed: operation.failedFiles,
      });
    }
  }, [currentJobId, getOperation, onImportComplete, selectedCollectionId, addDocumentsToCustomCollection]);

  return (
    <div>
      {historyError && <p role="alert" className="mb-4 text-sm text-text-secondary">Previous import results could not be loaded: {historyError}. Open History to refresh their status.</p>}
      <div
        className={cn(
          'rounded-md border border-dashed py-10 text-center transition-colors duration-fast',
          isDragging ? 'border-accent' : 'border-border-default'
        )}
        onDrop={handleAsyncEvent(handleDrop)}
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
      >
        <p className="text-sm text-text-secondary">
          Drop files here, or{' '}
          <button
            type="button"
            onClick={handleAsyncEvent(handleFileSelect)}
            disabled={isImporting}
            className="text-accent transition-colors duration-fast hover:underline disabled:cursor-not-allowed disabled:opacity-50"
          >
            choose files
          </button>
        </p>
        <p className="mt-1 text-xs text-text-muted">
          PDF, DOCX, RTF, ODT, XLSX, PPTX, TXT, MD, HTML, CSV, JSON, code · up to 50 MB each
        </p>
      </div>

      <div className="mt-8 border-t border-border-subtle">
        <SettingsRow label="Add to space" htmlFor="batch-file-space-select">
          <select
            id="batch-file-space-select"
            value={selectedSpaceId}
            onChange={(event) => {
              setHasCustomizedSpaceScope(true);
              setSelectedSpaceId(event.target.value);
            }}
            disabled={isImporting || isLoadingSpaces}
            className={settingsFieldClass}
          >
            <option value="">None</option>
            {spaces.map((space) => (
              <option key={space.id} value={space.id}>
                {space.icon ? `${space.icon} ` : ''}{space.name}{space.isArchived ? ' (archived)' : ''}
              </option>
            ))}
          </select>
        </SettingsRow>

        <SettingsRow
          label="Add to collection"
          htmlFor={manualCollections.length > 0 ? 'batch-file-collection-select' : undefined}
        >
          {manualCollections.length === 0 ? (
            <span className="text-xs text-text-muted">No collections yet</span>
          ) : (
            <select
              id="batch-file-collection-select"
              value={selectedCollectionId}
              onChange={(event) => setSelectedCollectionId(event.target.value)}
              disabled={isImporting}
              className={settingsFieldClass}
            >
              <option value="">None</option>
              {manualCollections.map((collection) => (
                <option key={collection.id} value={collection.id}>
                  {collection.label}
                </option>
              ))}
            </select>
          )}
        </SettingsRow>
      </div>

      <LibraryFilePicker disabled={isImporting} onAdd={addFilePaths} />
      {!isImporting && files.some(file => Boolean(file.jobId)) && (
        <div className="mt-4 space-y-1">
          <Button variant="secondary" onClick={startNewImport}>Start a new import</Button>
          <p className="text-xs text-text-muted">Configure a different book or source. Previous import results and retries stay in History.</p>
        </div>
      )}
      <RelatedSourceOptions value={sourceGroup} onChange={setSourceGroup} disabled={isImporting || files.some(file => Boolean(file.jobId))} />
      {selectedCount > 100 && <p role="alert" className="mt-2 text-sm text-danger-fg">Select up to 100 files per import.</p>}

      {isImporting && currentJobId && (() => {
        const operation = getOperation(currentJobId);
        const progress = operation?.totalFiles
          ? Math.round((operation.processedFiles / operation.totalFiles) * 100)
          : 0;

        return (
          <div className="mt-4 space-y-1 text-sm text-text-muted" role="status">
            <p className="tabular-nums">
              {operation?.processedFiles ?? 0} of {operation?.totalFiles ?? files.length} files processed · {progress}%
            </p>
            <p>
              {operation?.currentFile
                ? `Processing ${operation.currentFile.split(/[\\/]/).pop()} — extracting text and building search index`
                : 'Preparing PDFs and building search indexes…'}
            </p>
            {operation?.error && <p role="alert">{operation.error}</p>}
            {Boolean(operation?.failedFiles) && <p>{operation?.failedFiles} failed</p>}
          </div>
        );
      })()}

      {files.some(file => file.status === 'error' && file.jobId) && (
        <div role="status" className="mt-4 text-sm text-text-secondary">
          <p>{files.filter(file => file.status === 'error').length} {files.filter(file => file.status === 'error').length === 1 ? 'file failed' : 'files failed'}. Details are saved in History.</p>
          <button type="button" onClick={onReviewFailures} className="mt-1 text-accent underline underline-offset-2">
            Retry or replace failed files
          </button>
        </div>
      )}

      {files.length > 0 && (
        <div className="mt-8">
          <h2 className="pb-2 text-base font-medium text-text-primary">
            {files.length} {files.length === 1 ? 'file' : 'files'}{sourceGroup?.ordered ? ' · reading order' : ''}
          </h2>
          <div className="flex items-center gap-2">
            {sourceGroup?.ordered && <Button variant="ghost" disabled={isImporting || files.some(file => Boolean(file.jobId))} onClick={() => setFiles(current => [...current].sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true })))}>Sort by filename</Button>}
            {files.length > 1 && (
              <Button
                variant="ghost"
                disabled={isImporting || files.some(file => Boolean(file.jobId))}
                onClick={() => setFiles(current => [...current].reverse())}
              >
                Reverse order
              </Button>
            )}
          </div>
          <div className="border-t border-border-subtle">
            {files.map((file, index) => (
              <FileListItem
                key={file.path}
                file={file}
                position={sourceGroup?.ordered ? index : undefined}
                onMove={sourceGroup?.ordered && !files.some(item => item.jobId) ? (direction) => setFiles(current => {
                  const next = [...current]; const target = index + direction;
                  if (target < 0 || target >= next.length) return current;
                  [next[index], next[target]] = [next[target], next[index]];
                  return next;
                }) : undefined}
                isLast={index === files.length - 1}
                onRemove={handleRemoveFile}
                disabled={isImporting}
              />
            ))}
          </div>
        </div>
      )}



      <div className="mt-6 flex shrink-0 items-center justify-end gap-2">
        <Button variant="ghost" onClick={handleCancel} disabled={isImporting} className="h-9">
          Cancel
        </Button>
        <Button onClick={handleAsyncEvent(handleStartImport)} disabled={!canImport} className="h-9">
          {isImporting
            ? 'Importing…'
            : selectedCount > 0
              ? `${files.some(file => file.status === 'error' && !file.jobId) ? 'Retry' : 'Import'} ${selectedCount} ${selectedCount === 1 ? 'file' : 'files'}`
              : 'Import'}
        </Button>
      </div>
    </div>
  );
};

interface FileListItemProps {
  position?: number;
  onMove?: (direction: number) => void;
  isLast?: boolean;
  file: FileItem;
  onRemove: (path: string) => void;
  disabled?: boolean;
}

/** Status as plain muted text. No badges, no colored dots. */
const statusLabel = (file: FileItem): string => {
  switch (file.status) {
    case 'queued':
      return 'Queued';
    case 'importing':
      return 'Processing…';
    case 'success':
      return 'Imported';
    case 'error':
      return file.errorMessage ? `Failed — ${file.errorMessage}` : 'Failed';
    default:
      return '';
  }
};

const formatSize = (bytes: number): string => {
  const units = ['B', 'KB', 'MB', 'GB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value < 10 && unit > 0 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
};

const FileListItem: FC<FileListItemProps> = ({ file, onRemove, disabled, position, onMove, isLast }) => {
  const meta = file.size > 0
    ? formatSize(file.size)
    : file.path.slice(0, file.path.length - file.name.length).replace(/[\\/]$/, '');
  const status = statusLabel(file);

  return (
    <div className="flex items-center gap-3 border-b border-border-subtle py-3">
      {position !== undefined && <span className="text-xs tabular-nums text-text-muted">{position + 1}</span>}
      {onMove && <div className="flex shrink-0">
        <IconButton label={`Move ${file.name} up`} disabled={disabled || position === 0} onClick={() => onMove(-1)}><ArrowUp /></IconButton>
        <IconButton label={`Move ${file.name} down`} disabled={disabled || isLast} onClick={() => onMove(1)}><ArrowDown /></IconButton>
      </div>}
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm text-text-primary">{file.name}</div>
        {meta && <div className="truncate text-xs text-text-muted">{meta}</div>}
        {file.status === 'error' && (
          <p className="mt-1 break-words text-xs text-danger-fg">{status}</p>
        )}
      </div>
      {status && file.status !== 'error' && (
        <span className="max-w-[240px] shrink-0 truncate text-xs text-text-muted">{status}</span>
      )}
      {!disabled && (file.status === 'pending' || file.status === 'error') && (
        <IconButton label="Remove file" onClick={() => onRemove(file.path)} className="shrink-0">
          <X />
        </IconButton>
      )}
    </div>
  );
};
