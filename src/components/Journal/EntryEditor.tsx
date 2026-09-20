import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { PanelRight } from 'lucide-react';

import { TiptapEditor, type SelectionAction } from '@/components/TiptapEditor';
import { IconButton } from '@/components/ui/IconButton';
import type { SnapshotMessage, WorkspaceNote } from '@/types/api/dailyNotes';

import { EntryActionRail } from './EntryActionRail';
import { EntryFromConversation } from './EntryFromConversation';
import { EntryHeader } from './EntryHeader';
import { EntryHighlightsStrip, HIGHLIGHT_CHAR_LIMIT } from './EntryHighlightsStrip';
import { JournalContextRail } from './JournalContextRail';

import type { SynthesisScope, WeekCandidateCounts } from './SynthesizePopover';
import type { JournalEntrySummary } from './useJournalEntries';
import type { JournalSourceSummary } from './useJournalSources';

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
}

const CONTEXT_RAIL_KEY = 'journal.contextRail.open';

function readRailOpen(): boolean {
  // At the minimum supported desktop width the journal index and context rail
  // would otherwise leave almost no room for the page. Start compact windows
  // with the rail closed; it remains available as an overlay from the header.
  if (window.matchMedia('(max-width: 1023px)').matches) return false;
  try {
    return localStorage.getItem(CONTEXT_RAIL_KEY) !== '0';
  } catch {
    return true;
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
}: EntryEditorProps) {
  const editorContainerRef = useRef<HTMLDivElement | null>(null);
  const [railOpen, setRailOpen] = useState(readRailOpen);

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
        onSelect: (text) => onAddHighlight(text.slice(0, HIGHLIGHT_CHAR_LIMIT)),
      },
    ],
    [onAddHighlight],
  );

  const ready = !loadError && !isLoadingNote && Boolean(activeNote);

  return (
    <main className="relative flex min-h-0 min-w-0 flex-1">
      <div className="journal-desk relative flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto">
        <div className="journal-paper mx-auto flex w-full max-w-[740px] flex-1 flex-col px-8 pb-16 pt-7 lg:px-14">
          {loadError ? (
            <p className="text-sm text-[hsl(var(--danger-fg))]">{loadError}</p>
          ) : isLoadingNote || !activeNote ? (
            <p className="text-sm text-[hsl(var(--text-muted))]">Loading journal…</p>
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
                actions={
                  railOpen ? null : (
                    <IconButton label="Show conversation and highlights" shortcut="⌘." onClick={toggleRail}>
                      <PanelRight />
                    </IconButton>
                  )
                }
              />
              <div
                ref={editorContainerRef}
                className="journal-writing-surface min-h-[52vh] flex-1 font-serif text-[17px] leading-[1.85] text-[hsl(var(--text-primary))]"
              >
                <TiptapEditor
                  value={activeNote.content ?? ''}
                  onChange={onUpdateNoteContent}
                  placeholder="A thought, a question, a place to begin…"
                  selectionActions={selectionActions}
                />
              </div>
            </>
          )}
        </div>
      </div>

      {ready && activeNote && railOpen ? (
        <JournalContextRail
          selectedEntryId={selectedEntry?.id ?? null}
          highlightCount={activeNote.highlights.length}
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
          footer={
            <EntryActionRail
              selectedEntryId={selectedEntry?.id ?? null}
              pinnedCount={pinnedEntryCount}
              deckCount={deckCount}
              onSynthesize={onSynthesize}
              disabled={!activeNote}
              weekCandidates={weekCandidates}
            />
          }
        />
      ) : null}
    </main>
  );
}
