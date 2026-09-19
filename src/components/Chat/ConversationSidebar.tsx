import { useEffect, useMemo, useRef, useState } from 'react';

import {
  AlertCircle,
  ChevronDown,
  ListChecks,
  Loader2,
  NotebookPen,
  PanelLeft,
  Plus,
  X
} from 'lucide-react';
import { useNavigate } from 'react-router';

import { IconButton } from '@/components/ui/IconButton';
import { SidebarHeader, SidebarSearch, SidebarTabs } from '@/components/ui/SidebarHeader';
import { useDebounce } from '@/hooks/useDebounce';
import { useConversationsStore } from '@/stores/conversationsStore';
import { createDefaultConversationTitle } from '@/utils/conversationTitles';
import { handleAsyncEvent } from '@/utils/promiseHandlers';


import { ConversationList } from './sidebar/ConversationList';
import { SidebarReferences } from './sidebar/SidebarReferences';
import { SPACES_MODAL_LAYER_CLASSES } from './sidebar/sidebarUtils';
import { SpacesPanel } from './sidebar/SpacesPanel';
import { useConversationExport } from './sidebar/useConversationExport';
import { useConversationSynthesis } from './sidebar/useConversationSynthesis';
import { useJournalSelection } from './sidebar/useJournalSelection';
import { useSpaceEditor } from './sidebar/useSpaceEditor';
import { useJournalsQuery } from './sidebar/workspaceQueries';

interface ConversationSidebarProps {
  /** Hide the sidebar. Owned by ChatView, which also binds ⌘\. */
  onCollapse?: () => void;
}

export function ConversationSidebar({ onCollapse }: ConversationSidebarProps = {}) {
  const navigate = useNavigate();
  const {
    spaces,
    selectedSpaceId,
    filterMode,
    searchQuery,
    conversations,
    error,
    loadConversations,
    setSelectedSpace,
    setFilterMode,
    setSearchQuery,
    createConversation,
    clearError,
  } = useConversationsStore();

  const [isCreating, setIsCreating] = useState(false);
  const [isSpacesOpen, setIsSpacesOpen] = useState(false);
  const sidebarRef = useRef<HTMLDivElement | null>(null);
  const [localQuery, setLocalQuery] = useState(searchQuery);
  const debouncedQuery = useDebounce(localQuery, 250);
  const { journals, isLoading: isLoadingJournals, error: journalsError, refetch } = useJournalsQuery();
  const journalsLoadError = journalsError?.message ?? null;
  const loadJournals = async () => { await refetch(); };
  const journalSpaces = useMemo(() => journals.filter(journal => !journal.isArchived), [journals]);
  const synthesis = useConversationSynthesis();
  // Registers "Copy conversation as Markdown" and "Save conversation to
  // Journal" with the palette for as long as Chat is on screen, and hands the
  // row menu the same two verbs.
  const exportActions = useConversationExport();
  const spaceEditor = useSpaceEditor();
  const {
    isSelectionMode,
    setIsSelectionMode,
    selectedConversationIds,
    selectedConversationCount,
    bulkSpaceIdDraft,
    setBulkSpaceIdDraft,
    isBulkMoving,
    isCreatingQuickJournal,
    areAllVisibleConversationsSelected,
    toggleConversationSelection,
    toggleSelectAllVisibleConversations,
    addSelectedConversationsToJournal,
    createQuickJournal,
    openSelectedJournalNotebook,
  } = useJournalSelection();
  const selectedSpace = useMemo(() => {
    if (!selectedSpaceId) return null;
    return spaces.find((space) => space.id === selectedSpaceId) ?? null;
  }, [selectedSpaceId, spaces]);
  const selectedJournal = useMemo(() => {
    if (!selectedSpaceId) return null;
    return journals.find((journal) => journal.id === selectedSpaceId) ?? null;
  }, [journals, selectedSpaceId]);
  const isJournalScope = Boolean(
    selectedJournal
  );
  const scopeLabel = selectedSpace
    ? `${selectedSpace.icon ? `${selectedSpace.icon} ` : ''}${selectedSpace.name}`
    : selectedJournal
      ? `${selectedJournal.icon ? `${selectedJournal.icon} ` : ''}${selectedJournal.name} · Journal`
      : 'All spaces';

  useEffect(() => {
    setLocalQuery(searchQuery);
  }, [searchQuery]);

  useEffect(() => {
    if (!selectedSpaceId) return;
    if (!selectedJournal) return;
    setSelectedSpace(null);
    void loadConversations({ spaceId: null });
  }, [loadConversations, selectedJournal, selectedSpaceId, setSelectedSpace]);

  useEffect(() => {
    if (debouncedQuery === searchQuery) return;
    setSearchQuery(debouncedQuery);
    void loadConversations({ searchQuery: debouncedQuery });
  }, [debouncedQuery, loadConversations, searchQuery, setSearchQuery]);

  const handleNewConversation = async () => {
    setIsCreating(true);
    try {
      const title = createDefaultConversationTitle();
      await createConversation(title);
    } finally {
      setIsCreating(false);
    }
  };

  const handleFilterSelect = async (
    mode: 'all' | 'saved' | 'bookmarked' | 'pinned' | 'archived' | 'snippets'
  ) => {
    setFilterMode(mode);
    await loadConversations({ filterMode: mode });
  };

  type FilterId = 'all' | 'saved' | 'bookmarked' | 'pinned' | 'archived' | 'snippets';

  const filterOptions: ReadonlyArray<{ id: FilterId; label: string }> = [
    { id: 'all', label: 'All' },
    { id: 'saved', label: 'Starred' },
    { id: 'pinned', label: 'Pinned' },
    { id: 'archived', label: 'Archived' },
    { id: 'snippets', label: 'Referenced' },
  ];

  return (
    <div
      ref={sidebarRef}
      className={`relative flex h-full w-[280px] max-w-full shrink-0 flex-col overflow-hidden border-r border-border-subtle bg-bg ${isSpacesOpen ? SPACES_MODAL_LAYER_CLASSES.root : ''
        }`}
    >
      <SidebarHeader
        title={isJournalScope && selectedJournal ? selectedJournal.name : 'Chat'}
        actions={
          <>
            <IconButton
              label={isJournalScope ? 'New entry' : 'New conversation'}
              shortcut="⌘N"
              onClick={handleAsyncEvent(handleNewConversation)}
              disabled={isCreating}
            >
              {isCreating ? <Loader2 className="animate-spin" /> : <Plus />}
            </IconButton>
            {filterMode !== 'snippets' && (
              <IconButton
                label="Select"
                active={isSelectionMode}
                onClick={() => {
                  const nextSelectionMode = !isSelectionMode;
                  setIsSelectionMode(nextSelectionMode);
                  if (nextSelectionMode) {
                    void loadJournals();
                  }
                }}
              >
                <ListChecks />
              </IconButton>
            )}
            <IconButton label="Hide sidebar" shortcut="⌘\" onClick={onCollapse}>
              <PanelLeft />
            </IconButton>
          </>
        }
      />

      {/* Scope — one line, opens the Spaces panel */}
      <div className="flex shrink-0 items-center gap-1 px-3 pb-1">
        <button
          type="button"
          onClick={() => setIsSpacesOpen(true)}
          aria-label="Change scope"
          className="row-hover flex h-7 min-w-0 flex-1 items-center justify-between gap-2 rounded-md px-2 text-ui text-text-secondary"
        >
          <span className="truncate">{scopeLabel}</span>
          <ChevronDown className="h-3.5 w-3.5 shrink-0 text-text-muted" aria-hidden="true" />
        </button>
        {isJournalScope && selectedJournal && (
          <IconButton label="Open notebook" onClick={openSelectedJournalNotebook}>
            <NotebookPen />
          </IconButton>
        )}
      </div>

      <div className="shrink-0 space-y-2.5 border-b border-border-subtle px-4 pb-0 pt-1.5">
        <SidebarSearch
          value={localQuery}
          onChange={setLocalQuery}
          placeholder={isJournalScope ? 'Search entries' : 'Search conversations'}
        />
        <SidebarTabs
          value={filterMode as FilterId}
          onChange={handleAsyncEvent((id: FilterId) => handleFilterSelect(id))}
          options={filterOptions}
          className="justify-between gap-2"
        />
      </div>

      {isSelectionMode && filterMode !== 'snippets' && (
        <div className="shrink-0 space-y-1.5 border-b border-border-subtle px-4 py-2">
          <div className="flex items-center justify-between gap-2 text-xs">
            <span className="text-text-muted">{selectedConversationCount} selected</span>
            <div className="flex items-center gap-3">
              <button
                type="button"
                onClick={toggleSelectAllVisibleConversations}
                className="text-text-secondary transition-colors duration-fast hover:text-text-primary"
              >
                {areAllVisibleConversationsSelected ? 'Clear' : 'Select all'}
              </button>
              <button
                type="button"
                onClick={() => setIsSelectionMode(false)}
                className="text-text-secondary transition-colors duration-fast hover:text-text-primary"
              >
                Done
              </button>
            </div>
          </div>

          <div className="flex items-center gap-2">
            <select
              value={bulkSpaceIdDraft}
              onChange={(e) => setBulkSpaceIdDraft(e.target.value)}
              disabled={isLoadingJournals}
              aria-label="Move to journal"
              className="h-7 min-w-0 flex-1 rounded-sm border border-border-default bg-bg px-2 text-xs text-text-primary outline-none transition-colors duration-fast focus:border-accent"
            >
              {isLoadingJournals ? (
                <option value="" disabled>
                  Loading…
                </option>
              ) : journalsLoadError ? (
                <option value="" disabled>
                  Couldn't load journals
                </option>
              ) : journalSpaces.length === 0 ? (
                <option value="" disabled>
                  No journals yet
                </option>
              ) : (
                <>
                  <option value="" disabled>
                    Choose a journal
                  </option>
                  {journalSpaces.map((space) => (
                    <option key={space.id} value={space.id}>
                      {space.name}
                    </option>
                  ))}
                </>
              )}
            </select>
            <button
              type="button"
              onClick={handleAsyncEvent(addSelectedConversationsToJournal)}
              disabled={
                selectedConversationCount === 0
                || !bulkSpaceIdDraft
                || isBulkMoving
                || Boolean(journalsLoadError)
                || journalSpaces.length === 0
              }
              className="inline-flex shrink-0 items-center gap-1 text-xs text-text-secondary transition-colors duration-fast hover:text-text-primary disabled:cursor-not-allowed disabled:opacity-50"
            >
              {isBulkMoving ? <Loader2 className="h-3 w-3 animate-spin" /> : null}
              Add
            </button>
          </div>

          {journalsLoadError ? (
            <p className="text-xs text-[hsl(var(--danger-fg))]">
              Couldn't load journals. {journalsLoadError}
            </p>
          ) : journalSpaces.length === 0 ? (
            <div className="flex items-center gap-3 text-xs">
              <button
                type="button"
                onClick={handleAsyncEvent(createQuickJournal)}
                disabled={isCreatingQuickJournal}
                className="text-text-secondary transition-colors duration-fast hover:text-text-primary disabled:opacity-60"
              >
                New journal
              </button>
              <button
                type="button"
                onClick={() => navigate('/journals')}
                className="text-text-secondary transition-colors duration-fast hover:text-text-primary"
              >
                Open Journal
              </button>
            </div>
          ) : null}
        </div>
      )}

      {error && (
        <div className="mx-4 mt-4 flex items-center justify-between gap-3 rounded-sm border border-[hsl(var(--danger-muted))] bg-[hsl(var(--danger-muted))] px-3 py-2">
          <div className="flex items-center gap-2">
            <AlertCircle className="w-4 h-4 text-[hsl(var(--danger-fg))]" />
            <p className="text-xs text-[hsl(var(--danger-fg))]">{error}</p>
          </div>
          <button
            onClick={clearError}
            className="text-[hsl(var(--danger-fg))] opacity-70 transition-opacity hover:opacity-100"
            aria-label="Dismiss error"
            title="Dismiss"
          >
            <X className="w-3 h-3" />
          </button>
        </div>
      )}

      <div className="flex-1 overflow-y-auto [scrollbar-gutter:stable]">
        <SidebarReferences query={debouncedQuery} active={filterMode === 'snippets'} />
        <ConversationList isJournalScope={isJournalScope} isSelectionMode={isSelectionMode} selectedConversationIds={selectedConversationIds} toggleConversationSelection={toggleConversationSelection} synthesis={synthesis} exportActions={exportActions} />
      </div>

      <div className="flex h-8 shrink-0 items-center justify-end border-t border-border-subtle px-4">
        <span className="text-xs text-text-muted">
          {conversations.length} {isJournalScope ? 'entry' : 'conversation'}{conversations.length !== 1 ? 's' : ''}
        </span>
      </div>

      {isSpacesOpen && <SpacesPanel editor={spaceEditor} anchorRef={sidebarRef} onClose={() => setIsSpacesOpen(false)} />}
    </div>
  );
}
