import { useState, useCallback, useRef, useEffect, useMemo } from 'react';

import VaultAPI from '../../lib/api';
import { useConversationsStore } from '../../stores/conversationsStore';
import { useFileBrowserStore } from '../../stores/fileBrowserStore';

import type { CustomCollection } from '../../types/fileBrowser';

interface UploadProps {
  onUploadComplete?: () => void;
}

export default function Upload({ onUploadComplete }: UploadProps) {
  const selectedConversationSpaceId = useConversationsStore((state) => state.selectedSpaceId);
  const customCollections = useFileBrowserStore((state) => state.customCollections);
  const addDocumentsToCustomCollection = useFileBrowserStore(
    (state) => state.addDocumentsToCustomCollection
  );
  const [isUploading, setIsUploading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);
  const [dragActive, setDragActive] = useState(false);
  const [spaces, setSpaces] = useState<Array<{ id: string; name: string; isArchived: boolean }>>(
    []
  );
  const [selectedSpaceId, setSelectedSpaceId] = useState('');
  const [selectedCollectionId, setSelectedCollectionId] = useState('');
  const [isLoadingSpaces, setIsLoadingSpaces] = useState(false);
  const timeoutRef = useRef<NodeJS.Timeout | null>(null);
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

  const handleFileUpload = useCallback(async (filePath: string) => {
    setIsUploading(true);
    setError(null);
    setSuccess(null);

    const result = selectedSpaceId
      ? await VaultAPI.indexFile(filePath, selectedSpaceId)
      : await VaultAPI.indexFile(filePath);

    if (result.ok) {
      addIndexedDocsToCollection([result.data.documentId]);
      setSuccess(`Successfully indexed: ${filePath}`);
      onUploadComplete?.();
      if (timeoutRef.current) clearTimeout(timeoutRef.current);
      timeoutRef.current = setTimeout(() => setSuccess(null), 3000);
    } else {
      setError(result.error);
    }

    setIsUploading(false);
  }, [addIndexedDocsToCollection, onUploadComplete, selectedSpaceId]);

  const handleSelectFile = async () => {
    const filePath = await VaultAPI.selectFile();
    if (filePath) {
      await handleFileUpload(filePath);
    }
  };

  const handleSelectMultipleFiles = async () => {
    const filePaths = await VaultAPI.selectMultipleFiles();
    if (filePaths && filePaths.length > 0) {
      setIsUploading(true);
      setError(null);
      setSuccess(null);
      let hadError = false;
      const indexedDocumentIds: string[] = [];

      for (const filePath of filePaths) {
        const result = selectedSpaceId
          ? await VaultAPI.indexFile(filePath, selectedSpaceId)
          : await VaultAPI.indexFile(filePath);
        if (!result.ok) {
          setError(`Couldn't index ${filePath}: ${result.error}`);
          hadError = true;
          break;
        }
        indexedDocumentIds.push(result.data.documentId);
      }

      if (!hadError) {
        addIndexedDocsToCollection(indexedDocumentIds);
        setSuccess(`Successfully indexed ${filePaths.length} file(s)`);
        onUploadComplete?.();
        if (timeoutRef.current) clearTimeout(timeoutRef.current);
        timeoutRef.current = setTimeout(() => setSuccess(null), 3000);
      }

      setIsUploading(false);
    }
  };

  const handleSelectFolder = async () => {
    const folderPath = await VaultAPI.selectFolder();
    if (folderPath) {
      setIsUploading(true);
      setError(null);
      setSuccess(null);

      const result = selectedSpaceId
        ? await VaultAPI.startIndexing(folderPath, true, selectedSpaceId)
        : await VaultAPI.startIndexing(folderPath, true);

      if (result.ok) {
        setSuccess(`Started indexing folder: ${folderPath}`);
        onUploadComplete?.();
        if (timeoutRef.current) clearTimeout(timeoutRef.current);
        timeoutRef.current = setTimeout(() => setSuccess(null), 3000);
      } else {
        setError(result.error);
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

  const handleDrop = useCallback(async (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setDragActive(false);

    if (e.dataTransfer.files && e.dataTransfer.files.length > 0) {
      const files = Array.from(e.dataTransfer.files);
      setIsUploading(true);
      setError(null);
      setSuccess(null);
      let hadError = false;
      const indexedDocumentIds: string[] = [];

      for (const file of files) {
        const filePath = (file as File & { path?: string }).path || file.name;
        const result = selectedSpaceId
          ? await VaultAPI.indexFile(filePath, selectedSpaceId)
          : await VaultAPI.indexFile(filePath);
        if (!result.ok) {
          setError(`Couldn't index ${file.name}: ${result.error}`);
          hadError = true;
          break;
        }
        indexedDocumentIds.push(result.data.documentId);
      }

      if (!hadError) {
        addIndexedDocsToCollection(indexedDocumentIds);
        setSuccess(`Successfully indexed ${files.length} file(s)`);
        onUploadComplete?.();
        if (timeoutRef.current) clearTimeout(timeoutRef.current);
        timeoutRef.current = setTimeout(() => setSuccess(null), 3000);
      }

      setIsUploading(false);
    }
  }, [addIndexedDocsToCollection, onUploadComplete, selectedSpaceId]);

  return (
    <div className="space-y-4">
      <div
        className={`border-2 border-dashed rounded-lg p-8 text-center transition-colors ${
          dragActive
            ? 'border-[hsl(var(--accent))] bg-[hsl(var(--accent-muted))]/20'
            : 'border-[hsl(var(--border-subtle))] hover:border-[hsl(var(--border-subtle))]'
        }`}
        onDragEnter={handleDrag}
        onDragLeave={handleDrag}
        onDragOver={handleDrag}
        onDrop={handleDrop}
      >
        <svg
          className="mx-auto w-10 h-10 text-[hsl(var(--text-tertiary))] mb-4"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={1.75}
            d="M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M15 13l-3-3m0 0l-3 3m3-3v12"
          />
        </svg>
        <p className="text-[hsl(var(--text-secondary))] mb-4">
          Drag and drop files here, or click below to select
        </p>
        <div className="mx-auto mb-4 grid max-w-2xl gap-3 text-left sm:grid-cols-2">
          <div className="space-y-1">
            <label
              htmlFor="upload-space-select"
              className="block text-xs font-medium uppercase tracking-wide text-[hsl(var(--text-tertiary))]"
            >
              Optional Space Scope
            </label>
            <select
              id="upload-space-select"
              value={selectedSpaceId}
              onChange={(event) => setSelectedSpaceId(event.target.value)}
              disabled={isUploading || isLoadingSpaces}
              className="w-full rounded-md border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] px-3 py-2 text-sm text-[hsl(var(--text-primary))] disabled:cursor-not-allowed disabled:opacity-60"
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
              htmlFor="upload-collection-select"
              className="block text-xs font-medium uppercase tracking-wide text-[hsl(var(--text-tertiary))]"
            >
              Optional Collection
            </label>
            <select
              id="upload-collection-select"
              value={selectedCollectionId}
              onChange={(event) => setSelectedCollectionId(event.target.value)}
              disabled={isUploading || manualCollections.length === 0}
              className="w-full rounded-md border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] px-3 py-2 text-sm text-[hsl(var(--text-primary))] disabled:cursor-not-allowed disabled:opacity-60"
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
        <p className="mx-auto mb-4 max-w-2xl text-left text-xs text-[hsl(var(--text-tertiary))]">
          Collection assignment is applied to selected files and drag-drop uploads. Folder indexing
          applies the space scope and can be added to collections later from Library.
        </p>
        <div className="flex flex-col sm:flex-row gap-3 justify-center">
          <button
            onClick={handleSelectFile}
            disabled={isUploading}
            className="px-4 py-2 bg-[hsl(var(--accent))] text-[hsl(var(--accent-fg))] rounded-md hover:bg-[hsl(var(--accent-hover))] disabled:bg-[hsl(var(--surface-raised))] disabled:cursor-not-allowed transition-colors"
          >
            Select File
          </button>
          <button
            onClick={handleSelectMultipleFiles}
            disabled={isUploading}
            className="px-4 py-2 bg-[hsl(var(--accent))] text-[hsl(var(--accent-fg))] rounded-md hover:bg-[hsl(var(--accent-hover))] disabled:bg-[hsl(var(--surface-raised))] disabled:cursor-not-allowed transition-colors"
          >
            Select Multiple Files
          </button>
          <button
            onClick={handleSelectFolder}
            disabled={isUploading}
            className="px-4 py-2 bg-[hsl(var(--success-fg))] text-[hsl(var(--accent-fg))] rounded-md hover:bg-[hsl(var(--success-fg))] disabled:bg-[hsl(var(--surface-raised))] disabled:cursor-not-allowed transition-colors"
          >
            Select Folder
          </button>
        </div>
      </div>

      {isUploading && (
        <div className="flex items-center justify-center space-x-2 p-4 bg-[hsl(var(--accent-muted))]/20 rounded-lg">
          <div className="animate-spin rounded-full h-5 w-5 border-b-2 border-[hsl(var(--accent))]" />
          <span className="text-[hsl(var(--accent))]">Indexing files...</span>
        </div>
      )}

      {error && (
        <div className="p-4 bg-[hsl(var(--danger-muted))]/20 border border-[hsl(var(--danger-muted))] rounded-lg">
          <p className="text-[hsl(var(--danger-fg))] text-sm">{error}</p>
        </div>
      )}

      {success && (
        <div className="p-4 bg-[hsl(var(--success-muted))]/20 border border-[hsl(var(--success-muted))] rounded-lg">
          <p className="text-[hsl(var(--success-fg))] text-sm">{success}</p>
        </div>
      )}
    </div>
  );
}
