/**
 * UploadWithProgress Component
 *
 * Enhanced upload component with integrated progress tracking.
 * Shows real-time progress for file uploads and batch operations.
 */

import { useState, useCallback, useRef, useEffect, useMemo } from 'react';

import { useProgress } from '../../hooks/useProgressListener';
import VaultAPI from '../../lib/api';
import { useFileBrowserStore } from '../../stores/fileBrowserStore';
import { useConversationsStore } from '../../stores/conversationsStore';
import type { CustomCollection } from '../../types/fileBrowser';
import { useProgressStore } from '../../stores/progressStore';
import { showIndexingToast } from '../IndexingStatus';
import { MiniProgress } from '../Progress';

interface UploadWithProgressProps {
  onUploadComplete?: () => void;
}

export default function UploadWithProgress({ onUploadComplete }: UploadWithProgressProps) {
  const selectedConversationSpaceId = useConversationsStore((state) => state.selectedSpaceId);
  const customCollections = useFileBrowserStore((state) => state.customCollections);
  const addDocumentsToCustomCollection = useFileBrowserStore(
    (state) => state.addDocumentsToCustomCollection
  );
  const [isUploading, setIsUploading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [dragActive, setDragActive] = useState(false);
  const [spaces, setSpaces] = useState<Array<{ id: string; name: string; isArchived: boolean }>>(
    []
  );
  const [selectedSpaceId, setSelectedSpaceId] = useState('');
  const [selectedCollectionId, setSelectedCollectionId] = useState('');
  const [isLoadingSpaces, setIsLoadingSpaces] = useState(false);
  const timeoutRef = useRef<NodeJS.Timeout | null>(null);
  const uploadProgress = useProgress('upload', true);
  const { toggleCollapsed } = useProgressStore();
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

  const addIndexedDocsToCollection = useCallback((documentIds: string[]) => {
    if (!selectedCollectionId) return;
    const uniqueDocumentIds = Array.from(
      new Set(documentIds.filter((documentId) => typeof documentId === 'string' && documentId))
    );
    if (uniqueDocumentIds.length === 0) return;
    addDocumentsToCustomCollection(selectedCollectionId, uniqueDocumentIds);
  }, [addDocumentsToCustomCollection, selectedCollectionId]);

  // Cleanup timeout on unmount
  useEffect(() => () => {
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }
    }, []);

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
    if (!selectedSpaceId && selectedConversationSpaceId) {
      setSelectedSpaceId(selectedConversationSpaceId);
    }
  }, [selectedConversationSpaceId, selectedSpaceId]);

  const handleFileUpload = useCallback(
    async (filePath: string, fileName: string) => {
      const progressId = uploadProgress.start(`Uploading ${fileName}`, 1);

      try {
        const result = selectedSpaceId
          ? await VaultAPI.indexFile(filePath, selectedSpaceId)
          : await VaultAPI.indexFile(filePath);

        if (result.ok) {
          addIndexedDocsToCollection([result.data.documentId]);
          uploadProgress.complete(progressId, `${fileName} uploaded successfully`);

          showIndexingToast({
            status: result.data.status,
            fileName,
            message: result.data.error || undefined
          });

          onUploadComplete?.();
        } else {
          uploadProgress.fail(progressId, result.error);

          showIndexingToast({
            status: 'error',
            fileName,
            message: result.error
          });

          setError(result.error);
          if (timeoutRef.current) clearTimeout(timeoutRef.current);
          timeoutRef.current = setTimeout(() => setError(null), 5000);
        }
      } catch (err) {
        const errorMsg = err instanceof Error ? err.message : 'Upload failed';
        uploadProgress.fail(progressId, errorMsg);

        showIndexingToast({
          status: 'error',
          fileName,
          message: errorMsg
        });

        setError(errorMsg);
        if (timeoutRef.current) clearTimeout(timeoutRef.current);
        timeoutRef.current = setTimeout(() => setError(null), 5000);
      }
    },
    [addIndexedDocsToCollection, uploadProgress, onUploadComplete, selectedSpaceId]
  );

  const handleSelectFile = async () => {
    const filePath = await VaultAPI.selectFile();
    if (filePath) {
      setIsUploading(true);
      setError(null);
      const fileName = filePath.split(/[\\/]/).pop() || filePath;
      await handleFileUpload(filePath, fileName);
      setIsUploading(false);
    }
  };

  const handleSelectMultipleFiles = async () => {
    const filePaths = await VaultAPI.selectMultipleFiles();
    if (filePaths && filePaths.length > 0) {
      setIsUploading(true);
      setError(null);

      const progressId = uploadProgress.start(
        `Uploading ${filePaths.length} files`,
        filePaths.length
      );

      let successCount = 0;
      let _duplicateCount = 0;
      let errorCount = 0;
      const errors: string[] = [];
      const indexedDocumentIds: string[] = [];

      // Track duplicates for internal counting (unused but maintained for future use)
      void _duplicateCount;

      for (let i = 0; i < filePaths.length; i++) {
        const filePath = filePaths[i];
        const fileName = filePath.split(/[\\/]/).pop() || filePath;

        uploadProgress.update(progressId, i, `Uploading ${fileName}`, filePaths.length);

        try {
          const result = selectedSpaceId
            ? await VaultAPI.indexFile(filePath, selectedSpaceId)
            : await VaultAPI.indexFile(filePath);
          if (result.ok) {
            showIndexingToast({
              status: result.data.status,
              fileName,
              message: result.data.error || undefined
            });

            if (result.data.status === 'already_indexed') {
              _duplicateCount++;
            } else {
              successCount++;
            }
            indexedDocumentIds.push(result.data.documentId);
          } else {
            showIndexingToast({
              status: 'error',
              fileName,
              message: result.error
            });
            errors.push(`${fileName}: ${result.error}`);
            errorCount++;
          }
        } catch (err) {
          const errorMsg = err instanceof Error ? err.message : 'Unknown error';
          showIndexingToast({
            status: 'error',
            fileName,
            message: errorMsg
          });
          errors.push(`${fileName}: ${errorMsg}`);
          errorCount++;
        }
      }

      if (errorCount === 0) {
        addIndexedDocsToCollection(indexedDocumentIds);
        uploadProgress.complete(progressId, `${successCount} files uploaded successfully`);
        onUploadComplete?.();
      } else if (successCount > 0) {
        addIndexedDocsToCollection(indexedDocumentIds);
        uploadProgress.complete(
          progressId,
          `${successCount} of ${filePaths.length} files uploaded`
        );
        setError(`${errorCount} file(s) failed to upload`);
        if (timeoutRef.current) clearTimeout(timeoutRef.current);
        timeoutRef.current = setTimeout(() => setError(null), 5000);
        onUploadComplete?.();
      } else {
        uploadProgress.fail(progressId, 'All uploads failed');
        setError(errors[0]);
        if (timeoutRef.current) clearTimeout(timeoutRef.current);
        timeoutRef.current = setTimeout(() => setError(null), 5000);
      }

      setIsUploading(false);
    }
  };

  const handleSelectFolder = async () => {
    const folderPath = await VaultAPI.selectFolder();
    if (folderPath) {
      setIsUploading(true);
      setError(null);

      // const folderName = folderPath.split(/[\\/]/).pop() || folderPath;

      try {
        const result = selectedSpaceId
          ? await VaultAPI.startIndexing(folderPath, true, selectedSpaceId)
          : await VaultAPI.startIndexing(folderPath, true);

        if (result.ok) {
          // Indexing progress will be tracked via Tauri events
          onUploadComplete?.();
        } else {
          setError(result.error);
          if (timeoutRef.current) clearTimeout(timeoutRef.current);
          timeoutRef.current = setTimeout(() => setError(null), 5000);
        }
      } catch (err) {
        const errorMsg = err instanceof Error ? err.message : 'Failed to start indexing';
        setError(errorMsg);
        if (timeoutRef.current) clearTimeout(timeoutRef.current);
        timeoutRef.current = setTimeout(() => setError(null), 5000);
      }

      setIsUploading(false);
    }
  };

  const handleDrag = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (e.type === 'dragenter' || e.type === 'dragover') {
      setDragActive(true);
    } else if (e.type === 'dragleave') {
      setDragActive(false);
    }
  }, []);

  const handleDrop = useCallback(
    async (e: React.DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      setDragActive(false);

      if (e.dataTransfer.files && e.dataTransfer.files.length > 0) {
        const files = Array.from(e.dataTransfer.files);
        setIsUploading(true);
        setError(null);

        const progressId = uploadProgress.start(`Uploading ${files.length} files`, files.length);

        let successCount = 0;
        let _duplicateCount = 0;
        let errorCount = 0;
        const errors: string[] = [];
        const indexedDocumentIds: string[] = [];

        // Track duplicates for internal counting (unused but maintained for future use)
        void _duplicateCount;

        for (let i = 0; i < files.length; i++) {
          const file = files[i];
          uploadProgress.update(progressId, i, `Uploading ${file.name}`, files.length);

          try {
            const filePath = (file as File & { path?: string }).path || file.name;
            const result = selectedSpaceId
              ? await VaultAPI.indexFile(filePath, selectedSpaceId)
              : await VaultAPI.indexFile(filePath);
            if (result.ok) {
              showIndexingToast({
                status: result.data.status,
                fileName: file.name,
                message: result.data.error || undefined
              });

              if (result.data.status === 'already_indexed') {
                _duplicateCount++;
              } else {
                successCount++;
              }
              indexedDocumentIds.push(result.data.documentId);
            } else {
              showIndexingToast({
                status: 'error',
                fileName: file.name,
                message: result.error
              });
              errors.push(`${file.name}: ${result.error}`);
              errorCount++;
            }
          } catch (err) {
            const errorMsg = err instanceof Error ? err.message : 'Unknown error';
            showIndexingToast({
              status: 'error',
              fileName: file.name,
              message: errorMsg
            });
            errors.push(`${file.name}: ${errorMsg}`);
            errorCount++;
          }
        }

        if (errorCount === 0) {
          addIndexedDocsToCollection(indexedDocumentIds);
          uploadProgress.complete(progressId, `${successCount} files uploaded successfully`);
          onUploadComplete?.();
        } else if (successCount > 0) {
          addIndexedDocsToCollection(indexedDocumentIds);
          uploadProgress.complete(progressId, `${successCount} of ${files.length} files uploaded`);
          setError(`${errorCount} file(s) failed to upload`);
          if (timeoutRef.current) clearTimeout(timeoutRef.current);
          timeoutRef.current = setTimeout(() => setError(null), 5000);
          onUploadComplete?.();
        } else {
          uploadProgress.fail(progressId, 'All uploads failed');
          setError(errors[0]);
          if (timeoutRef.current) clearTimeout(timeoutRef.current);
          timeoutRef.current = setTimeout(() => setError(null), 5000);
        }

        setIsUploading(false);
      }
    },
    [addIndexedDocsToCollection, uploadProgress, onUploadComplete, selectedSpaceId]
  );

  return (
    <div className="space-y-4">
      <div
        className={`border-2 border-dashed rounded-lg p-8 text-center transition-colors ${
          dragActive
            ? 'border-[var(--accent-primary)] bg-[var(--accent-light)]/20'
            : 'border-[var(--border-color)] hover:border-[var(--border-color)]'
        }`}
        onDragEnter={handleDrag}
        onDragLeave={handleDrag}
        onDragOver={handleDrag}
        onDrop={handleDrop}
      >
        <svg
          className="mx-auto w-12 h-12 text-[var(--text-tertiary)] mb-4"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M15 13l-3-3m0 0l-3 3m3-3v12"
          />
        </svg>
        <p className="text-[var(--text-secondary)] mb-4">
          Drag and drop files here, or click below to select
        </p>
        <div className="mx-auto mb-4 grid max-w-2xl gap-3 text-left sm:grid-cols-2">
          <div className="space-y-1">
            <label
              htmlFor="upload-progress-space-select"
              className="block text-xs font-medium uppercase tracking-wide text-[var(--text-tertiary)]"
            >
              Optional Space Scope
            </label>
            <select
              id="upload-progress-space-select"
              value={selectedSpaceId}
              onChange={(event) => setSelectedSpaceId(event.target.value)}
              disabled={isUploading || isLoadingSpaces}
              className="w-full rounded-md border border-[var(--border-color)] bg-[var(--surface-elevated)] px-3 py-2 text-sm text-[var(--text-primary)] disabled:cursor-not-allowed disabled:opacity-60"
            >
              <option value="">No Space (All)</option>
              {spaces.map((space) => (
                <option key={space.id} value={space.id}>
                  {space.name}
                  {space.isArchived ? ' (archived)' : ''}
                </option>
              ))}
            </select>
          </div>
          <div className="space-y-1">
            <label
              htmlFor="upload-progress-collection-select"
              className="block text-xs font-medium uppercase tracking-wide text-[var(--text-tertiary)]"
            >
              Optional Collection
            </label>
            <select
              id="upload-progress-collection-select"
              value={selectedCollectionId}
              onChange={(event) => setSelectedCollectionId(event.target.value)}
              disabled={isUploading || manualCollections.length === 0}
              className="w-full rounded-md border border-[var(--border-color)] bg-[var(--surface-elevated)] px-3 py-2 text-sm text-[var(--text-primary)] disabled:cursor-not-allowed disabled:opacity-60"
            >
              <option value="">
                {manualCollections.length === 0 ? 'No collections available' : 'No Collection'}
              </option>
              {manualCollections.map((collection) => (
                <option key={collection.id} value={collection.id}>
                  {collection.label}
                </option>
              ))}
            </select>
          </div>
        </div>
        <p className="mx-auto mb-4 max-w-2xl text-left text-xs text-[var(--text-tertiary)]">
          Collection assignment is applied to selected files and drag-drop uploads. Folder indexing
          applies the space scope and can be added to collections later from Library.
        </p>
        <div className="flex flex-col sm:flex-row gap-3 justify-center">
          <button
            onClick={handleSelectFile}
            disabled={isUploading}
            className="px-4 py-2 bg-[var(--accent-primary)] text-white rounded-lg hover:bg-[var(--accent-hover)] disabled:bg-[var(--bg-tertiary)] disabled:cursor-not-allowed transition-colors"
          >
            Select File
          </button>
          <button
            onClick={handleSelectMultipleFiles}
            disabled={isUploading}
            className="px-4 py-2 bg-[var(--accent-primary)] text-white rounded-lg hover:bg-[var(--accent-hover)] disabled:bg-[var(--bg-tertiary)] disabled:cursor-not-allowed transition-colors"
          >
            Select Multiple Files
          </button>
          <button
            onClick={handleSelectFolder}
            disabled={isUploading}
            className="px-4 py-2 bg-[var(--success)] text-white rounded-lg hover:bg-[var(--success)] disabled:bg-[var(--bg-tertiary)] disabled:cursor-not-allowed transition-colors"
          >
            Select Folder
          </button>
        </div>
      </div>

      {/* Mini Progress Indicator */}
      <div className="flex justify-center">
        <MiniProgress onClick={toggleCollapsed} />
      </div>

      {error && (
        <div className="p-4 bg-[var(--error-light)]/20 border border-[var(--error-light)] rounded-lg">
          <p className="text-[var(--error)] text-sm">{error}</p>
        </div>
      )}
    </div>
  );
}
