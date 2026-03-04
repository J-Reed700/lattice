import { type FC, useEffect, useMemo, useState } from 'react';

import * as Dialog from '@radix-ui/react-dialog';
import { open as openExternal } from '@tauri-apps/plugin-shell';
import { X, Download, ExternalLink, Loader2 } from 'lucide-react';

import { HTMLViewer } from '@/components/ContentViewer/renderers/HTMLViewer';
import { useFileContent } from '@/hooks/useFileContent';
import VaultAPI from '@/lib/api';
import { useConversationsStore } from '@/stores/conversationsStore';
import { toast } from '@/stores/toastStore';
import type { SourceWithMetadata } from '@/types/conversation';
import { formatFileSize, getLanguageFromFileName } from '@/utils/files';
import { sanitizeFileName } from '@/utils/sanitize';
import {
  getSourceExternalUrl,
  getSourcePreviewKind,
  getWebArchiveHtmlPath,
} from '@/utils/sourcePreview';

import { ImageViewer } from './viewers/ImageViewer';
import { MarkdownViewer } from './viewers/MarkdownViewer';
import { PDFViewer } from './viewers/PDFViewer';
import { TextViewer } from './viewers/TextViewer';

const UUID_PATTERN =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

const normalizeCategory = (category: string): string =>
  category.toLowerCase().replace(/[_-]+/g, ' ').trim();

const isHttpUrl = (value: string): boolean => /^https?:\/\//i.test(value.trim());
const isAbsolutePath = (value: string): boolean => value.startsWith('/') || /^[A-Za-z]:[\\/]/.test(value);

interface FilePreviewModalProps {
  isOpen: boolean;
  onClose: () => void;
  source: SourceWithMetadata | null;
}

export const FilePreviewModal: FC<FilePreviewModalProps> = ({
  isOpen,
  onClose,
  source,
}) => {
  const activeConversationId = useConversationsStore((state) => state.activeConversationId);
  const conversations = useConversationsStore((state) => state.conversations);
  const spaces = useConversationsStore((state) => state.spaces);
  const selectedSpaceId = useConversationsStore((state) => state.selectedSpaceId);

  // File size limits (10MB backend limit)
  const MAX_PREVIEW_SIZE = 10 * 1024 * 1024; // 10MB
  const WARN_SIZE = 5 * 1024 * 1024; // 5MB

  const isFileTooLarge = source && source.fileSizeBytes > MAX_PREVIEW_SIZE;
  const isFileLarge = source && source.fileSizeBytes > WARN_SIZE && !isFileTooLarge;

  const mimeType = source?.mimeType || '';
  const sourcePreviewKind = source ? getSourcePreviewKind(source) : 'local-file';
  const isExternalWebSource = sourcePreviewKind === 'external-web';
  const normalizedCategory = normalizeCategory(source?.category ?? '');
  const isWebCategory =
    normalizedCategory.includes('web article') || normalizedCategory === 'web';
  const isWebArchiveArticle = sourcePreviewKind === 'archived-web';
  const externalSourceUrl = source ? getSourceExternalUrl(source) : null;
  const hasDocumentId = Boolean(source?.documentId && UUID_PATTERN.test(source.documentId));
  const isWebSourceLike =
    isWebArchiveArticle ||
    isExternalWebSource ||
    isWebCategory ||
    mimeType === 'text/html' ||
    Boolean(source?.documentId?.startsWith('web:'));
  const shouldUseUrlActions = isExternalWebSource || (isWebCategory && !!externalSourceUrl);
  const importableUrl = externalSourceUrl ?? (source && isHttpUrl(source.filePath) ? source.filePath : null);
  const openableUrl = importableUrl;
  const isBinaryPreview = mimeType === 'application/pdf' || mimeType.startsWith('image/');
  const webArchiveHtmlPath = source ? getWebArchiveHtmlPath(source.filePath) : null;
  const usesEmbeddedViewer = isWebArchiveArticle || isBinaryPreview || isExternalWebSource;
  const canShowInFolder = hasDocumentId || (!shouldUseUrlActions && !isHttpUrl(source?.filePath ?? ''));

  const shouldFetchContent =
    !isFileTooLarge &&
    !isBinaryPreview &&
    !isExternalWebSource &&
    (!isWebSourceLike || (!externalSourceUrl && !isWebArchiveArticle));
  const [resolvedPreviewPath, setResolvedPreviewPath] = useState<string | undefined>(undefined);
  const [isResolvingPreviewPath, setIsResolvingPreviewPath] = useState(false);
  const [isImportingUrl, setIsImportingUrl] = useState(false);
  const activeConversation = useMemo(
    () => conversations.find((conversation) => conversation.id === activeConversationId) ?? null,
    [conversations, activeConversationId]
  );
  const defaultImportSpaceId = activeConversation?.spaceId ?? selectedSpaceId ?? null;
  const [targetImportSpaceId, setTargetImportSpaceId] = useState<string>('');
  const allowUnscopedImport = !activeConversationId;
  const spaceNameById = useMemo(() => {
    const map = new Map<string, string>();
    for (const space of spaces) {
      map.set(space.id, space.name);
    }
    return map;
  }, [spaces]);

  const renderImportScopeSelect = (extraClassName = '') => (
    <label
      className={`inline-flex items-center gap-2 rounded-xl border border-[var(--border-color)]/70 bg-[var(--bg-secondary)]/70 px-3 py-2 text-xs text-[var(--text-secondary)] ${extraClassName}`.trim()}
    >
      <span className="uppercase tracking-wide text-[var(--text-tertiary)]">Scope</span>
      <select
        value={targetImportSpaceId}
        onChange={(event) => setTargetImportSpaceId(event.target.value)}
        className="min-w-[12rem] rounded-md border border-[var(--border-color)]/70 bg-[var(--surface-primary)] px-2 py-1 text-sm text-[var(--text-primary)] focus:outline-none focus:ring-1 focus:ring-[var(--accent-primary)]"
      >
        {allowUnscopedImport && <option value="">No space scope</option>}
        {spaces.map((space) => (
          <option key={space.id} value={space.id}>
            {space.name}
            {space.isArchived ? ' (archived)' : ''}
          </option>
        ))}
      </select>
    </label>
  );

  useEffect(() => {
    let mounted = true;
    const resolvePreviewPath = async () => {
      if (!source || !shouldFetchContent) {
        setResolvedPreviewPath(undefined);
        return;
      }
      if (isAbsolutePath(source.filePath) || isHttpUrl(source.filePath) || !hasDocumentId) {
        setResolvedPreviewPath(source.filePath);
        return;
      }
      setIsResolvingPreviewPath(true);
      const result = await VaultAPI.getFilePathById(source.documentId);
      if (!mounted) return;
      if (result.ok) {
        setResolvedPreviewPath(result.data);
      } else {
        setResolvedPreviewPath(source.filePath);
      }
      setIsResolvingPreviewPath(false);
    };

    void resolvePreviewPath();
    return () => {
      mounted = false;
    };
  }, [source, shouldFetchContent, hasDocumentId]);

  useEffect(() => {
    if (!isOpen) {
      return;
    }
    if (defaultImportSpaceId) {
      setTargetImportSpaceId(defaultImportSpaceId);
      return;
    }
    if (!allowUnscopedImport && spaces.length > 0) {
      setTargetImportSpaceId(spaces[0].id);
      return;
    }
    setTargetImportSpaceId('');
  }, [allowUnscopedImport, defaultImportSpaceId, isOpen, spaces]);

  const { content, isLoading, error } = useFileContent(
    shouldFetchContent ? resolvedPreviewPath : undefined,
    shouldFetchContent
  );

  if (!source) return null;

  const sanitizedFileName = sanitizeFileName(source.fileName);
  const primaryActionLabel = shouldUseUrlActions ? 'Open URL' : 'Open File';

  const handleOpenPrimary = async () => {
    if (shouldUseUrlActions) {
      if (!openableUrl) {
        console.error('No URL available for external web source');
        return;
      }
      try {
        await openExternal(openableUrl);
      } catch (err) {
        toast.error('Failed to open URL', {
          message: err instanceof Error ? err.message : String(err),
        });
        // Fallback for environments where shell open is blocked.
        window.open(openableUrl, '_blank', 'noopener,noreferrer');
      }
      return;
    }

    if (hasDocumentId) {
      const byId = await VaultAPI.openFileById(source.documentId);
      if (!byId.ok) {
        toast.error('Failed to open file', { message: byId.error });
        return;
      }

      // If backend chooses internal rendering, open the resolved content path externally.
      if (byId.data.action === 'render_internal') {
        const htmlOpen = await VaultAPI.openFile(byId.data.contentPath);
        if (!htmlOpen.ok) {
          if (openableUrl) {
            try {
              await openExternal(openableUrl);
            } catch {
              window.open(openableUrl, '_blank', 'noopener,noreferrer');
            }
          } else {
            toast.error('Failed to open archived content', { message: htmlOpen.error });
          }
        }
      }
      return;
    }

    const byPath = await VaultAPI.openFile(source.filePath);
    if (!byPath.ok) {
      toast.error('Failed to open file', { message: byPath.error });
    }
  };

  const handleShowInFolder = async () => {
    if (!canShowInFolder) {
      return;
    }

    if (hasDocumentId) {
      const pathResult = await VaultAPI.getFilePathById(source.documentId);
      if (!pathResult.ok) {
        toast.error('Failed to locate file path', { message: pathResult.error });
        return;
      }
      const folderResult = await VaultAPI.showInFolder(pathResult.data);
      if (!folderResult.ok) {
        toast.error('Failed to show in folder', { message: folderResult.error });
      }
      return;
    }

    const folderResult = await VaultAPI.showInFolder(source.filePath);
    if (!folderResult.ok) {
      toast.error('Failed to show in folder', { message: folderResult.error });
    }
  };

  const handleImportSourceUrl = async () => {
    if (!importableUrl || isImportingUrl) {
      return;
    }

    const scopeId = targetImportSpaceId.trim() || undefined;
    const conversationId = activeConversationId ?? undefined;
    const scopedSpaceName =
      (scopeId && spaceNameById.get(scopeId)) ||
      (defaultImportSpaceId ? spaceNameById.get(defaultImportSpaceId) : undefined);

    setIsImportingUrl(true);
    try {
      const result = await VaultAPI.ingestWebUrl(importableUrl, {
        spaceId: scopeId,
        conversationId,
      });
      if (!result.ok) {
        toast.error('Failed to import source URL', { message: result.error });
        return;
      }

      const imported = result.data;
      toast.success('Source imported', {
        message: `${imported.title} (${imported.wordCount.toLocaleString()} words)${scopedSpaceName ? ` · ${scopedSpaceName}` : ''}`,
        action: {
          label: 'Open',
          onClick: () => {
            void (async () => {
              const openResult = await VaultAPI.openFileById(imported.documentId);
              if (!openResult.ok) {
                toast.error('Imported source saved, but failed to open', {
                  message: openResult.error,
                });
              }
            })();
          },
        },
      });
      if (conversationId) {
        void useConversationsStore
          .getState()
          .loadConversationLinkedDocuments(conversationId);
      }
    } finally {
      setIsImportingUrl(false);
    }
  };

  const renderViewer = () => {
    // File too large - show error message
    if (isFileTooLarge) {
      return (
        <div className="flex flex-col items-center justify-center h-64 text-center px-6">
          <div className="text-[var(--warning)] dark:text-[var(--warning-light)] mb-4">
            <svg className="w-16 h-16 mx-auto" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
            </svg>
          </div>
          <p className="text-lg font-semibold text-[var(--text-primary)] dark:text-[var(--text-primary)] mb-2">
            File Too Large for Preview
          </p>
          <p className="text-sm text-[var(--text-secondary)] dark:text-[var(--text-tertiary)] mb-4">
            This file ({formatFileSize(source.fileSizeBytes)}) exceeds the {formatFileSize(MAX_PREVIEW_SIZE)} preview limit.
          </p>
          <button
            onClick={handleOpenPrimary}
            className="px-4 py-2 bg-[var(--accent-primary)] hover:bg-[var(--accent-hover)] text-white rounded-lg transition-colors"
          >
            Open in External Viewer
          </button>
        </div>
      );
    }

    if (isLoading || isResolvingPreviewPath) {
      return (
        <div className="flex flex-col items-center justify-center h-64">
          <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-[var(--accent-primary)] mb-4" />
          {isFileLarge && (
            <p className="text-sm text-[var(--text-tertiary)] dark:text-[var(--text-tertiary)]">
              Loading large file ({formatFileSize(source.fileSizeBytes)})...
            </p>
          )}
        </div>
      );
    }

    if (isWebSourceLike && !isWebArchiveArticle) {
      const sourceUrl = externalSourceUrl ?? source.filePath;
      return (
        <div className="h-full min-h-0 flex flex-col rounded-2xl border border-[var(--border-color)]/70 bg-[var(--bg-secondary)]/50">
          <div className="flex items-center justify-between gap-4 border-b border-[var(--border-color)]/70 px-5 py-4">
            <div className="min-w-0">
              <p className="text-xs uppercase tracking-wide text-[var(--text-tertiary)]">
                Web Source
              </p>
              <p className="mt-1 truncate text-sm text-[var(--text-secondary)]">{sourceUrl}</p>
            </div>
            {openableUrl && (
              <div className="inline-flex items-center gap-2">
                {importableUrl && renderImportScopeSelect()}
                {importableUrl && (
                  <button
                    type="button"
                    onClick={handleImportSourceUrl}
                    disabled={isImportingUrl}
                    className="inline-flex items-center gap-2 rounded-xl border border-[var(--border-color)]/70 bg-[var(--bg-secondary)] px-3 py-2 text-sm font-medium text-[var(--text-primary)] hover:bg-[var(--surface-elevated)] disabled:cursor-not-allowed disabled:opacity-60"
                  >
                    {isImportingUrl ? (
                      <Loader2 size={14} className="animate-spin" />
                    ) : (
                      <Download size={14} />
                    )}
                    Import
                  </button>
                )}
                <button
                  type="button"
                  onClick={handleOpenPrimary}
                  className="inline-flex items-center gap-2 rounded-xl bg-[var(--accent-primary)] px-3 py-2 text-sm font-medium text-white hover:bg-[var(--accent-hover)]"
                >
                  <ExternalLink size={14} />
                  Open URL
                </button>
              </div>
            )}
          </div>
          <div className="p-6">
            <p className="text-sm leading-7 text-[var(--text-primary)]">
              {(source.excerpt || source.content || content || 'Preview unavailable for this web source.')}
            </p>
            {error && (
              <p className="mt-3 text-xs text-[var(--text-tertiary)]">
                {error}
              </p>
            )}
          </div>
        </div>
      );
    }

    if (error && isWebSourceLike) {
      const sourceUrl = externalSourceUrl ?? source.filePath;
      return (
        <div className="h-full min-h-0 flex flex-col rounded-2xl border border-[var(--border-color)]/70 bg-[var(--bg-secondary)]/50">
          <div className="flex items-center justify-between gap-4 border-b border-[var(--border-color)]/70 px-5 py-4">
            <div className="min-w-0">
              <p className="text-xs uppercase tracking-wide text-[var(--text-tertiary)]">
                Web Source
              </p>
              <p className="mt-1 truncate text-sm text-[var(--text-secondary)]">{sourceUrl}</p>
            </div>
            {openableUrl && (
              <div className="inline-flex items-center gap-2">
                {importableUrl && renderImportScopeSelect()}
                {importableUrl && (
                  <button
                    type="button"
                    onClick={handleImportSourceUrl}
                    disabled={isImportingUrl}
                    className="inline-flex items-center gap-2 rounded-xl border border-[var(--border-color)]/70 bg-[var(--bg-secondary)] px-3 py-2 text-sm font-medium text-[var(--text-primary)] hover:bg-[var(--surface-elevated)] disabled:cursor-not-allowed disabled:opacity-60"
                  >
                    {isImportingUrl ? (
                      <Loader2 size={14} className="animate-spin" />
                    ) : (
                      <Download size={14} />
                    )}
                    Import
                  </button>
                )}
                <button
                  type="button"
                  onClick={handleOpenPrimary}
                  className="inline-flex items-center gap-2 rounded-xl bg-[var(--accent-primary)] px-3 py-2 text-sm font-medium text-white hover:bg-[var(--accent-hover)]"
                >
                  <ExternalLink size={14} />
                  Open URL
                </button>
              </div>
            )}
          </div>
          <div className="p-6">
            <p className="text-sm leading-7 text-[var(--text-primary)]">
              {(source.excerpt || source.content || 'Preview unavailable. Open the source URL.')}
            </p>
          </div>
        </div>
      );
    }

    if (error) {
      return (
        <div className="flex flex-col items-center justify-center h-64 text-center">
          <p className="text-[var(--error)] mb-2">Failed to load file</p>
          <p className="text-sm text-[var(--text-tertiary)]">{error}</p>
        </div>
      );
    }

    // Web archive articles should render from captured HTML snapshot, not raw markdown text.
    if (isWebArchiveArticle && webArchiveHtmlPath) {
      return <HTMLViewer htmlPath={webArchiveHtmlPath} />;
    }

    // Route to appropriate viewer based on MIME type
    if (mimeType === 'application/pdf') {
      return <PDFViewer filePath={source.filePath} />;
    }

    if (mimeType.startsWith('text/x-') || mimeType === 'application/javascript') {
      const language = getLanguageFromFileName(source.fileName);
      return <TextViewer content={content} language={language} />;
    }

    if (mimeType === 'text/markdown' || source.fileName.endsWith('.md')) {
      return <MarkdownViewer content={content} />;
    }

    if (mimeType.startsWith('image/')) {
      return <ImageViewer filePath={source.filePath} />;
    }

    // Fallback: plain text
    return <TextViewer content={content} language="text" />;
  };

  return (
    <Dialog.Root open={isOpen} onOpenChange={onClose}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 bg-black/50 backdrop-blur-sm z-50" />

        <Dialog.Content className="
          fixed top-1/2 left-1/2 transform -translate-x-1/2 -translate-y-1/2
          bg-[var(--surface-primary)]/95 dark:bg-[var(--bg-primary)]/95 backdrop-blur-xl
          border border-[var(--border-color)]/70 dark:border-[var(--border-hover)]/60
          rounded-2xl shadow-[0_32px_80px_rgba(0,0,0,0.45)]
          w-[96vw] max-w-[1500px]
          h-[92vh]
          flex flex-col
          z-50
        ">
          {/* Header */}
          <div className="flex items-center justify-between px-8 py-6 border-b border-[var(--border-color)]/80 dark:border-[var(--border-hover)]/80 bg-gradient-to-r from-[var(--bg-secondary)]/70 via-[var(--surface-primary)]/30 to-transparent">
            <div className="flex-1">
              <Dialog.Title className="text-2xl font-semibold leading-tight text-[var(--text-primary)] dark:text-[var(--text-primary)]">
                {sanitizedFileName}
              </Dialog.Title>
              <div className="mt-3 flex items-center gap-3">
                <span className="inline-flex items-center rounded-full border border-[var(--border-color)]/70 bg-[var(--bg-secondary)]/80 px-3 py-1 text-xs font-medium tracking-wide text-[var(--text-secondary)] uppercase">
                  {source.category}
                </span>
                <span className="text-sm text-[var(--text-tertiary)]">
                  {formatFileSize(source.fileSizeBytes)}
                </span>
              </div>
            </div>

            <Dialog.Close className="
              p-2.5 rounded-xl
              text-[var(--text-tertiary)] hover:text-[var(--text-secondary)] hover:bg-[var(--bg-secondary)]/90
              dark:hover:text-[var(--text-secondary)] dark:hover:bg-[var(--surface-elevated)]
              transition-colors
            ">
              <X size={24} />
            </Dialog.Close>
          </div>

          {/* Content */}
          <div className={usesEmbeddedViewer ? 'flex-1 min-h-0 overflow-hidden p-5' : 'flex-1 min-h-0 overflow-auto p-7'}>
            {renderViewer()}
          </div>

          {/* Footer */}
          <div className="flex items-center justify-end gap-3 px-8 py-5 border-t border-[var(--border-color)]/80 dark:border-[var(--border-hover)]/80 bg-gradient-to-r from-transparent to-[var(--bg-secondary)]/55">
            {canShowInFolder && (
              <button
                onClick={handleShowInFolder}
                className="
                  flex items-center gap-2 px-5 py-2.5 rounded-xl
                  bg-[var(--bg-secondary)] hover:bg-[var(--surface-elevated)]
                  dark:bg-[var(--surface-elevated)] dark:hover:bg-[var(--bg-tertiary)]
                  text-[var(--text-primary)] dark:text-[var(--text-secondary)]
                  transition-colors
                "
              >
                <ExternalLink size={16} />
                <span>Show in Folder</span>
              </button>
            )}

            {shouldUseUrlActions && importableUrl && (
              renderImportScopeSelect('mr-1')
            )}

            {shouldUseUrlActions && importableUrl && (
              <button
                onClick={handleImportSourceUrl}
                disabled={isImportingUrl}
                className="
                  flex items-center gap-2 px-5 py-2.5 rounded-xl
                  bg-[var(--bg-secondary)] hover:bg-[var(--surface-elevated)]
                  dark:bg-[var(--surface-elevated)] dark:hover:bg-[var(--bg-tertiary)]
                  text-[var(--text-primary)] dark:text-[var(--text-secondary)]
                  transition-colors disabled:cursor-not-allowed disabled:opacity-60
                "
              >
                {isImportingUrl ? <Loader2 size={16} className="animate-spin" /> : <Download size={16} />}
                <span>Import</span>
              </button>
            )}

            <button
              onClick={handleOpenPrimary}
              className="
                flex items-center gap-2 px-5 py-2.5 rounded-xl
                bg-[var(--accent-primary)] hover:bg-[var(--accent-hover)]
                text-white
                transition-colors
              "
            >
              <Download size={16} />
              <span>{primaryActionLabel}</span>
            </button>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
};
