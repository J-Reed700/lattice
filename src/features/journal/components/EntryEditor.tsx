import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { PanelRight, PanelRightClose } from 'lucide-react';

import { TiptapEditor, type SelectionAction } from '@/components/TiptapEditor';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui';
import { EntryActionRail } from '@/features/journal/components/EntryActionRail';
import { EntryFromConversation } from '@/features/journal/components/EntryFromConversation';
import { EntryHeader } from '@/features/journal/components/EntryHeader';
import { EntryHighlightsStrip } from '@/features/journal/components/EntryHighlightsStrip';
import { JournalContextRail } from '@/features/journal/components/JournalContextRail';
import { JournalFormattingPill, useDomTiptapEditor } from '@/features/journal/components/JournalFormattingPill';
import type { SynthesisScope, WeekCandidateCounts } from '@/features/journal/components/SynthesizePopover';
import type { JournalEntrySummary } from '@/features/journal/hooks/useJournalEntries';
import type { JournalSourceSummary } from '@/features/journal/hooks/useJournalSources';
import { journalCitationMap, toJournalCitationSource } from '@/features/journal/model/journalCitationSources';
import { FilePreviewModal } from '@/features/reading/components/FilePreviewModal';
import { SourceCitations } from '@/features/reading/components/SourceCitations';
import { useReaderStore } from '@/features/reading/stores/readerStore';
import { cn } from '@/lib/utils';
import { locatorFromSource } from '@/shared/sources/passageLocator';
import type { SnapshotMessage, WorkspaceNote } from '@/types/api/dailyNotes';
import type { SourceWithMetadata } from '@/types/conversation';

import './journal-editor.css';

interface EntryEditorProps {
  activeNote: WorkspaceNote | null;
  isLoadingNote: boolean;
  loadError: string | null;
  saveError: string | null;
  hasPendingChanges: boolean;
  isSaving: boolean;
  onSaveNow: () => void;
  onUpdateNoteContent: (content: string) => void;
  onAddHighlight: (text: string) => void;
  onRemoveHighlight: (highlightId: string) => void;
  pinnedHighlightIds: Set<string>;
  onTogglePinnedHighlight: (highlightId: string) => void;
  journalName: string;
  /** Title of the page being edited, so the reader knows which one it is. */
  pageTitle: string | null;
  /** True when that title is just this journal's default page name. */
  isDefaultPage: boolean;
  /** A page that was just made: put the caret in its title. */
  autoFocusTitle?: boolean;
  onRenamePage: (title: string) => void;
  selectedEntry: JournalEntrySummary | null;
  selectedEntryMessages: SnapshotMessage[];
  selectedEntryLoading: boolean;
  pinnedEntryCount: number;
  deckCount: number;
  crossEntrySources: JournalSourceSummary[];
  crossEntryLoading: boolean;
  scannedConversationCount: number;
  onJumpToEntry: (entryId: string) => void;
  onSynthesize: (scope: SynthesisScope) => Promise<boolean>;
  weekCandidates?: WeekCandidateCounts;
  onNotify: (tone: 'info' | 'success' | 'error', message: string) => void;
  /** Monotonic signal incremented on explicit conversation picks to open the Context Inspector. */
  openContextRequest?: number;
}

const CONTEXT_RAIL_KEY = 'journal.contextRail.open';

function readRailOpen(): boolean {
  // Default collapsed so the writing canvas opens with generous editorial
  // measure; users can toggle the Context Inspector any time via ⌘. or the
  // top-bar control.
  if (window.matchMedia('(max-width: 1023px)').matches) return false;
  try {
    return localStorage.getItem(CONTEXT_RAIL_KEY) === '1';
  } catch {
    return false;
  }
}

function countWords(markdown: string): number {
  if (!markdown) return 0;
  const stripped = markdown
    .replace(/```[\s\S]*?```/g, ' ')
    .replace(/`[^`]*`/g, ' ')
    .replace(/!?\[[^\]]*\]\([^)]*\)/g, ' ')
    .replace(/[#>*_~`-]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
  if (!stripped) return 0;
  return stripped.split(' ').filter(Boolean).length;
}

/**
 * The page and what sits beside it: a writing column on the desk, and a
 * context rail holding the picked conversation, the kept highlights and
 * Synthesize. The rail can be put away (⌘.) when the page wants the room.
 * Spec §5, §6.
 */
export function EntryEditor({
  activeNote,
  isLoadingNote,
  loadError,
  saveError,
  hasPendingChanges,
  isSaving,
  onSaveNow,
  onUpdateNoteContent,
  onAddHighlight,
  onRemoveHighlight,
  pinnedHighlightIds,
  onTogglePinnedHighlight,
  journalName,
  pageTitle,
  isDefaultPage,
  autoFocusTitle = false,
  onRenamePage,
  selectedEntry,
  selectedEntryMessages,
  selectedEntryLoading,
  pinnedEntryCount,
  deckCount,
  crossEntrySources,
  crossEntryLoading,
  scannedConversationCount,
  onJumpToEntry,
  onSynthesize,
  weekCandidates,
  onNotify,
  openContextRequest = 0,
}: EntryEditorProps) {
  const editorContainerRef = useRef<HTMLDivElement | null>(null);
  const [editorContainer, setEditorContainer] = useState<HTMLDivElement | null>(null);
  const handleEditorContainerRef = useCallback((node: HTMLDivElement | null) => {
    editorContainerRef.current = node;
    setEditorContainer(node);
  }, []);
  const tiptapEditor = useDomTiptapEditor(editorContainer, activeNote?.id ?? null);
  const [railOpen, setRailOpen] = useState(readRailOpen);
  const [seenOpenRequest, setSeenOpenRequest] = useState(openContextRequest);
  if (seenOpenRequest !== openContextRequest) {
    setSeenOpenRequest(openContextRequest);
    if (openContextRequest > 0) setRailOpen(true);
  }
  useEffect(() => {
    if (openContextRequest <= 0) return;
    try {
      localStorage.setItem(CONTEXT_RAIL_KEY, '1');
    } catch {
      // Preference only.
    }
  }, [openContextRequest]);
  const readerSession = useReaderStore((state) => state.session);
  const openReader = useReaderStore((state) => state.open);
  const setReaderIndex = useReaderStore((state) => state.setIndex);
  const closeReader = useReaderStore((state) => state.close);
  const noteSources = useMemo(
    () => (activeNote?.sources ?? []).map(toJournalCitationSource),
    [activeNote?.sources],
  );
  const citationMap = useMemo(() => journalCitationMap(noteSources), [noteSources]);

  useEffect(() => {
    if (readerSession?.ownerKey.startsWith('journal:') && readerSession.ownerKey !== `journal:${activeNote?.id ?? ''}`) {
      closeReader();
    }
  }, [activeNote?.id, closeReader, readerSession?.ownerKey]);

  useEffect(() => () => {
    if (useReaderStore.getState().session?.ownerKey.startsWith('journal:')) closeReader();
  }, [closeReader]);

  const openJournalCitation = useCallback((number: number, occurrence: number | null = null) => {
    const source = citationMap.get(number);
    if (!source || !activeNote) return;
    const citations = [...citationMap.entries()].sort(([a], [b]) => a - b).map(([, value]) => value);
    const index = citations.findIndex((candidate) => candidate.citationId === number);
    openReader(`journal:${activeNote.id}`, citations, index, occurrence);
  }, [activeNote, citationMap, openReader]);
  const openJournalSource = useCallback((clickedSource: SourceWithMetadata, _occurrence?: number | null, chunkId?: string) => {
    if (!activeNote) return;
    const source = noteSources.find((candidate) => candidate.citationId === clickedSource.citationId) ?? clickedSource;
    if (!chunkId || chunkId === source.chunkId) {
      openJournalCitation(source.citationId ?? 0);
      return;
    }
    const excerpt = source.chunkExcerpts?.find((chunk) => chunk.chunkId === chunkId);
    if (!excerpt) {
      openJournalCitation(source.citationId ?? 0);
      return;
    }
    const citations = [...citationMap.entries()].sort(([a], [b]) => a - b).map(([, value]) => value);
    const index = citations.findIndex((candidate) => candidate.citationId === source.citationId);
    // The shared reader resolves the selected passage from source metadata.
    const selected = { ...source, chunkId: excerpt.chunkId, content: excerpt.excerpt,
      excerpt: excerpt.excerpt, section: excerpt.section ?? source.section,
      pageNumber: excerpt.pageNumber ?? source.pageNumber };
    if (index < 0) return;
    citations[index] = selected;
    openReader(`journal:${activeNote.id}`, citations, index, null);
  }, [activeNote, citationMap, noteSources, openJournalCitation, openReader]);

  useEffect(() => {
    const compact = window.matchMedia('(max-width: 1023px)');
    const closeForCompactLayout = (event: MediaQueryListEvent) => {
      if (event.matches) setRailOpen(false);
    };
    compact.addEventListener('change', closeForCompactLayout);
    return () => compact.removeEventListener('change', closeForCompactLayout);
  }, []);

  const toggleRail = useCallback(() => {
    setRailOpen((open) => {
      try {
        localStorage.setItem(CONTEXT_RAIL_KEY, open ? '0' : '1');
      } catch {
        // Preference only.
      }
      return !open;
    });
  }, []);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && !event.shiftKey && !event.altKey && event.key === '.') {
        event.preventDefault();
        toggleRail();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [toggleRail]);

  const date = useMemo(() => {
    if (activeNote?.updatedAt) {
      const d = new Date(activeNote.updatedAt);
      if (!Number.isNaN(d.getTime())) return d;
    }
    return new Date();
  }, [activeNote?.updatedAt]);

  const wordCount = useMemo(() => countWords(activeNote?.content ?? ''), [activeNote?.content]);

  // The Journal's one capture verb rides in the editor's selection toolbar so a
  // selection never grows a second floating bar.
  const selectionActions = useMemo<SelectionAction[]>(
    () => [
      {
        id: 'journal.highlight',
        label: 'Save highlight',
        onSelect: (text) => onAddHighlight(text),
      },
    ],
    [onAddHighlight],
  );

  const ready = !loadError && !isLoadingNote && Boolean(activeNote);
  const contextItemCount = (activeNote?.highlights.length ?? 0) + noteSources.length + (selectedEntry ? 1 : 0);

  return (
    <main className="relative flex min-h-0 min-w-0 flex-1 bg-bg">
      <div className="relative flex min-h-0 min-w-0 flex-1 flex-col">
        <div className="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto [scrollbar-gutter:stable]">
          <div className="journal-editorial-sheet flex w-full flex-1 flex-col pb-16">
            {loadError ? (
              <p className="mx-auto w-full max-w-[740px] px-8 pt-8 text-sm text-danger-fg lg:px-14">{loadError}</p>
            ) : isLoadingNote || !activeNote ? (
              <p className="mx-auto w-full max-w-[740px] px-8 pt-8 text-sm text-text-muted lg:px-14">Loading journal…</p>
            ) : (
              <>
                <EntryHeader
                  date={date}
                  journalName={journalName}
                  pageId={activeNote.id}
                  pageTitle={isDefaultPage ? null : pageTitle}
                  autoFocusTitle={autoFocusTitle}
                  onRename={onRenamePage}
                  wordCount={wordCount}
                  lastEditedAt={activeNote.updatedAt}
                  hasPendingChanges={hasPendingChanges}
                  isSaving={isSaving}
                  saveError={saveError}
                  onRetrySave={onSaveNow}
                  toolbar={<JournalFormattingPill editor={tiptapEditor} />}
                  actions={
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <button
                          type="button"
                          onClick={toggleRail}
                          aria-label={railOpen ? 'Hide context inspector' : 'Show conversation and highlights'}
                          aria-expanded={railOpen}
                          className={cn(
                            'pressable inline-flex h-7 items-center gap-1.5 rounded-lg border px-2 text-xs font-medium transition-colors duration-fast focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring',
                            railOpen
                              ? 'border-accent/30 bg-accent-muted text-accent shadow-sm'
                              : 'border-border-subtle bg-surface text-text-secondary shadow-sm hover:border-border-default hover:text-text-primary',
                          )}
                        >
                          {railOpen ? (
                            <PanelRightClose className="h-3.5 w-3.5" strokeWidth={1.75} aria-hidden="true" />
                          ) : (
                            <PanelRight className="h-3.5 w-3.5" strokeWidth={1.75} aria-hidden="true" />
                          )}
                          <span className="hidden sm:inline">Context</span>
                          {contextItemCount > 0 ? (
                            <span className="inline-flex h-4 min-w-4 items-center justify-center rounded-full bg-[hsl(var(--text-primary)/0.08)] px-1 text-[10px] font-medium tabular-nums text-text-secondary">
                              {contextItemCount}
                            </span>
                          ) : null}
                        </button>
                      </TooltipTrigger>
                      <TooltipContent>
                        <span className="inline-flex items-center gap-2">
                          <span>{railOpen ? 'Hide context inspector' : 'Show conversation and highlights'}</span>
                          <kbd className="font-mono text-[10px] opacity-70">⌘.</kbd>
                        </span>
                      </TooltipContent>
                    </Tooltip>
                  }
                />
                <div
                  ref={handleEditorContainerRef}
                  className="journal-editorial-prose mx-auto min-h-[52vh] w-full max-w-[740px] flex-1 px-8 font-serif text-[17px] leading-[1.85] text-text-primary lg:px-14"
                >
                  <TiptapEditor
                    value={activeNote.content ?? ''}
                    onChange={onUpdateNoteContent}
                    ariaLabel="Journal entry"
                    placeholder="A thought, a question, a place to begin…"
                    selectionActions={selectionActions}
                    citationNumbers={[...citationMap.keys()]}
                    onCitationClick={openJournalCitation}
                  />
                </div>
              </>
            )}
          </div>
        </div>

        {ready && activeNote ? (
          <div className="shrink-0 border-t border-border-subtle bg-bg/95 backdrop-blur-xs">
            <div className="w-full px-6 py-2.5 lg:px-8">
              <EntryActionRail
                selectedEntryId={selectedEntry?.id ?? null}
                pinnedCount={pinnedEntryCount}
                deckCount={deckCount}
                onSynthesize={onSynthesize}
                disabled={!activeNote}
                weekCandidates={weekCandidates}
              />
            </div>
          </div>
        ) : null}
      </div>

      {ready && activeNote && railOpen ? (
        <JournalContextRail
          selectedEntryId={selectedEntry?.id ?? null}
          highlightCount={activeNote.highlights.length}
          sourceCount={noteSources.length}
          onClose={toggleRail}
          conversation={
            <EntryFromConversation
              embedded
              entry={selectedEntry}
              messages={selectedEntryMessages}
              isLoading={selectedEntryLoading}
              crossEntrySources={crossEntrySources}
              crossEntryLoading={crossEntryLoading}
              scannedConversationCount={scannedConversationCount}
              onJumpToEntry={onJumpToEntry}
              onNotify={onNotify}
            />
          }
          highlights={
            <EntryHighlightsStrip
              embedded
              highlights={activeNote.highlights}
              pinnedIds={pinnedHighlightIds}
              onAddHighlight={onAddHighlight}
              onRemoveHighlight={onRemoveHighlight}
              onTogglePinned={onTogglePinnedHighlight}
              editorContainerRef={editorContainerRef}
              showFloatingToolbar={false}
            />
          }
          sources={
            noteSources.length === 0 ? (
              <div className="px-1 py-10 text-center">
                <p className="text-ui font-medium text-text-secondary">No sources saved yet</p>
                <p className="mx-auto mt-1 max-w-[230px] text-xs leading-relaxed text-text-muted">
                  Sources attached to journal syntheses will appear here.
                </p>
              </div>
            ) : (
              <section aria-label="Saved sources">
                <h3 className="mb-3 font-serif text-[17px] font-medium text-text-primary">Sources & citations</h3>
                <p className="mb-4 text-xs leading-relaxed text-text-muted">
                  Select a source here or a citation in the page text to open its passage.
                </p>
                <SourceCitations
                  sources={noteSources}
                  onViewSource={openJournalSource}
                />
              </section>
            )
          }
        />
      ) : null}
      {readerSession?.ownerKey === `journal:${activeNote?.id ?? ''}` ? (
        <FilePreviewModal
          isOpen
          presentation="reading-pane"
          onClose={closeReader}
          source={readerSession.citations[readerSession.index] ?? null}
          initialLocator={readerSession.citations[readerSession.index]
            ? locatorFromSource(readerSession.citations[readerSession.index]!, readerSession.citations[readerSession.index]!.chunkId)
            : null}
          citations={readerSession.citations}
          citationIndex={readerSession.index}
          onCitationIndexChange={setReaderIndex}
          ownerKey={readerSession.ownerKey}
          occurrence={readerSession.occurrence}
          citationContent={activeNote?.content ?? ''}
        />
      ) : null}
    </main>
  );
}
