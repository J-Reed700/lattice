import { type FC, useCallback, useEffect, useRef, useState } from 'react';

import { useQuery } from '@tanstack/react-query';
import { open as openExternal } from '@tauri-apps/plugin-shell';

import { PassageHighlighter, SelectionToolbar, useTextSelection } from '@/features/reading';
import { CitationRail } from '@/features/reading/components/CitationRail';
import { ImportScopeSelect } from '@/features/reading/components/ImportScopeSelect';
import { JournalCapturePreview } from '@/features/reading/components/JournalCapturePreview';
import { SourceReaderFooter } from '@/features/reading/components/SourceReaderFooter';
import { SourceReaderHeader } from '@/features/reading/components/SourceReaderHeader';
import { SourceViewer } from '@/features/reading/components/SourceViewer';
import { WebSourceView } from '@/features/reading/components/WebSourceView';
import { readingKeys } from '@/features/reading/hooks/readSourceFile';
import {
  openSourceDocument,
  openSourcePath,
  resolveSourcePath,
  revealSourcePath,
} from '@/features/reading/hooks/sourceFileActions';
import { useCitationTravel } from '@/features/reading/hooks/useCitationTravel';
import { useFileContent } from '@/features/reading/hooks/useFileContent';
import { usePassageCapture } from '@/features/reading/hooks/usePassageCapture';
import { useSourceUrlImport } from '@/features/reading/hooks/useSourceUrlImport';
import { passageMatchNotice, sourceHeaderMeta } from '@/features/reading/model/filePreviewMeta';
import { textFragmentUrl } from '@/features/reading/model/textFragment';
import { useWebPageMode } from '@/features/reading/snapshot';
import { useAppNavigate } from '@/hooks/useAppNavigate';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import { toast } from '@/stores/toastStore';
import type {
  PassageLocator,
  PassageMatchTier,
  SourceWithMetadata,
} from '@/types/conversation';
import { handleAsyncEvent } from '@/utils/promiseHandlers';
import { sanitizeFileName } from '@/utils/sanitize';
import {
  getSourceExternalUrl,
  getSourcePreviewKind,
  getWebArchiveHtmlPath,
} from '@/utils/sourcePreview';


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
  /** Citation-bearing text for surfaces outside a chat message. */
  citationContent?: string;
  /** Which of the answer's marks for this source was clicked, if one was. */
  occurrence?: number | null;
  /** The reader at its largest. Owned by the surface, which changes size for it. */
  isFocused?: boolean;
  /** Omit to leave the expand toggle out (the full-screen dialog has nothing to expand into). */
  onToggleFocus?: () => void;
  /**
   * Omit to leave the minimize button out. The overlay sheet tucks itself down
   * to a pill in the corner so the page underneath is fully readable; the
   * docked pane and the full-screen dialog have nothing to minimize into.
   */
  onMinimize?: () => void;
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
  citationContent,
  occurrence = null,
  isFocused = false,
  onToggleFocus,
  onMinimize,
}) => {
  const navigate = useAppNavigate();
  /** The marked passage the web article has in view, if it has one. */
  const [activePassage, setActivePassage] = useState<string | null>(null);
  const isCompact = presentation !== 'dialog';
  const isDocked = presentation === 'docked';
  const viewerColumnRef = useRef<HTMLDivElement | null>(null);
  const selection = useTextSelection(viewerColumnRef);
  const [matchTier, setMatchTier] = useState<PassageMatchTier>('exact');
  const [resolvedLabel, setResolvedLabel] = useState<string | undefined>(undefined);
  const activeConversationId = useConversationsStore((state) => state.activeConversationId);

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
  const isWebDocumentId = Boolean(source.documentId?.startsWith('web:'));
  // An HTML file on disk is a page to render, not a web citation to read as
  // text: the sandboxed, theme-aware HTML viewer reads and shows it.
  const isLocalHtmlFile =
    mimeType === 'text/html' &&
    sourcePreviewKind === 'local-file' &&
    !isWebCategory &&
    !isWebDocumentId;
  const isWebSourceLike =
    isWebArchiveArticle ||
    isExternalWebSource ||
    isWebCategory ||
    (mimeType === 'text/html' && !isLocalHtmlFile) ||
    isWebDocumentId;
  const shouldUseUrlActions = isExternalWebSource || (isWebCategory && !!externalSourceUrl);
  const importableUrl = externalSourceUrl ?? (isHttpUrl(source.filePath) ? source.filePath : null);
  const openableUrl = importableUrl;
  const isDocx =
    mimeType === 'application/vnd.openxmlformats-officedocument.wordprocessingml.document' ||
    Boolean(source.filePath?.toLowerCase().endsWith('.docx'));
  // Audio is binary: without this, useFileContent reads the mp3 as UTF-8,
  // errors, and the generic error branch short-circuits before the audio
  // branch in renderViewer.
  const isBinaryPreview =
    mimeType === 'application/pdf' ||
    mimeType.startsWith('image/') ||
    mimeType.startsWith('audio/') ||
    isDocx;
  const webArchiveHtmlPath = getWebArchiveHtmlPath(source.filePath);
  /** A web citation rendered from its saved page text or saved excerpt. */
  const showsWebArticle = isWebSourceLike && !isWebArchiveArticle;
  // A frozen copy belongs to a conversation, so the Page view needs one.
  const snapshotConversationId = showsWebArticle && openableUrl
    ? (source.conversationId ?? activeConversationId ?? '')
    : null;
  const snapshotUrl = showsWebArticle ? openableUrl : null;
  const webPageMode = useWebPageMode(snapshotConversationId, snapshotUrl);
  // The article view scrolls itself and marks its own passages, so it is left
  // alone like the other embedded viewers: an outer scroller and the block
  // highlighter would both be fighting it.
  const usesEmbeddedViewer = isWebArchiveArticle || isBinaryPreview || showsWebArticle || isLocalHtmlFile;
  const canShowInFolder = hasDocumentId || (!shouldUseUrlActions && !isHttpUrl(source.filePath ?? ''));

  const shouldFetchContent =
    !isFileTooLarge &&
    !isBinaryPreview &&
    !isWebArchiveArticle &&
    !isLocalHtmlFile &&
    (hasDocumentId || Boolean(source.filePath && !isHttpUrl(source.filePath)));
  const shouldResolveDocumentPath = hasDocumentId && !isWebSourceLike;
  // Citation metadata can retain the path from when it was indexed. A stable
  // document ID is authoritative when the file has moved; a failed lookup
  // falls back to the cited path.
  const resolvesFromDocument = shouldResolveDocumentPath && !isHttpUrl(source.filePath);
  const documentPathQuery = useQuery({
    queryKey: readingKeys.documentPath(source.documentId ?? ''),
    queryFn: async () => {
      const result = await resolveSourcePath(source.documentId!);
      return result.ok ? result.data : null;
    },
    enabled: resolvesFromDocument,
    retry: false,
    staleTime: 0,
  });
  const isResolvingPreviewPath = resolvesFromDocument && documentPathQuery.isPending;
  const resolvedPreviewPath = !shouldResolveDocumentPath
    ? undefined
    : !resolvesFromDocument
      ? source.filePath
      : isResolvingPreviewPath
        ? undefined
        : documentPathQuery.data ?? source.filePath;

  const {
    spaces,
    targetImportSpaceId,
    setTargetImportSpaceId,
    allowUnscopedImport,
    isImportingUrl,
    handleImportSourceUrl,
  } = useSourceUrlImport(importableUrl, activeConversationId);

  const renderImportScopeSelect = (extraClassName = '') => (
    <ImportScopeSelect
      value={targetImportSpaceId}
      onChange={setTargetImportSpaceId}
      spaces={spaces}
      allowUnscoped={allowUnscopedImport}
      className={extraClassName}
    />
  );

  const { content, isLoading, error } = useFileContent(
    shouldFetchContent ? (resolvedPreviewPath ?? (shouldResolveDocumentPath ? undefined : source.filePath)) : undefined,
    shouldFetchContent
  );
  const savedCitationText = source.excerpt?.trim() || source.content.trim();
  const showingSavedCitationText = Boolean(error && savedCitationText);
  const previewFilePath = resolvedPreviewPath ?? source.filePath;

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

  const { canTravel, goToCitation } = useCitationTravel(citations, citationIndex, onCitationIndexChange);

  const { captureDraft, setCaptureDraft, handleReference, handleAddToJournal, handleAskAbout } =
    usePassageCapture({ source, initialLocator, resolvedLabel, onClose });

  const sanitizedFileName = sanitizeFileName(source.fileName);
  const headerMeta = sourceHeaderMeta(source);
  const matchNotice = showingSavedCitationText
    ? null
    : passageMatchNotice(matchTier, Boolean(initialLocator));
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
      const byId = await openSourceDocument(source.documentId);
      if (!byId.ok) {
        toast.error("Couldn't open file", { message: byId.error });
        return;
      }

      // If backend chooses internal rendering, open the resolved content path externally.
      if (byId.data.action === 'render_internal') {
        const htmlOpen = await openSourcePath(byId.data.contentPath);
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

    const byPath = await openSourcePath(source.filePath);
    if (!byPath.ok) {
      toast.error("Couldn't open file", { message: byPath.error });
    }
  };

  const handleShowInFolder = async () => {
    if (!canShowInFolder) {
      return;
    }

    if (hasDocumentId) {
      const pathResult = await resolveSourcePath(source.documentId);
      if (!pathResult.ok) {
        toast.error("Couldn't locate file", { message: pathResult.error });
        return;
      }
      const folderResult = await revealSourcePath(pathResult.data);
      if (!folderResult.ok) {
        toast.error(`Couldn't ${platformRevealLabel.toLowerCase()}`, { message: folderResult.error });
      }
      return;
    }

    const folderResult = await revealSourcePath(source.filePath);
    if (!folderResult.ok) {
      toast.error(`Couldn't ${platformRevealLabel.toLowerCase()}`, { message: folderResult.error });
    }
  };

  const enrichedWebSource = (() => {
    if (!showsWebArticle) return source;
    if (source.webSnapshot?.text) return source;
    const resolvedText = webPageMode.snapshot?.readerText?.trim() || content?.trim();
    if (!resolvedText) return source;
    return {
      ...source,
      content: resolvedText,
      webSnapshot: {
        url: openableUrl ?? source.filePath,
        title: webPageMode.snapshot?.title ?? source.fileName,
        text: resolvedText,
        fetchedAt: webPageMode.snapshot?.capturedAt ?? null,
        truncated: Boolean(webPageMode.snapshot?.partial),
      },
    };
  })();

  const renderViewer = () => (
    <SourceViewer
      source={source}
      mimeType={mimeType}
      isFileTooLarge={isFileTooLarge}
      isFileLarge={isFileLarge}
      maxPreviewSize={MAX_PREVIEW_SIZE}
      isLoading={isLoading || isResolvingPreviewPath}
      content={content}
      error={error}
      savedCitationText={savedCitationText}
      previewFilePath={previewFilePath}
      initialLocator={initialLocator}
      onLocationResolved={handleLocationResolved}
      onMatch={setMatchTier}
      isWebArchiveArticle={isWebArchiveArticle}
      webArchiveHtmlPath={webArchiveHtmlPath}
      webArticle={
        showsWebArticle ? (
          <WebSourceView
            source={enrichedWebSource}
            url={openableUrl}
            ownerKey={ownerKey}
            citationContent={citationContent}
            occurrence={occurrence}
            onActivePassageChange={setActivePassage}
            compact={isCompact}
            importableUrl={importableUrl}
            importScopeSelect={renderImportScopeSelect()}
            isImportingUrl={isImportingUrl}
            onImportUrl={handleAsyncEvent(handleImportSourceUrl)}
            onOpenUrl={handleAsyncEvent(handleOpenPrimary)}
            snapshotConversationId={snapshotConversationId}
            snapshotUrl={snapshotUrl}
            webPageMode={webPageMode}
            initialLocator={initialLocator}
          />
        ) : null
      }
      onOpenPrimary={handleAsyncEvent(handleOpenPrimary)}
    />
  );

  return (
    <>
      {/* Header */}
      <SourceReaderHeader
        isDocked={isDocked}
        title={sanitizedFileName}
        meta={headerMeta}
        isFocused={isFocused}
        onToggleFocus={onToggleFocus}
        onMinimize={onMinimize}
        onClose={onClose}
      />

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
      <SourceReaderFooter
        canShowInFolder={canShowInFolder}
        onShowInFolder={handleAsyncEvent(handleShowInFolder)}
        platformRevealLabel={platformRevealLabel}
        canImportUrl={shouldUseUrlActions && Boolean(importableUrl)}
        importScopeSelect={renderImportScopeSelect('mr-1')}
        isImportingUrl={isImportingUrl}
        onImportUrl={handleAsyncEvent(handleImportSourceUrl)}
        primaryActionLabel={primaryActionLabel}
        onOpenPrimary={handleAsyncEvent(handleOpenPrimary)}
      />
      {captureDraft !== null && (
        <JournalCapturePreview content={captureDraft} onClose={() => setCaptureDraft(null)}
          onOpenNote={(noteId) => { setCaptureDraft(null); onClose(); navigate(`/journals?noteId=${encodeURIComponent(noteId)}`); }} />
      )}
    </>
  );
};
