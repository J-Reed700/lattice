import { useCallback, useMemo, useState } from 'react';

import { useNavigate, useSearchParams } from 'react-router';

import { useJournalsQuery } from '@/features/journal/api/queries';
import { EntryEditor } from '@/features/journal/components/EntryEditor';
import { EntryList } from '@/features/journal/components/EntryList';
import {
  CollapsedJournalSidebar,
  JournalEmptyState,
  JournalLoadError,
  JournalNoticeToast,
} from '@/features/journal/components/JournalWorkspaceChrome';
import { useJournalPaletteCommands, useJournalShortcuts } from '@/features/journal/hooks/useJournalCommands';
import { useJournalEntries } from '@/features/journal/hooks/useJournalEntries';
import { useJournalEntryActions } from '@/features/journal/hooks/useJournalEntryActions';
import { useJournalNavigationGuard } from '@/features/journal/hooks/useJournalNavigationGuard';
import { UNTITLED_PAGE, useJournalNote } from '@/features/journal/hooks/useJournalNote';
import { useJournalNotice } from '@/features/journal/hooks/useJournalNotice';
import { useJournalPageActions } from '@/features/journal/hooks/useJournalPageActions';
import { useJournalSources } from '@/features/journal/hooks/useJournalSources';
import { useFollowNoteIdParam, useJournalSpaces } from '@/features/journal/hooks/useJournalSpaces';
import { RECENT_DECK_SIZE, useJournalSynthesis } from '@/features/journal/hooks/useJournalSynthesis';
import { usePinnedNoteHighlights, useSidebarCollapsed } from '@/features/journal/hooks/useJournalViewPrefs';
import { useWeeklySynthesisCandidatesQuery } from '@/features/journal/hooks/useWeeklySynthesisCandidatesQuery';
import { useActiveSynthesis } from '@/features/journal/synthesis/synthesisPanel';

function nowIso(): string {
  return new Date().toISOString();
}

function makeId(prefix: string): string {
  return `${prefix}_${crypto.randomUUID()}`;
}

/**
 * Shell orchestrator for the journal workspace. Reads URL params, handles
 * journal list / auto-redirect lifecycle, wires the data hooks to the
 * presentational components.
 */
export function JournalWorkspace() {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const requestedJournalSpaceId = searchParams.get('journalSpaceId');
  const requestedEntryId = searchParams.get('entryId');
  const requestedNoteIdParam = searchParams.get('noteId');
  // `?noteId=` is a one-shot instruction ("open the page that was just
  // written"), not a mirror of the selection: it is consumed on arrival so a
  // later journal switch does not drag the user back to the same page.
  const [requestedNoteId, setRequestedNoteId] = useState<string | null>(requestedNoteIdParam);

  const { data: weekCandidates } =
    useWeeklySynthesisCandidatesQuery();
  const isSynthesizing = useActiveSynthesis() !== null;

  const journalsQuery = useJournalsQuery();
  const allJournals = useMemo(() => journalsQuery.journals.filter(journal => !journal.isArchived), [journalsQuery.journals]);
  const journalSpace = allJournals.find(journal => journal.id === requestedJournalSpaceId) ?? null;
  const { notice, setNotice, notify } = useJournalNotice();

  const entriesState = useJournalEntries({
    journalSpaceId: requestedJournalSpaceId,
    requestedEntryId,
  });
  const {
    entries,
    pinnedIds,
    selectedId,
    setSelectedId,
    messagesByConversation,
    loadingByConversation,
    loadMessages,
    removeEntry,
    reload: reloadEntries,
  } = entriesState;

  const noteState = useJournalNote({
    journalSpaceId: requestedJournalSpaceId,
    journalName: journalSpace?.name ?? null,
    requestedNoteId,
  });
  const {
    activeNote,
    pages,
    isLoadingNote,
    loadError: noteLoadError,
    saveError,
    hasPendingChanges,
    isSavingNow,
    updateNote,
    saveNow,
    selectPage,
    createPage,
    renamePage,
    deletePage,
  } = noteState;

  useJournalNavigationGuard(hasPendingChanges, saveNow);

  const sourcesState = useJournalSources({
    entries,
    pinnedIds,
    messagesByConversation,
    loadingByConversation,
    loadMessages,
    enabled: true,
  });


  const [sidebarCollapsed, setSidebarCollapsed] = useSidebarCollapsed();

  useFollowNoteIdParam({ requestedNoteIdParam, searchParams, setSearchParams, setRequestedNoteId });

  const { pinnedNoteHighlightIds, handleTogglePinnedHighlight } = usePinnedNoteHighlights(
    requestedJournalSpaceId,
    activeNote,
  );

  const {
    topLevelError,
    retryJournalLoad,
    isCreatingJournal,
    handleSwitchJournal,
    handleCreateJournal,
    handleRenameJournal,
    handleDeleteJournal,
  } = useJournalSpaces({
    requestedJournalSpaceId,
    journalSpace,
    activeNote,
    updateNote,
    searchParams,
    navigate,
    notify,
  });

  const { handleNewEntry, handleRenameEntry, handleDeleteEntry } = useJournalEntryActions({
    requestedJournalSpaceId,
    navigate,
    notify,
    saveNow,
    reloadEntries,
    removeEntry,
  });

  const {
    handleSelectPage,
    displayPageTitle,
    pageToName,
    handleNewPage,
    handleRenamePage,
    handleDeletePage,
  } = useJournalPageActions({
    journalSpace,
    requestedJournalSpaceId,
    isLoadingNote,
    selectPage,
    createPage,
    renamePage,
    deletePage,
    notify,
    searchParams,
    setSearchParams,
  });

  const handleUpdateNoteContent = useCallback(
    (content: string) => {
      updateNote((note) => ({ ...note, content }));
    },
    [updateNote],
  );

  const handleAddHighlight = useCallback(
    (text: string) => {
      if (!text.trim()) return;
      updateNote((note) => ({
        ...note,
        highlights: [
          {
            id: makeId('highlight'),
            text,
            createdAt: nowIso(),
          },
          ...note.highlights,
        ],
      }));
      notify('success', 'Highlight added.');
    },
    [notify, updateNote],
  );

  const handleRemoveHighlight = useCallback(
    (highlightId: string) => {
      updateNote((note) => ({
        ...note,
        highlights: note.highlights.filter((h) => h.id !== highlightId),
      }));
    },
    [updateNote],
  );

  const selectedEntry = useMemo(
    () => entries.find((e) => e.id === selectedId) ?? null,
    [entries, selectedId],
  );

  const selectedEntryMessages = selectedId ? messagesByConversation[selectedId] ?? [] : [];
  const selectedEntryLoading = selectedId ? Boolean(loadingByConversation[selectedId]) : false;

  const handleSynthesize = useJournalSynthesis({
    activeNote,
    entries,
    pinnedIds,
    selectedId,
    notify,
    searchParams,
    setSearchParams,
  });

  useJournalPaletteCommands({
    weekCandidateTotal: weekCandidates?.total,
    isSynthesizing,
    pinnedIds,
    journalSpace,
    handleSynthesize,
    handleNewEntry,
    handleNewPage,
  });

  useJournalShortcuts({ entries, selectedId, setSelectedId, saveNow, handleNewPage, setSidebarCollapsed });

  const journalName = journalSpace?.name ?? 'Journal';
  const pinnedEntryCount = [...pinnedIds].length;
  const deckCount = Math.min(entries.length, RECENT_DECK_SIZE);

  if (topLevelError) {
    return <JournalLoadError error={topLevelError} onRetry={retryJournalLoad} />;
  }

  if (!requestedJournalSpaceId) {
    return (
      <JournalEmptyState
        isCreatingJournal={isCreatingJournal}
        onCreateJournal={() => void handleCreateJournal()}
        notice={notice}
      />
    );
  }

  return (
    <div className="relative flex h-full overflow-hidden bg-[hsl(var(--bg))] text-[hsl(var(--text-primary))]">
      {sidebarCollapsed ? (
        <CollapsedJournalSidebar
          onExpand={() => setSidebarCollapsed(false)}
          onNewPage={() => void handleNewPage()}
        />
      ) : (
        <EntryList
          entriesState={entriesState}
          journals={allJournals}
          currentJournal={journalSpace}
          onSwitchJournal={handleSwitchJournal}
          onCreateJournal={() => void handleCreateJournal()}
          onRenameJournal={handleRenameJournal}
          onDeleteJournal={handleDeleteJournal}
          onNewEntry={(spaceId) => void handleNewEntry(spaceId)}
          onRenameEntry={handleRenameEntry}
          onDeleteEntry={handleDeleteEntry}
          onToggleCollapse={() => setSidebarCollapsed(true)}
          pages={pages}
          activePageId={activeNote?.id ?? null}
          onSelectPage={handleSelectPage}
          onNewPage={() => void handleNewPage()}
          onRenamePage={handleRenamePage}
          onDeletePage={handleDeletePage}
          displayPageTitle={displayPageTitle}
        />
      )}

      <EntryEditor
        activeNote={activeNote}
        isLoadingNote={isLoadingNote}
        loadError={noteLoadError}
        saveError={saveError}
        hasPendingChanges={hasPendingChanges}
        isSaving={isSavingNow}
        onSaveNow={() => void saveNow()}
        onUpdateNoteContent={handleUpdateNoteContent}
        onAddHighlight={handleAddHighlight}
        onRemoveHighlight={handleRemoveHighlight}
        pinnedHighlightIds={pinnedNoteHighlightIds}
        onTogglePinnedHighlight={handleTogglePinnedHighlight}
        journalName={journalName}
        pageTitle={activeNote ? displayPageTitle(activeNote) : null}
        isDefaultPage={activeNote ? displayPageTitle(activeNote) === UNTITLED_PAGE : false}
        autoFocusTitle={Boolean(pageToName === activeNote?.id)}
        onRenamePage={(title) => {
          if (activeNote) void handleRenamePage(activeNote.id, title);
        }}
        selectedEntry={selectedEntry}
        selectedEntryMessages={selectedEntryMessages}
        selectedEntryLoading={selectedEntryLoading}
        pinnedEntryCount={pinnedEntryCount}
        deckCount={deckCount}
        crossEntrySources={sourcesState.summaries}
        crossEntryLoading={sourcesState.isLoading}
        scannedConversationCount={sourcesState.scannedConversationCount}
        onJumpToEntry={(id) => setSelectedId(id)}
        onSynthesize={handleSynthesize}
        weekCandidates={weekCandidates}
        onNotify={notify}
      />

      {notice && <JournalNoticeToast notice={notice} onDismiss={() => setNotice(null)} />}
    </div>
  );
}
