import { type FC, useState, useCallback, useEffect, useMemo, type DragEvent } from 'react';

import { getCurrentWindow } from '@tauri-apps/api/window';
import { Upload, X, FileText, CheckCircle, XCircle, Loader2, Layers3, FolderTree } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Progress } from '@/components/ui/progress';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
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

const ALL_SPACES_OPTION = '__all_spaces__';
const NO_COLLECTION_OPTION = '__no_collection__';

export const BatchFileImport: FC<BatchFileImportProps> = ({
  onImportComplete,
  onClose,
}) => {
  const { startBatchImport, getOperation } = useIndexing();
  const { toast } = useToast();
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
  const selectedSpace = useMemo(
    () => spaces.find((space) => space.id === selectedSpaceId) ?? null,
    [selectedSpaceId, spaces]
  );
  const selectedCollection = useMemo(
    () => manualCollections.find((collection) => collection.id === selectedCollectionId) ?? null,
    [manualCollections, selectedCollectionId]
  );

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
        toast.error(`Unsupported file type(s): ${summary}`);
      }

      return next;
    });
  }, [buildFileItem, toast]);

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
            toast.error(`Unsupported file type: ${file.name}`);
          } else {
            toast.error(
              `Dropped file "${file.name}" is missing a filesystem path. Please use the file picker.`
            );
          }
        }
      }
    }

    if (droppedPaths.length > 0) {
      addFilePaths(droppedPaths);
    }
  }, [addFilePaths, toast]);

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
          toast.success(
            `${indexedDocumentIds.length} document${indexedDocumentIds.length === 1 ? '' : 's'} added to ${collectionName}`
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
    toast,
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
    <div className="flex h-full min-h-0 flex-col gap-4 p-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <h2 className="text-2xl font-bold">Import Files</h2>
        {onClose && (
          <Button variant="ghost" size="icon" onClick={onClose}>
            <X className="h-4 w-4" />
          </Button>
        )}
      </div>

      {/* Drag & Drop Zone */}
      <div
        className={cn(
          'border-2 border-dashed rounded-lg p-8 text-center transition-colors',
          isDragging
            ? 'border-primary bg-primary/10'
            : 'border-muted-foreground/25 hover:border-muted-foreground/50'
        )}
        onDrop={handleAsyncEvent(handleDrop)}
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
      >
        <Upload className="mx-auto h-12 w-12 text-muted-foreground mb-4" />
        <p className="text-lg mb-2">Drag & drop files here</p>
        <p className="text-sm text-muted-foreground mb-4">
          or click the button below to select files
        </p>
        <Button onClick={handleAsyncEvent(handleFileSelect)} disabled={isImporting}>
          <Upload className="mr-2 h-4 w-4" />
          Select Files
        </Button>
        <p className="text-xs text-muted-foreground mt-2">
        Supported: PDF, DOCX, RTF, ODT, XLSX, PPTX, TXT, MD, HTML, CSV, JSON, and code/config
        files (max 50MB per file)
      </p>
      </div>

      <div className="grid gap-3 md:grid-cols-2">
        <div className="rounded-xl border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] p-3 shadow-[var(--shadow-sm)]">
          <div className="mb-2 flex items-start justify-between gap-2">
            <div className="min-w-0">
              <p className="text-xs font-medium uppercase tracking-wide text-[hsl(var(--text-tertiary))]">
                Optional Space Scope
              </p>
              <p className="mt-1 truncate text-sm font-medium text-[hsl(var(--text-primary))]">
                {selectedSpace
                  ? `${selectedSpace.icon ? `${selectedSpace.icon} ` : ''}${selectedSpace.name}`
                  : (isLoadingSpaces ? 'Loading spaces...' : 'All Spaces')}
              </p>
              <p className="mt-0.5 truncate text-[11px] text-[hsl(var(--text-secondary))]">
                {selectedSpace?.description || 'Applies this import to one conversation space'}
              </p>
            </div>
            <div className="flex items-center gap-1">
              {selectedSpaceId && (
                <button
                  type="button"
                  onClick={() => {
                    setHasCustomizedSpaceScope(true);
                    setSelectedSpaceId('');
                  }}
                  disabled={isImporting}
                  className="rounded-md border border-[hsl(var(--border-subtle))] px-2 py-1 text-[11px] text-[hsl(var(--text-secondary))] transition-colors hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))] disabled:cursor-not-allowed disabled:opacity-60"
                >
                  Clear
                </button>
              )}
              <span className="inline-flex h-8 w-8 items-center justify-center rounded-md border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))] text-[hsl(var(--text-secondary))]">
                <Layers3 className="h-4 w-4" />
              </span>
            </div>
          </div>
          <Select
            value={selectedSpaceId || ALL_SPACES_OPTION}
            onValueChange={(value) => {
              setHasCustomizedSpaceScope(true);
              setSelectedSpaceId(value === ALL_SPACES_OPTION ? '' : value);
            }}
            disabled={isImporting || isLoadingSpaces}
          >
            <SelectTrigger
              id="batch-file-space-select"
              aria-label="Optional Space Scope"
              className="h-10 border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))] text-[hsl(var(--text-primary))] data-[placeholder]:text-[hsl(var(--text-secondary))]"
            >
              <SelectValue placeholder={isLoadingSpaces ? 'Loading spaces...' : 'All Spaces'} />
            </SelectTrigger>
            <SelectContent className="border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] text-[hsl(var(--text-primary))]">
              <SelectItem value={ALL_SPACES_OPTION}>All Spaces (No scope)</SelectItem>
              {spaces.map((space) => (
                <SelectItem key={space.id} value={space.id}>
                  {space.icon ? `${space.icon} ` : ''}{space.name}{space.isArchived ? ' (archived)' : ''}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <div className="rounded-xl border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] p-3 shadow-[var(--shadow-sm)]">
          <div className="mb-2 flex items-start justify-between gap-2">
            <div className="min-w-0">
              <p className="text-xs font-medium uppercase tracking-wide text-[hsl(var(--text-tertiary))]">
                Optional Collection
              </p>
              <p className="mt-1 truncate text-sm font-medium text-[hsl(var(--text-primary))]">
                {selectedCollection?.label ?? (manualCollections.length === 0 ? 'No collections available' : 'No Collection')}
              </p>
              <p className="mt-0.5 text-[11px] text-[hsl(var(--text-secondary))]">
                Auto-add imported docs to a curated collection
              </p>
            </div>
            <div className="flex items-center gap-1">
              {selectedCollectionId && (
                <button
                  type="button"
                  onClick={() => setSelectedCollectionId('')}
                  disabled={isImporting}
                  className="rounded-md border border-[hsl(var(--border-subtle))] px-2 py-1 text-[11px] text-[hsl(var(--text-secondary))] transition-colors hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))] disabled:cursor-not-allowed disabled:opacity-60"
                >
                  Clear
                </button>
              )}
              <span className="inline-flex h-8 w-8 items-center justify-center rounded-md border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))] text-[hsl(var(--text-secondary))]">
                <FolderTree className="h-4 w-4" />
              </span>
            </div>
          </div>
          <Select
            value={selectedCollectionId || NO_COLLECTION_OPTION}
            onValueChange={(value) =>
              setSelectedCollectionId(value === NO_COLLECTION_OPTION ? '' : value)
            }
            disabled={isImporting || manualCollections.length === 0}
          >
            <SelectTrigger
              id="batch-file-collection-select"
              aria-label="Optional Collection"
              className="h-10 border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))] text-[hsl(var(--text-primary))] data-[placeholder]:text-[hsl(var(--text-secondary))]"
            >
              <SelectValue
                placeholder={manualCollections.length === 0 ? 'No collections available' : 'No Collection'}
              />
            </SelectTrigger>
            <SelectContent className="border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] text-[hsl(var(--text-primary))]">
              <SelectItem value={NO_COLLECTION_OPTION}>
                {manualCollections.length === 0 ? 'No collections available' : 'No Collection'}
              </SelectItem>
              {manualCollections.map((collection) => (
                <SelectItem key={collection.id} value={collection.id}>
                  {collection.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </div>

      {/* File List */}
      {files.length > 0 && (
        <div className="flex min-h-0 flex-1 flex-col gap-2">
          <h3 className="font-semibold">
            Files to Import ({selectedCount})
          </h3>
          <div className="min-h-[8rem] flex-1 overflow-y-auto rounded-lg border divide-y">
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

      {/* Progress */}
      {isImporting && currentJobId && (() => {
        const operation = getOperation(currentJobId);
        const progress = operation?.totalFiles
          ? Math.round((operation.processedFiles / operation.totalFiles) * 100)
          : 0;

        return (
          <div className="space-y-2">
            <div className="flex items-center justify-between text-sm">
              <span>Importing files...</span>
              <span>{progress}%</span>
            </div>
            <Progress value={progress} />
          </div>
        );
      })()}

      {/* Actions */}
      <div className="flex shrink-0 justify-end gap-2">
        <Button variant="outline" onClick={handleCancel} disabled={isImporting}>
          Cancel
        </Button>
        <Button onClick={handleAsyncEvent(handleStartImport)} disabled={!canImport}>
          {isImporting ? (
            <>
              <Loader2 className="mr-2 h-4 w-4 animate-spin" />
              Importing...
            </>
          ) : (
            <>Import {selectedCount} {selectedCount === 1 ? 'File' : 'Files'}</>
          )}
        </Button>
      </div>
    </div>
  );
};

// File List Item Component
interface FileListItemProps {
  file: FileItem;
  onRemove: (path: string) => void;
  disabled?: boolean;
}

const FileListItem: FC<FileListItemProps> = ({ file, onRemove, disabled }) => {
  const StatusIcon = {
    pending: FileText,
    importing: Loader2,
    success: CheckCircle,
    error: XCircle,
  }[file.status];

  const statusColor = {
    pending: 'text-muted-foreground',
    importing: 'text-[hsl(var(--accent))]',
    success: 'text-[hsl(var(--success-fg))]',
    error: 'text-[hsl(var(--danger-fg))]',
  }[file.status];

  return (
    <div className="flex items-center gap-3 p-3">
      <StatusIcon
        className={cn(
          'h-5 w-5',
          statusColor,
          file.status === 'importing' && 'animate-spin'
        )}
      />
      <div className="flex-1 min-w-0">
        <p className="font-medium truncate">{file.name}</p>
        {file.errorMessage && (
          <p className="text-sm text-[hsl(var(--danger-fg))]">{file.errorMessage}</p>
        )}
      </div>
      {!disabled && (file.status === 'pending' || file.status === 'error') && (
        <Button
          variant="ghost"
          size="icon"
          onClick={() => onRemove(file.path)}
        >
          <X className="h-4 w-4" />
        </Button>
      )}
    </div>
  );
};
