import { type FC, useCallback, useEffect, useMemo, useRef, useState } from 'react';

import * as Dialog from '@radix-ui/react-dialog';
import { useQueryClient } from '@tanstack/react-query';
import { open as openExternal } from '@tauri-apps/plugin-shell';
import { X, Download, ExternalLink, Loader2, Maximize2, Minimize2 } from 'lucide-react';
import { useNavigate } from 'react-router';

import { HTMLViewer } from '@/components/ContentViewer/renderers/HTMLViewer';
import { PassageHighlighter, SelectionToolbar, useTextSelection } from '@/components/Reading';
import { PASSAGE_REFERENCES_QUERY_KEY } from '@/hooks/queries/usePassageReferencesQuery';
import { useFileContent } from '@/hooks/useFileContent';
import VaultAPI from '@/lib/api';
import { useConversationsStore } from '@/stores/conversationsStore';
import { toast } from '@/stores/toastStore';
import type {
  PassageLocator,
  PassageMatchTier,
  SourceWithMetadata,
} from '@/types/conversation';
import { formatFileSize, getLanguageFromFileName } from '@/utils/files';
import { sanitizeFileName } from '@/utils/sanitize';
import {
  getSourceExternalUrl,
  getSourcePreviewKind,
  getWebArchiveHtmlPath,
} from '@/utils/sourcePreview';

import { formatSourceLocation } from '../../Reading/passageLocator';
import { CitationRail } from '../CitationRail';
import { passageMatchNotice, sourceHeaderMeta } from '../filePreviewMeta';
import { JournalCapturePreview } from '../JournalCapturePreview';
import { textFragmentUrl } from './textFragment';
import { WebArticleView } from './WebArticleView';
import { AudioViewer, audioViewerPropsFromSource } from '../viewers/AudioViewer';
import { ImageViewer } from '../viewers/ImageViewer';
import { MarkdownViewer } from '../viewers/MarkdownViewer';
import { PDFViewer } from '../viewers/PDFViewer';
import { TextViewer } from '../viewers/TextViewer';

/**
 * Everything the source reader shows, minus the surface it is shown on.
 *
 * The reader has two homes — a pane docked beside the thread, and the overlay
 * it falls back to when the window cannot hold both — and both must show the
 * same document with the same passage marked, the same citation travel and the
 * same capture verbs. So the viewers, the rail and the footer live here once,
 * and the caller supplies only the box: `FilePreviewModal` wraps this in a
 * Radix dialog, `ChatReaderPane` in a plain aside.
 *
 * `presentation` says which, and that is the only thing this file branches on:
 * `dialog` is the full-screen modal, `reading-pane` the overlay on the right
 * edge, `docked` the third pane. The last two are the "compact" forms — they
 * stack the citation rail under the viewer and offer the expand toggle.
 */

export type ReaderPresentation = 'dialog' | 'reading-pane' | 'docked';

const UUID_PATTERN =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

const normalizeCategory = (category: string): string =>
  category.toLowerCase().replace(/[_-]+/g, ' ').trim();

const isHttpUrl = (value: string): boolean => /^https?:\/\//i.test(value.trim());
const isAbsolutePath = (value: string): boolean => value.startsWith('/') || /^[A-Za-z]:[\\/]/.test(value);

const TITLE_CLASS =
  'break-words text-xl font-semibold font-serif leading-tight text-[hsl(var(--text-primary))]';
const CLOSE_CLASS =
  'rounded-sm p-2 text-[hsl(var(--text-muted))] transition-colors duration-fast hover:bg-surface hover:text-[hsl(var(--text-primary))]';

export interface SourceReaderBodyProps {
  presentation: ReaderPresentation;
  source: SourceWithMetadata;
  onClose: () => void;
  /** Where in the file to land. Omit for "open the whole file". */
  initialLocator?: PassageLocator | null;
  /** Every citation on the message, for `[` / `]` travel. */
  citations?: SourceWithMetadata[];
  /** Index of `source` within `citations`. */
  citationIndex?: number;
  onCitationIndexChange?: (_index: number) => void;
  /** Called when a viewer resolves a real location (e.g. a PDF page). */
  onLocationResolved?: (_chunkId: string, _label: string) => void;
  /**
   * The message whose citations these are.
   *
   * What lets a web article mark the passages the answer's own sentences match.
   * Absent when the reader was opened from somewhere with no answer behind it —
   * the Library, Compare, the reference inbox — and then nothing is marked.
   */
  ownerKey?: string;
  /** The reader at its largest. Owned by the surface, which changes size for it. */
  isFocused?: boolean;
  /** Omit to leave the expand toggle out (the full-screen dialog has nothing to expand into). */
  onToggleFocus?: () => void;
}

export const SourceReaderBody: FC<SourceReaderBodyProps> = ({
  presentation,
  source,
  onClose,
  initialLocator = null,
  citations,
  citationIndex,
  onCitationIndexChange,
  onLocationResolved,
  ownerKey,
  isFocused = false,
  onToggleFocus,
}) => {
  const navigate = useNavigate();
  const [captureDraft, setCaptureDraft] = useState<string | null>(null);
  /** The marked passage the web article has in view, if it has one. */
  const [activePassage, setActivePassage] = useState<string | null>(null);
  const isCompact = presentation !== 'dialog';
  const isDocked = presentation === 'docked';
  const queryClient = useQueryClient();
  const viewerColumnRef = useRef<HTMLDivElement | null>(null);
  const selection = useTextSelection(viewerColumnRef);
  const [matchTier, setMatchTier] = useState<PassageMatchTier>('exact');
  const [resolvedLabel, setResolvedLabel] = useState<string | undefined>(undefined);
  const activeConversationId = useConversationsStore((state) => state.activeConversationId);
  const conversations = useConversationsStore((state) => state.conversations);
  const spaces = useConversationsStore((state) => state.spaces);
  const selectedSpaceId = useConversationsStore((state) => state.selectedSpaceId);
  const loadConversationLinkedDocuments = useConversationsStore(
    (state) => state.loadConversationLinkedDocuments
  );

  // File size limits (10MB backend limit)
  const MAX_PREVIEW_SIZE = 10 * 1024 * 1024; // 10MB
  const WARN_SIZE = 5 * 1024 * 1024; // 5MB

  const isFileTooLarge = source.fileSizeBytes > MAX_PREVIEW_SIZE;
  const isFileLarge = source.fileSizeBytes > WARN_SIZE && !isFileTooLarge;

  const mimeType = source.mimeType || '';
  const sourcePreviewKind = getSourcePreviewKind(source);
  const isExternalWebSource = sourcePreviewKind === 'external-web';
  const normalizedCategory = normalizeCategory(source.category ?? '');
  const isWebCategory =
    normalizedCategory.includes('web article') || normalizedCategory === 'web';
  const isWebArchiveArticle = sourcePreviewKind === 'archived-web';
  const externalSourceUrl = getSourceExternalUrl(source);
  const hasDocumentId = Boolean(source.documentId && UUID_PATTERN.test(source.documentId));
  const isWebSourceLike =
    isWebArchiveArticle ||
    isExternalWebSource ||
    isWebCategory ||
    mimeType === 'text/html' ||
    Boolean(source.documentId?.startsWith('web:'));
  const shouldUseUrlActions = isExternalWebSource || (isWebCategory && !!externalSourceUrl);
  const importableUrl = externalSourceUrl ?? (isHttpUrl(source.filePath) ? source.filePath : null);
  const openableUrl = importableUrl;
  // Audio is binary: without this, useFileContent reads the mp3 as UTF-8,
  // errors, and the generic error branch short-circuits before the audio
  // branch in renderViewer.
  const isBinaryPreview =
    mimeType === 'application/pdf' ||
    mimeType.startsWith('image/') ||
    mimeType.startsWith('audio/');
  const webArchiveHtmlPath = getWebArchiveHtmlPath(source.filePath);
  /** A live web page, read out of the page cache rather than off disk. */
  const showsWebArticle = isWebSourceLike && !isWebArchiveArticle;
  // The article view scrolls itself and marks its own passages, so it is left
  // alone like the other embedded viewers: an outer scroller and the block
  // highlighter would both be fighting it.
  const usesEmbeddedViewer = isWebArchiveArticle || isBinaryPreview || showsWebArticle;
  const canShowInFolder = hasDocumentId || (!shouldUseUrlActions && !isHttpUrl(source.filePath ?? ''));

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
      className={`inline-flex items-center gap-2 text-xs text-[hsl(var(--text-secondary))] ${extraClassName}`.trim()}
    >
      <span className="text-[hsl(var(--text-muted))]">Scope</span>
      <select
        value={targetImportSpaceId}
        onChange={(event) => setTargetImportSpaceId(event.target.value)}
        className="min-w-[12rem] rounded-sm border border-border-default bg-surface-raised px-2 py-1 text-sm text-[hsl(var(--text-primary))] focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring))]"
      >
        {allowUnscopedImport && <option value="">Unscoped</option>}
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
      if (!shouldFetchContent) {
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
    if (defaultImportSpaceId) {
      setTargetImportSpaceId(defaultImportSpaceId);
      return;
    }
    if (!allowUnscopedImport && spaces.length > 0) {
      setTargetImportSpaceId(spaces[0].id);
      return;
    }
    setTargetImportSpaceId('');
  }, [allowUnscopedImport, defaultImportSpaceId, spaces]);

  const { content, isLoading, error } = useFileContent(
    shouldFetchContent ? resolvedPreviewPath : undefined,
    shouldFetchContent
  );

  const locatorChunkId = initialLocator?.chunkId;
  const locatorText = initialLocator?.text;

  // A new citation is a new locate attempt; carrying the old tier or page
  // label over would describe the previous passage.
  useEffect(() => {
    setMatchTier('exact');
    setResolvedLabel(undefined);
  }, [locatorChunkId, locatorText]);

  const handleLocationResolved = useCallback(
    (label: string) => {
      setResolvedLabel(label);
      if (locatorChunkId) onLocationResolved?.(locatorChunkId, label);
    },
    [locatorChunkId, onLocationResolved]
  );

  const total = citations?.length ?? 0;
  const canTravel = Boolean(onCitationIndexChange) && total > 1 && typeof citationIndex === 'number';

  const goToCitation = useCallback(
    (next: number) => {
      if (!onCitationIndexChange || total === 0) return;
      onCitationIndexChange(Math.min(total - 1, Math.max(0, next)));
    },
    [onCitationIndexChange, total]
  );

  useEffect(() => {
    if (!canTravel) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== '[' && event.key !== ']') return;
      const target = event.target as HTMLElement | null;
      if (
        target?.closest('input, textarea, select, [contenteditable="true"]')
      ) {
        return;
      }
      event.preventDefault();
      goToCitation((citationIndex ?? 0) + (event.key === ']' ? 1 : -1));
    };
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [canTravel, citationIndex, goToCitation]);

  const captureLocator = useCallback(
    (text: string) => text.trim().slice(0, 4000),
    []
  );

  const handleReference = useCallback(
    async (text: string) => {
      const trimmed = captureLocator(text);
      if (!trimmed) return;
      const result = await VaultAPI.createPassageReference({
        documentId: source.documentId,
        chunkId: initialLocator?.chunkId ?? source.chunkId,
        filePath: source.filePath,
        fileName: source.fileName,
        locator: resolvedLabel ?? formatSourceLocation(source) ?? undefined,
        text: trimmed,
      });
      if (!result.ok) {
        // Say what actually went wrong. "Not available yet" was true while the
        // slice was unbuilt and is a lie now that it ships.
        toast.error("Couldn't save this reference", { message: result.error });
        return;
      }
      await queryClient.invalidateQueries({ queryKey: PASSAGE_REFERENCES_QUERY_KEY });
      toast.success('Saved to references', {
        message: 'Lattice will use this in future answers.',
      });
    },
    [source, captureLocator, initialLocator?.chunkId, resolvedLabel, queryClient]
  );

  const handleAddToJournal = useCallback(
    async (text: string) => {
      const trimmed = captureLocator(text);
      if (!trimmed) return;
      const location = resolvedLabel ?? formatSourceLocation(source);
      const attribution = location
        ? `— ${source.fileName}, ${location}`
        : `— ${source.fileName}`;
      const quoted = trimmed
        .split('\n')
        .map((line) => `> ${line}`)
        .join('\n');
      setCaptureDraft(`${quoted}\n${attribution}`);
    },
    [source, captureLocator, resolvedLabel]
  );

  const handleAskAbout = useCallback(
    (text: string) => {
      const trimmed = captureLocator(text);
      if (!trimmed) return;
      onClose();
      navigate(
        `/chat?new=1&documentId=${encodeURIComponent(source.documentId)}` +
          `&quote=${encodeURIComponent(trimmed)}`
      );
    },
    [source, captureLocator, navigate, onClose]
  );

  const sanitizedFileName = sanitizeFileName(source.fileName);
  const headerMeta = sourceHeaderMeta(source);
  const matchNotice = passageMatchNotice(matchTier, Boolean(initialLocator));
  const primaryActionLabel = shouldUseUrlActions ? 'Open URL' : 'Open file';
  const platformRevealLabel =
    typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform)
      ? 'Reveal in Finder'
      : typeof navigator !== 'undefined' && /Win/i.test(navigator.platform)
        ? 'Show in Explorer'
        : 'Show in file manager';

  const handleOpenPrimary = async () => {
    if (shouldUseUrlActions) {
      if (!openableUrl) {
        console.error('No URL available for external web source');
        return;
      }
      // The browser can find the passage too: a text fragment lands the reader
      // on the same sentences, highlighted, instead of the top of the article.
      const target = activePassage ? textFragmentUrl(openableUrl, activePassage) : openableUrl;
      try {
        await openExternal(target);
      } catch (err) {
        toast.error("Couldn't open URL", {
          message: err instanceof Error ? err.message : String(err),
        });
        // Fallback for environments where shell open is blocked.
        window.open(target, '_blank', 'noopener,noreferrer');
      }
      return;
    }

    if (hasDocumentId) {
      const byId = await VaultAPI.openFileById(source.documentId);
      if (!byId.ok) {
        toast.error("Couldn't open file", { message: byId.error });
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
            toast.error("Couldn't open archive", { message: htmlOpen.error });
          }
        }
      }
      return;
    }

    const byPath = await VaultAPI.openFile(source.filePath);
    if (!byPath.ok) {
      toast.error("Couldn't open file", { message: byPath.error });
    }
  };

  const handleShowInFolder = async () => {
    if (!canShowInFolder) {
      return;
    }

    if (hasDocumentId) {
      const pathResult = await VaultAPI.getFilePathById(source.documentId);
      if (!pathResult.ok) {
        toast.error("Couldn't locate file", { message: pathResult.error });
        return;
      }
      const folderResult = await VaultAPI.showInFolder(pathResult.data);
      if (!folderResult.ok) {
        toast.error(`Couldn't ${platformRevealLabel.toLowerCase()}`, { message: folderResult.error });
      }
      return;
    }

    const folderResult = await VaultAPI.showInFolder(source.filePath);
    if (!folderResult.ok) {
      toast.error(`Couldn't ${platformRevealLabel.toLowerCase()}`, { message: folderResult.error });
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
        toast.error("Couldn't import URL", { message: result.error });
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
                toast.error("Saved, but couldn't open", {
                  message: openResult.error,
                });
              }
            })();
          },
        },
      });
      if (conversationId) {
        void loadConversationLinkedDocuments(conversationId);
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
          <div className="text-[hsl(var(--warning-fg))] mb-4">
            <svg className="w-16 h-16 mx-auto" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
            </svg>
          </div>
          <p className="text-lg font-semibold text-[hsl(var(--text-primary))] mb-2">
            File too large to preview
          </p>
          <p className="text-sm text-[hsl(var(--text-secondary))] mb-4">
            This file is {formatFileSize(source.fileSizeBytes)}. Preview limit is {formatFileSize(MAX_PREVIEW_SIZE)}.
          </p>
          <button
            onClick={handleOpenPrimary}
            className="rounded-md bg-[hsl(var(--accent))] px-4 py-2 text-sm font-medium text-[hsl(var(--accent-fg))] transition-colors duration-fast hover:bg-[hsl(var(--accent-hover))]"
          >
            Open externally
          </button>
        </div>
      );
    }

    if (isLoading || isResolvingPreviewPath) {
      return (
        <div className="flex flex-col items-center justify-center h-64">
          <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-[hsl(var(--accent))] mb-4" />
          {isFileLarge && (
            <p className="text-sm text-[hsl(var(--text-tertiary))]">
              Loading large file ({formatFileSize(source.fileSizeBytes)})...
            </p>
          )}
        </div>
      );
    }

    if (showsWebArticle) {
      return (
        <WebArticleView
          url={openableUrl}
          source={source}
          ownerKey={ownerKey}
          onActivePassageChange={setActivePassage}
          actions={
            openableUrl && !isCompact ? (
              <div className="inline-flex shrink-0 items-center gap-2">
                {importableUrl && renderImportScopeSelect()}
                {importableUrl && (
                  <button
                    type="button"
                    onClick={handleImportSourceUrl}
                    disabled={isImportingUrl}
                    className="inline-flex items-center gap-2 rounded-md border border-border-default bg-surface-raised px-3 py-2 text-sm font-medium text-[hsl(var(--text-primary))] transition-colors duration-fast hover:bg-surface disabled:cursor-not-allowed disabled:opacity-60"
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
                  className="inline-flex items-center gap-2 rounded-md bg-[hsl(var(--accent))] px-3 py-2 text-sm font-medium text-[hsl(var(--accent-fg))] transition-colors duration-fast hover:bg-[hsl(var(--accent-hover))]"
                >
                  <ExternalLink size={14} />
                  Open URL
                </button>
              </div>
            ) : null
          }
        />
      );
    }

    if (error) {
      return (
        <div className="flex flex-col items-center justify-center h-64 text-center">
          <p className="text-[hsl(var(--danger-fg))] mb-2">Couldn't load file</p>
          <p className="text-sm text-[hsl(var(--text-muted))]">{error}</p>
        </div>
      );
    }

    // Web archive articles should render from captured HTML snapshot, not raw markdown text.
    if (isWebArchiveArticle && webArchiveHtmlPath) {
      return <HTMLViewer htmlPath={webArchiveHtmlPath} />;
    }

    // Route to appropriate viewer based on MIME type
    if (mimeType === 'application/pdf') {
      return (
        <PDFViewer
          filePath={source.filePath}
          highlight={initialLocator}
          onLocationResolved={handleLocationResolved}
          onMatch={setMatchTier}
        />
      );
    }

    if (mimeType.startsWith('text/x-') || mimeType === 'application/javascript') {
      const language = getLanguageFromFileName(source.fileName);
      return <TextViewer content={content} language={language} />;
    }

    if (mimeType === 'text/markdown' || source.fileName.endsWith('.md')) {
      return <MarkdownViewer content={content} />;
    }

    if (mimeType.startsWith('audio/')) {
      return <AudioViewer filePath={source.filePath} {...audioViewerPropsFromSource(source)} />;
    }

    if (mimeType.startsWith('image/')) {
      return <ImageViewer filePath={source.filePath} />;
    }

    // Fallback: plain text
    return <TextViewer content={content} language="text" />;
  };

  return (
    <>
      {/* Header */}
      <div className="flex shrink-0 items-center justify-between gap-3 border-b border-subtle px-6 py-5">
        <div className="min-w-0 flex-1">
          <p className="mb-1 text-xs text-text-muted">Source reader</p>
          {isDocked ? (
            <h2 className={TITLE_CLASS}>{sanitizedFileName}</h2>
          ) : (
            <Dialog.Title className={TITLE_CLASS}>{sanitizedFileName}</Dialog.Title>
          )}
          {headerMeta.length > 0 && (
            <p className="mt-2 text-sm text-[hsl(var(--text-muted))]">
              {headerMeta.join(' · ')}
            </p>
          )}
        </div>

        {onToggleFocus && (
          <button type="button" className="rounded-md p-2 text-text-secondary hover:bg-surface-raised"
            aria-label={isFocused ? 'Return to reading pane' : 'Expand reader'}
            title={isFocused ? 'Return to reading pane' : 'Expand reader'}
            onClick={onToggleFocus}>
            {isFocused ? <Minimize2 size={18} /> : <Maximize2 size={18} />}
          </button>
        )}
        {isDocked ? (
          <button
            type="button"
            onClick={onClose}
            className={CLOSE_CLASS}
            aria-label="Close"
            title="Close · Esc"
          >
            <X size={20} />
          </button>
        ) : (
          <Dialog.Close className={CLOSE_CLASS} aria-label="Close" title="Close · Esc">
            <X size={20} />
          </Dialog.Close>
        )}
      </div>

      {/* Content */}
      <div
        className="source-reader-body flex min-h-0 flex-1"
        // Where the citation rail goes. The compact forms stack it under the
        // viewer until the reader is expanded; the full dialog always has room
        // for it beside.
        data-rail={isCompact ? (isFocused ? 'beside' : 'stacked') : undefined}
      >
        <div ref={viewerColumnRef} className="flex min-h-0 min-w-0 flex-1 flex-col">
          {/*
            How well we found the passage belongs beside the passage, not in
            the rail: the rail is `lg:flex` and this admission has to survive
            below 1024px.
          */}
          {matchNotice && (
            <p className="shrink-0 border-b border-subtle px-7 py-2 text-xs text-[hsl(var(--text-muted))]">
              {matchNotice}
            </p>
          )}
          <div
            className={
              usesEmbeddedViewer
                ? 'min-h-0 flex-1 overflow-hidden p-5'
                : 'min-h-0 flex-1 overflow-auto p-7'
            }
          >
            {/*
              Only the text-ish viewers are wrapped: the PDF resolves its own
              page and marks its own text layer, and an image has no text to
              locate.
            */}
            {initialLocator && !usesEmbeddedViewer ? (
              <PassageHighlighter locator={initialLocator} onMatch={setMatchTier}>
                {renderViewer()}
              </PassageHighlighter>
            ) : (
              renderViewer()
            )}
          </div>
        </div>

        {initialLocator && (
          <CitationRail
            source={source}
            locator={initialLocator}
            resolvedLabel={resolvedLabel}
            index={citationIndex}
            total={citations?.length}
            onPrevious={canTravel ? () => goToCitation((citationIndex ?? 0) - 1) : undefined}
            onNext={canTravel ? () => goToCitation((citationIndex ?? 0) + 1) : undefined}
            onReference={() => handleReference(initialLocator.text)}
            onAddToJournal={() => handleAddToJournal(initialLocator.text)}
            onAskAbout={() => handleAskAbout(initialLocator.text)}
          />
        )}
      </div>

      {captureDraft === null && <SelectionToolbar
        selection={selection}
        onReference={handleReference}
        onAddToJournal={handleAddToJournal}
        onAskAbout={handleAskAbout}
      />}

      {/* Footer */}
      <div className="flex shrink-0 flex-wrap items-center justify-end gap-2 border-t border-subtle px-5 py-3">
        {canShowInFolder && (
          <button
            onClick={handleShowInFolder}
            className="flex items-center gap-2 rounded-md border border-border-default bg-surface px-4 py-2 text-sm text-[hsl(var(--text-primary))] transition-colors duration-fast hover:bg-surface-raised"
          >
            <ExternalLink size={16} />
            <span>{platformRevealLabel}</span>
          </button>
        )}

        {shouldUseUrlActions && importableUrl && (
          renderImportScopeSelect('mr-1')
        )}

        {shouldUseUrlActions && importableUrl && (
          <button
            onClick={handleImportSourceUrl}
            disabled={isImportingUrl}
            className="flex items-center gap-2 rounded-md border border-border-default bg-surface px-4 py-2 text-sm text-[hsl(var(--text-primary))] transition-colors duration-fast hover:bg-surface-raised disabled:cursor-not-allowed disabled:opacity-60"
          >
            {isImportingUrl ? <Loader2 size={16} className="animate-spin" /> : <Download size={16} />}
            <span>Import</span>
          </button>
        )}

        <button
          onClick={handleOpenPrimary}
          className="flex items-center gap-2 rounded-md bg-[hsl(var(--accent))] px-4 py-2 text-sm font-medium text-[hsl(var(--accent-fg))] transition-colors duration-fast hover:bg-[hsl(var(--accent-hover))]"
        >
          <Download size={16} />
          <span>{primaryActionLabel}</span>
        </button>
      </div>
      {captureDraft !== null && (
        <JournalCapturePreview content={captureDraft} onClose={() => setCaptureDraft(null)}
          onOpenNote={(noteId) => { setCaptureDraft(null); onClose(); navigate(`/journals?noteId=${encodeURIComponent(noteId)}`); }} />
      )}
    </>
  );
};
