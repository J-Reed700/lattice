import { type FC, useState, useCallback, useEffect, useMemo, useRef, type DragEvent } from 'react';

import { getCurrentWindow } from '@tauri-apps/api/window';
import { X } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { IconButton } from '@/components/ui/IconButton';
import { SettingsRow, settingsFieldClass } from '@/components/ui/SettingsSection';
import { useIndexing } from '@/hooks/useIndexing';
import { useToast } from '@/hooks/useToast';
import VaultAPI from '@/lib/api';
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

interface FileItem {
  path: string;
  name: string;
  size: number;
  type: string;
  status: 'pending' | 'importing' | 'success' | 'error';
  errorMessage?: string;
}

interface BatchFileImportProps {
  onImportComplete?: (results: { successful: number; failed: number }) => void;
  onClose?: () => void;
}

const isRetryableStatus = (status: FileItem['status']): boolean =>
  status === 'pending' || status === 'error';

export const BatchFileImport: FC<BatchFileImportProps> = ({
  onImportComplete,
  onClose,
}) => {
  const { startBatchImport, getOperation } = useIndexing();
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

  const selectedCount = files.filter((file) => isRetryableStatus(file.status)).length;
  const canImport = selectedCount > 0 && !isImporting;
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
    const name = path.split('/').pop() || path.split('\\').pop() || path;
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

      const dedupedAccepted = accepted.filter((path) => !existing.has(path));
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

  // Handle file selection via dialog
  const handleFileSelect = useCallback(async () => {
    try {
      const selectedPaths = await VaultAPI.selectMultipleFiles();
      addFilePaths(selectedPaths);
    } catch (error) {
      console.error('[BatchFileImport] File selection failed:', error);
    }
  }, [addFilePaths]);

  // Handle drag over
  const handleDragOver = useCallback((e: DragEvent) => {
    e.preventDefault();
    setIsDragging(true);
  }, []);

  // Handle drag leave
  const handleDragLeave = useCallback((e: DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
  }, []);

  // Handle drop
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

    getCurrentWindow()
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
      })
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

  // Remove file from list
  const handleRemoveFile = useCallback((path: string) => {
    setFiles((prev) => prev.filter((f) => f.path !== path));
  }, []);

  // Start batch import
  const handleStartImport = useCallback(async () => {
    if (!canImport) return;

    try {
      setIsImporting(true);
      const retryableFiles = files.filter((file) => isRetryableStatus(file.status));
      const filePaths = retryableFiles.map((file) => file.path);
      const shouldUseDirectIndexing =
        retryableFiles.length === 1 || Boolean(selectedSpaceId) || Boolean(selectedCollectionId);

      // Single file should use the direct indexing path, not batch job orchestration.
      // This avoids unnecessary batch transaction overhead and improves error clarity.
      if (shouldUseDirectIndexing) {
        let successCount = 0;
        let failedCount = 0;
        const indexedDocumentIds: string[] = [];

        for (const target of retryableFiles) {
          setFiles((prev) =>
            prev.map((file) =>
              file.path === target.path
                ? { ...file, status: 'importing' as const, errorMessage: undefined }
                : file
            )
          );

          const result = selectedSpaceId
            ? await VaultAPI.indexFile(target.path, selectedSpaceId)
            : await VaultAPI.indexFile(target.path);

          if (result.ok) {
            successCount += 1;
            if (result.data.documentId) {
              indexedDocumentIds.push(result.data.documentId);
            }

            setFiles((prev) =>
              prev.map((file) =>
                file.path === target.path
                  ? { ...file, status: 'success' as const, errorMessage: undefined }
                  : file
              )
            );
          } else {
            failedCount += 1;
            const errorMsg = result.error || 'Failed to index file';
            setFiles((prev) =>
              prev.map((file) =>
                file.path === target.path
                  ? { ...file, status: 'error' as const, errorMessage: errorMsg }
                  : file
              )
            );
          }
        }

        if (selectedCollectionId && indexedDocumentIds.length > 0) {
          addDocumentsToCustomCollection(selectedCollectionId, indexedDocumentIds);
          const collectionName =
            manualCollections.find((collection) => collection.id === selectedCollectionId)?.label ??
            'selected collection';
          toastRef.current.success(
            `Added ${indexedDocumentIds.length} ${indexedDocumentIds.length === 1 ? 'document' : 'documents'} to ${collectionName}`
          );
        }

        setIsImporting(false);
        setCurrentJobId(null);
        onImportComplete?.({ successful: successCount, failed: failedCount });
        return;
      }

      const result = await startBatchImport(filePaths);

      if (result.ok && result.data) {
        setCurrentJobId(result.data);

        // Update pending files to "importing"
        setFiles((prev) =>
          prev.map((f) =>
            isRetryableStatus(f.status) ? { ...f, status: 'importing' as const } : f
          )
        );
      } else {
        throw new Error('Import failed');
      }
    } catch (error) {
      console.error('[BatchFileImport] Import failed:', error);
      const errorMsg = getErrorMessage(error);
      setIsImporting(false);
      setFiles((prev) =>
        prev.map((f) => ({
          ...f,
          status: 'error' as const,
          errorMessage: errorMsg,
        }))
      );
    }
  }, [
    addDocumentsToCustomCollection,
    canImport,
    files,
    manualCollections,
    onImportComplete,
    selectedCollectionId,
    selectedSpaceId,
    startBatchImport,
  ]);

  // Cancel import
  const handleCancel = useCallback(() => {
    if (onClose) {
      onClose();
    } else {
      setFiles([]);
    }
  }, [onClose]);

  // Watch operation progress and completion
  useEffect(() => {
    if (!currentJobId) return;

    const operation = getOperation(currentJobId);
    if (!operation) return;

    // Calculate progress (available for future use)
    // const progress = operation.totalFiles > 0
    //   ? Math.round((operation.processedFiles / operation.totalFiles) * 100)
    //   : 0;

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
                : 'importing';

          return {
            ...file,
            status: nextStatus,
            errorMessage: item.errorMessage || (nextStatus === 'error' ? 'Import failed' : undefined),
          };
        })
      );
    } else {
      // Fallback: Update file statuses based on aggregate progress
      setFiles((prev) => {
        const updated = [...prev];
        const processedCount = operation.processedFiles;

        for (let i = 0; i < processedCount && i < updated.length; i++) {
          if (updated[i].status === 'importing') {
            updated[i] = { ...updated[i], status: 'success' };
          }
        }

        return updated;
      });
    }

    // Handle completion
    if (operation.status === 'completed') {
      console.log('[BatchFileImport] Job complete:', {
        status: operation.status,
        successful: operation.processedFiles,
        failed: operation.totalFiles - operation.processedFiles,
      });

      setIsImporting(false);
      setCurrentJobId(null);

      onImportComplete?.({
        successful: operation.processedFiles,
        failed: operation.totalFiles - operation.processedFiles,
      });
    } else if (operation.status === 'error') {
      console.error('[BatchFileImport] Job failed:', operation.error);

      setIsImporting(false);
      setCurrentJobId(null);

      // Mark all remaining as error
      setFiles((prev) =>
        prev.map((f) => ({
          ...f,
          status: f.status === 'importing' ? 'error' as const : f.status,
          errorMessage: f.status === 'importing' ? operation.error || 'Import failed' : f.errorMessage,
        }))
      );

      onImportComplete?.({
        successful: operation.processedFiles,
        failed: operation.totalFiles - operation.processedFiles,
      });
    }
  }, [currentJobId, getOperation, onImportComplete]);

  return (
    <div>
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

      {files.length > 0 && (
        <div className="mt-8">
          <h2 className="pb-2 text-base font-medium text-text-primary">
            {files.length} {files.length === 1 ? 'file' : 'files'}
          </h2>
          <div className="border-t border-border-subtle">
            {files.map((file) => (
              <FileListItem
                key={file.path}
                file={file}
                onRemove={handleRemoveFile}
                disabled={isImporting}
              />
            ))}
          </div>
        </div>
      )}

      {isImporting && currentJobId && (() => {
        const operation = getOperation(currentJobId);
        const progress = operation?.totalFiles
          ? Math.round((operation.processedFiles / operation.totalFiles) * 100)
          : 0;

        return (
          <p className="mt-4 text-sm tabular-nums text-text-muted">Importing… {progress}%</p>
        );
      })()}

      <div className="mt-6 flex shrink-0 items-center justify-end gap-2">
        <Button variant="ghost" onClick={handleCancel} disabled={isImporting} className="h-9">
          Cancel
        </Button>
        <Button onClick={handleAsyncEvent(handleStartImport)} disabled={!canImport} className="h-9">
          {isImporting
            ? 'Importing…'
            : selectedCount > 0
              ? `Import ${selectedCount} ${selectedCount === 1 ? 'file' : 'files'}`
              : 'Import'}
        </Button>
      </div>
    </div>
  );
};

interface FileListItemProps {
  file: FileItem;
  onRemove: (path: string) => void;
  disabled?: boolean;
}

/** Status as plain muted text. No badges, no colored dots. */
const statusLabel = (file: FileItem): string => {
  switch (file.status) {
    case 'importing':
      return 'Importing…';
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

const FileListItem: FC<FileListItemProps> = ({ file, onRemove, disabled }) => {
  const meta = file.size > 0
    ? formatSize(file.size)
    : file.path.slice(0, file.path.length - file.name.length).replace(/[\\/]$/, '');
  const status = statusLabel(file);

  return (
    <div className="flex items-center gap-3 border-b border-border-subtle py-3">
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm text-text-primary">{file.name}</div>
        {meta && <div className="truncate text-xs text-text-muted">{meta}</div>}
      </div>
      {status && (
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
