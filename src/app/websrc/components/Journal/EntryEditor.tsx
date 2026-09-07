import { useMemo, useRef } from 'react';

import { TiptapEditor, type SelectionAction } from '@/components/TiptapEditor';
import type { SnapshotMessage, WorkspaceNote } from '@/types/api/dailyNotes';

import { EntryActionRail } from './EntryActionRail';
import { EntryFromConversation } from './EntryFromConversation';
import { EntryHeader } from './EntryHeader';
import { EntryHighlightsStrip, HIGHLIGHT_CHAR_LIMIT } from './EntryHighlightsStrip';

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
 * Main editor pane: header → editor body → highlights strip →
 * from-this-conversation → action rail.
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

  return (
    <main className="relative flex min-h-0 flex-1 flex-col overflow-y-auto bg-[hsl(var(--bg))]">
      <div className="mx-auto flex w-full max-w-[clamp(680px,72vw,900px)] flex-1 flex-col px-6 pt-10 pb-6">
        {loadError ? (
          <p className="text-sm text-[hsl(var(--danger-fg))]">{loadError}</p>
        ) : isLoadingNote || !activeNote ? (
          <p className="text-sm text-[hsl(var(--text-muted))]">Loading journal…</p>
        ) : (
          <>
            <EntryHeader
              date={date}
              journalName={journalName}
              pageTitle={isDefaultPage ? null : pageTitle}
              wordCount={wordCount}
              lastEditedAt={activeNote.updatedAt}
              hasPendingChanges={hasPendingChanges}
              isSaving={isSaving}
              saveError={saveError}
              onRetrySave={onSaveNow}
            />
            <div
              ref={editorContainerRef}
              className="min-h-[60vh] font-serif text-base leading-[1.65] text-[hsl(var(--text-primary))]"
            >
              <TiptapEditor
                value={activeNote.content ?? ''}
                onChange={onUpdateNoteContent}
                placeholder="Start writing — just yourself, today."
                selectionActions={selectionActions}
              />
            </div>
            <EntryHighlightsStrip
              highlights={activeNote.highlights}
              pinnedIds={pinnedHighlightIds}
              onAddHighlight={onAddHighlight}
              onRemoveHighlight={onRemoveHighlight}
              onTogglePinned={onTogglePinnedHighlight}
              editorContainerRef={editorContainerRef}
              showFloatingToolbar={false}
            />
            <EntryFromConversation
              entry={selectedEntry}
              messages={selectedEntryMessages}
              isLoading={selectedEntryLoading}
              crossEntrySources={crossEntrySources}
              crossEntryLoading={crossEntryLoading}
              scannedConversationCount={scannedConversationCount}
              onJumpToEntry={onJumpToEntry}
              onNotify={onNotify}
            />
            <EntryActionRail
              selectedEntryId={selectedEntry?.id ?? null}
              pinnedCount={pinnedEntryCount}
              deckCount={deckCount}
              onSynthesize={onSynthesize}
              disabled={!activeNote}
              weekCandidates={weekCandidates}
            />
          </>
        )}
      </div>
    </main>
  );
}
