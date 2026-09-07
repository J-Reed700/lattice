import { useCallback, useEffect, useMemo, useState } from 'react';

import { ArrowUpRight, Bookmark, Copy, PanelLeft, Trash2 } from 'lucide-react';
import { useNavigate, useSearchParams } from 'react-router';

import { FilePreviewModal } from '@/components/Chat/FilePreviewModal';
import { useRegisterPaletteCommands } from '@/hooks/useRegisterPaletteCommands';
import type { PaletteCommand } from '@/stores/paletteCommandsStore';
import { toast } from '@/stores/toastStore';
import type { ConversationMessageBookmarkDto } from '@/types';
import type { PassageReferenceDto } from '@/types/api/references';
import type { PassageLocator, SourceWithMetadata } from '@/types/conversation';
import type { CapturedChatReference } from '@/utils/chatReferenceIndex';
import { mimeTypeForPath } from '@/utils/mimeTypes';

import { PassageReader } from './PassageReader';
import { ReferenceList } from './ReferenceList';
import { ReferenceReader } from './ReferenceReader';
import { useReferenceInbox } from './useReferenceInbox';

const SIDEBAR_COLLAPSED_KEY = 'references.sidebar.collapsed';

function readSidebarCollapsed(): boolean {
  try {
    return localStorage.getItem(SIDEBAR_COLLAPSED_KEY) === '1';
  } catch {
    return false;
  }
}

function writeSidebarCollapsed(value: boolean): void {
  try {
    localStorage.setItem(SIDEBAR_COLLAPSED_KEY, value ? '1' : '0');
  } catch {
    // Ignore
  }
}

/**
 * Shell orchestrator for the reference inbox surface. Two-pane layout
 * (sidebar + reader), URL-driven selection via ?referenceId=, wires the
 * data hook's actions into navigation callbacks and toasts.
 * Spec §3, §10.
 */
export function ReferenceInbox() {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const requestedReferenceId = searchParams.get('referenceId');

  const [sidebarCollapsed, setSidebarCollapsed] = useState(readSidebarCollapsed);

  useEffect(() => {
    writeSidebarCollapsed(sidebarCollapsed);
  }, [sidebarCollapsed]);

  const state = useReferenceInbox({ requestedReferenceId });

  const [previewSource, setPreviewSource] = useState<SourceWithMetadata | null>(null);
  const [previewLocator, setPreviewLocator] = useState<PassageLocator | null>(null);

  const {
    selectedItem,
    selectedBookmark,
    selectedCapture,
    selectedSpace,
    selectedPayload,
    selectedId,
    isResolvingSelected,
    resolutionFailed,
    resolveCaptureDestination,
    resolveJournalSpaceIdForNote,
    captureReference,
    removeReference,
    saveAnnotations,
    savePassageAnnotations,
    removePassage,
    getPayload,
  } = state;

  const selectedPassage: PassageReferenceDto | null =
    selectedItem?.kind === 'passage' ? selectedItem.passage : null;

  // Sync selectedId back into URL (?referenceId=) without polluting history.
  useEffect(() => {
    const current = searchParams.get('referenceId');
    if (!selectedId) {
      // A deep link arrives before the list does. Stripping it on that first
      // render — when nothing is selected yet because nothing is loaded yet —
      // threw away the id the inbox was about to open. Only an inbox that has
      // finished loading with nothing in it clears the parameter.
      if (current && !state.isLoading && state.filteredItems.length === 0) {
        const next = new URLSearchParams(searchParams);
        next.delete('referenceId');
        setSearchParams(next, { replace: true });
      }
      return;
    }
    if (current !== selectedId) {
      const next = new URLSearchParams(searchParams);
      next.set('referenceId', selectedId);
      setSearchParams(next, { replace: true });
    }
  }, [searchParams, selectedId, setSearchParams, state.filteredItems.length, state.isLoading]);

  // Toggle sidebar via ⌘\
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && !e.shiftKey && e.key === '\\') {
        e.preventDefault();
        setSidebarCollapsed((v) => !v);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const openBookmarkInChat = useCallback(
    (bookmark: ConversationMessageBookmarkDto) => {
      const params = new URLSearchParams({
        conversationId: bookmark.conversationId,
        messageId: bookmark.messageId,
      });
      navigate(`/chat?${params.toString()}`);
    },
    [navigate],
  );

  const openCapturedNote = useCallback(
    (reference: CapturedChatReference | null) => {
      if (!reference) {
        navigate('/journals');
        return;
      }
      const params = new URLSearchParams({
        noteId: reference.noteId,
        snapshotId: reference.snapshotId,
      });
      const journalSpaceId = resolveJournalSpaceIdForNote(reference.noteId);
      if (journalSpaceId) {
        params.set('journalSpaceId', journalSpaceId);
      }
      navigate(`/journals?${params.toString()}`);
    },
    [navigate, resolveJournalSpaceIdForNote],
  );

  const handleCopy = useCallback(
    async (bookmark: ConversationMessageBookmarkDto): Promise<boolean> => {
      try {
        const payload = await getPayload(bookmark);
        await navigator.clipboard.writeText(payload.content);
        return true;
      } catch (error) {
        const message = error instanceof Error ? error.message : 'Unknown error';
        toast.error('Failed to copy reference', { message });
        return false;
      }
    },
    [getPayload],
  );

  const handleCapture = useCallback(
    async (bookmark: ConversationMessageBookmarkDto): Promise<void> => {
      try {
        const result = await captureReference(bookmark);
        toast.success('Reference captured', {
          message: result ? `Saved to "${result.noteTitle}".` : 'Saved to Journals Inbox.',
          action: result
            ? {
                label: 'Open',
                onClick: () => openCapturedNote(result),
              }
            : undefined,
        });
      } catch (error) {
        const message = error instanceof Error ? error.message : 'Unknown error';
        toast.error('Failed to capture reference', { message });
      }
    },
    [captureReference, openCapturedNote],
  );

  const handleDelete = useCallback(
    async (bookmark: ConversationMessageBookmarkDto): Promise<void> => {
      const ok = await removeReference(bookmark);
      if (ok) {
        toast.success('Reference removed');
      }
    },
    [removeReference],
  );

  const openPassageSource = useCallback((passage: PassageReferenceDto) => {
    // `section` is the label the reader shows; `initialLocator` is what makes
    // it land on the saved passage rather than the top of the file.
    setPreviewLocator({
      text: passage.text,
      chunkId: passage.chunkId ?? undefined,
      label: passage.locator ?? undefined,
    });
    setPreviewSource({
      documentId: passage.documentId,
      chunkId: passage.chunkId ?? '',
      fileName: passage.fileName,
      filePath: passage.filePath,
      mimeType: mimeTypeForPath(passage.filePath),
      category: '',
      content: passage.text,
      excerpt: passage.text,
      highlights: [],
      section: passage.locator ?? undefined,
      score: 1,
      fileSizeBytes: 0,
      modifiedAt: passage.createdAt,
    });
  }, []);

  const handleCopyPassage = useCallback(
    async (passage: PassageReferenceDto): Promise<boolean> => {
      try {
        await navigator.clipboard.writeText(passage.text);
        return true;
      } catch (error) {
        const message = error instanceof Error ? error.message : 'Unknown error';
        toast.error('Failed to copy reference', { message });
        return false;
      }
    },
    [],
  );

  const handleDeletePassage = useCallback(
    async (passage: PassageReferenceDto): Promise<void> => {
      const ok = await removePassage(passage);
      if (ok) {
        toast.success('Reference removed');
      }
    },
    [removePassage],
  );

  const handleViewSource = useCallback(
    (source: SourceWithMetadata) => {
      if (source.filePath.startsWith('http://') || source.filePath.startsWith('https://')) {
        window.open(source.filePath, '_blank', 'noopener,noreferrer');
        return;
      }
      // The preview modal is already mounted on this surface: open the file
      // rather than naming it. A control that only announces itself is a lie.
      setPreviewLocator(
        source.content
          ? { text: source.content, chunkId: source.chunkId || undefined }
          : null,
      );
      setPreviewSource(source);
    },
    [],
  );

  const paletteCommands = useMemo<PaletteCommand[]>(
    () => [
      {
        id: 'references.openSource',
        label: 'Open the source document',
        group: 'References',
        icon: ArrowUpRight,
        enabled: selectedPassage !== null,
        run: () => {
          if (selectedPassage) openPassageSource(selectedPassage);
        },
      },
      {
        id: 'references.copy',
        label: 'Copy reference',
        group: 'References',
        icon: Copy,
        enabled: selectedItem !== null,
        run: async () => {
          if (selectedPassage) {
            await handleCopyPassage(selectedPassage);
            return;
          }
          if (selectedBookmark) await handleCopy(selectedBookmark);
        },
      },
      {
        id: 'references.delete',
        label: 'Delete reference',
        group: 'References',
        icon: Trash2,
        enabled: selectedItem !== null,
        run: async () => {
          if (selectedPassage) {
            await handleDeletePassage(selectedPassage);
            return;
          }
          if (selectedBookmark) await handleDelete(selectedBookmark);
        },
      },
    ],
    [
      handleCopy,
      handleCopyPassage,
      handleDelete,
      handleDeletePassage,
      openPassageSource,
      selectedBookmark,
      selectedItem,
      selectedPassage,
    ],
  );
  useRegisterPaletteCommands(paletteCommands);

  const selectedDestination = selectedBookmark
    ? resolveCaptureDestination(selectedBookmark)
    : null;

  // Show payload as null while resolving if cache empty to keep preview fallback clean.
  const effectivePayload = selectedPayload;
  // isResolvingSelected: not currently surfaced to reader body visually
  // because the body already falls back to preview; reserved for future indicator.
  void isResolvingSelected;

  return (
    <div className="relative flex h-full overflow-hidden bg-[hsl(var(--bg))] text-[hsl(var(--text-primary))]">
      {sidebarCollapsed ? (
        <aside className="flex h-full w-14 shrink-0 flex-col items-center gap-2 border-r border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))] py-2">
          <button
            type="button"
            onClick={() => setSidebarCollapsed(false)}
            className="rounded-sm p-2 text-[hsl(var(--text-tertiary))] hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
            aria-label="Expand sidebar"
            title="Expand sidebar (⌘\\)"
          >
            <PanelLeft className="h-4 w-4" strokeWidth={1.75} />
          </button>
          <div className="mt-1 flex flex-col items-center gap-1 text-[hsl(var(--text-tertiary))]">
            <Bookmark className="h-4 w-4" strokeWidth={1.75} />
          </div>
        </aside>
      ) : (
        <ReferenceList
          state={state}
          onToggleCollapse={() => setSidebarCollapsed(true)}
          onOpenInOrigin={openBookmarkInChat}
          onCopy={(b) => void handleCopy(b)}
          onDelete={(b) => void handleDelete(b)}
          onOpenPassageSource={openPassageSource}
          onCopyPassage={(p) => void handleCopyPassage(p)}
          onDeletePassage={(p) => void handleDeletePassage(p)}
        />
      )}

      {selectedPassage ? (
        <PassageReader
          passage={selectedPassage}
          onSaveAnnotations={async (next) => savePassageAnnotations(selectedPassage, next)}
          onOpenSource={() => openPassageSource(selectedPassage)}
          onCopy={async () => handleCopyPassage(selectedPassage)}
          onDelete={() => void handleDeletePassage(selectedPassage)}
        />
      ) : (
      <ReferenceReader
        bookmark={selectedBookmark}
        space={selectedSpace}
        capture={selectedCapture}
        payload={effectivePayload}
        resolutionFailed={resolutionFailed}
        captureDestination={selectedDestination}
        onSaveAnnotations={async (next) => {
          if (!selectedBookmark) return false;
          return saveAnnotations(selectedBookmark, next);
        }}
        onOpenInChat={() => {
          if (selectedBookmark) openBookmarkInChat(selectedBookmark);
        }}
        onOpenConversation={() => {
          if (selectedBookmark) openBookmarkInChat(selectedBookmark);
        }}
        onOpenCapturedNote={() => openCapturedNote(selectedCapture)}
        onCapture={async () => {
          if (selectedBookmark) await handleCapture(selectedBookmark);
        }}
        onCopy={async () => {
          if (!selectedBookmark) return false;
          return handleCopy(selectedBookmark);
        }}
        onDelete={() => {
          if (selectedBookmark) void handleDelete(selectedBookmark);
        }}
        onViewSource={handleViewSource}
      />
      )}

      <FilePreviewModal
        isOpen={previewSource !== null}
        onClose={() => {
          setPreviewSource(null);
          setPreviewLocator(null);
        }}
        source={previewSource}
        initialLocator={previewLocator}
      />
    </div>
  );
}
