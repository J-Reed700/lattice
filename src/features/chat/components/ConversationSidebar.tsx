import { useEffect, useMemo, useRef, useState } from 'react';

import {
  AlertCircle,
  Archive,
  Bookmark,
  Check,
  ChevronDown,
  ListChecks,
  Loader2,
  NotebookPen,
  PanelLeft,
  Plus,
  SlidersHorizontal,
  X
} from 'lucide-react';

import { IconButton, Popover, PopoverContent, PopoverTrigger } from '@/components/ui';
import { SidebarHeader, SidebarSearch } from '@/components/ui/SidebarHeader';
import { ConversationList } from '@/features/chat/components/sidebar/ConversationList';
import { SidebarReferences } from '@/features/chat/components/sidebar/SidebarReferences';
import { SPACES_MODAL_LAYER_CLASSES } from '@/features/chat/components/sidebar/sidebarUtils';
import { SpacesPanel } from '@/features/chat/components/sidebar/SpacesPanel';
import { useConversationSynthesis } from '@/features/chat/components/sidebar/useConversationSynthesis';
import { useJournalSelection } from '@/features/chat/components/sidebar/useJournalSelection';
import { useNewSpaceForm } from '@/features/chat/components/sidebar/useNewSpaceForm';
import { useConversationExport } from '@/features/chat/hooks/useConversationExport';
import { useJournalsQuery } from '@/features/chat/hooks/workspaceQueries';
import { useSpaceEditor } from '@/features/spaces/hooks/useSpaceEditor';
import { useAppNavigate } from '@/hooks/useAppNavigate';
import { useDebounce } from '@/hooks/useDebounce';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import { createDefaultConversationTitle } from '@/utils/conversationTitles';
import { handleAsyncEvent } from '@/utils/promiseHandlers';

interface ConversationSidebarProps {
  /** Hide the sidebar. Owned by ChatView, which also binds ⌘\. */
  onCollapse?: () => void;
}

export function ConversationSidebar({ onCollapse }: ConversationSidebarProps = {}) {
  const navigate = useAppNavigate();
  const spaces = useConversationsStore((state) => state.spaces);
  const selectedSpaceId = useConversationsStore((state) => state.selectedSpaceId);
  const filterMode = useConversationsStore((state) => state.filterMode);
  const searchQuery = useConversationsStore((state) => state.searchQuery);
  const conversations = useConversationsStore((state) => state.conversations);
  const error = useConversationsStore((state) => state.error);
  const loadConversations = useConversationsStore((state) => state.loadConversations);
  const setSelectedSpace = useConversationsStore((state) => state.setSelectedSpace);
  const setFilterMode = useConversationsStore((state) => state.setFilterMode);
  const setSearchQuery = useConversationsStore((state) => state.setSearchQuery);
  const createConversation = useConversationsStore((state) => state.createConversation);
  const clearError = useConversationsStore((state) => state.clearError);

  const [isCreating, setIsCreating] = useState(false);
  const [isSpacesOpen, setIsSpacesOpen] = useState(false);
  const [isFilterOverflowOpen, setIsFilterOverflowOpen] = useState(false);
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
  const newSpace = useNewSpaceForm(spaceEditor);
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

  const primaryFilterOptions: ReadonlyArray<{ id: FilterId; label: string }> = [
    { id: 'all', label: 'All' },
    { id: 'saved', label: 'Starred' },
    { id: 'pinned', label: 'Pinned' },
  ];

  const overflowFilterOptions: ReadonlyArray<{ id: 'archived' | 'snippets'; label: string; icon: typeof Archive }> = [
    { id: 'archived', label: 'Archived', icon: Archive },
    { id: 'snippets', label: 'Referenced', icon: Bookmark },
  ];

  const activeOverflowOption = overflowFilterOptions.find((option) => option.id === filterMode) ?? null;
  const hasActivePrimaryFilter = primaryFilterOptions.some((option) => option.id === filterMode);

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

      <div className="shrink-0 space-y-2 border-b border-border-subtle px-4 pb-3 pt-1.5">
        <SidebarSearch
          value={localQuery}
          onChange={setLocalQuery}
          placeholder={isJournalScope ? 'Search entries' : 'Search conversations'}
        />
        <div className="flex items-center gap-1.5">
          <div
            role="tablist"
            aria-label="Conversation filters"
            className="flex h-8 min-w-0 flex-1 items-center gap-0.5 rounded-lg bg-[hsl(var(--text-primary)/0.06)] p-[3px]"
          >
            {primaryFilterOptions.map((option) => {
              const isActive = option.id === filterMode;
              return (
                <button
                  key={option.id}
                  type="button"
                  role="tab"
                  aria-selected={isActive}
                  tabIndex={isActive || (!hasActivePrimaryFilter && option.id === 'all') ? 0 : -1}
                  onClick={handleAsyncEvent(() => handleFilterSelect(option.id))}
                  onKeyDown={(event) => {
                    const index = primaryFilterOptions.findIndex((item) => item.id === option.id);
                    const nextIndex = event.key === 'ArrowRight' ? (index + 1) % primaryFilterOptions.length
                      : event.key === 'ArrowLeft' ? (index - 1 + primaryFilterOptions.length) % primaryFilterOptions.length
                        : event.key === 'Home' ? 0 : event.key === 'End' ? primaryFilterOptions.length - 1 : null;
                    if (nextIndex === null) return;
                    event.preventDefault();
                    void handleFilterSelect(primaryFilterOptions[nextIndex].id);
                    event.currentTarget.parentElement?.querySelectorAll<HTMLButtonElement>('[role="tab"]')[nextIndex]?.focus();
                  }}
                  className={`flex h-full min-w-0 flex-1 items-center justify-center whitespace-nowrap rounded-md px-2 text-xs font-medium transition-[background-color,color,box-shadow] duration-fast focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring ${
                    isActive
                      ? 'bg-surface-overlay text-text-primary shadow-control'
                      : 'text-text-tertiary hover:text-text-primary'
                  }`}
                >
                  {option.label}
                </button>
              );
            })}
          </div>

          <Popover open={isFilterOverflowOpen} onOpenChange={setIsFilterOverflowOpen}>
            <PopoverTrigger asChild>
              <button
                type="button"
                aria-label={activeOverflowOption ? `More filters (${activeOverflowOption.label})` : 'More conversation filters'}
                aria-haspopup="menu"
                title="Archived & Referenced"
                className={`inline-flex h-8 shrink-0 items-center justify-center gap-1 rounded-lg border px-2 text-xs font-medium transition-colors duration-fast focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring ${
                  activeOverflowOption
                    ? 'border-accent/35 bg-accent-muted text-accent'
                    : 'border-transparent bg-[hsl(var(--text-primary)/0.06)] text-text-tertiary hover:text-text-primary'
                }`}
              >
                {activeOverflowOption ? (
                  <>
                    <span>{activeOverflowOption.label}</span>
                    <ChevronDown className="h-3 w-3 shrink-0 opacity-75" aria-hidden="true" />
                  </>
                ) : (
                  <SlidersHorizontal className="h-3.5 w-3.5 shrink-0" strokeWidth={1.75} aria-hidden="true" />
                )}
              </button>
            </PopoverTrigger>
            <PopoverContent align="end" className="w-44 rounded-xl p-1">
              <div role="menu" aria-label="More conversation filters" className="flex flex-col">
                {overflowFilterOptions.map((option) => {
                  const Icon = option.icon;
                  const isActive = filterMode === option.id;
                  return (
                    <button
                      key={option.id}
                      type="button"
                      role="menuitemradio"
                      aria-checked={isActive}
                      onClick={() => {
                        setIsFilterOverflowOpen(false);
                        void handleFilterSelect(option.id);
                      }}
                      className={`flex w-full items-center justify-between gap-2 rounded-lg px-2.5 py-1.5 text-left text-ui transition-colors duration-fast hover:bg-surface ${
                        isActive ? 'font-medium text-text-primary' : 'text-text-secondary'
                      }`}
                    >
                      <span className="flex items-center gap-2">
                        <Icon className="h-3.5 w-3.5 shrink-0 text-text-tertiary" strokeWidth={1.75} aria-hidden="true" />
                        {option.label}
                      </span>
                      {isActive ? <Check className="h-3.5 w-3.5 shrink-0 text-accent" aria-hidden="true" /> : null}
                    </button>
                  );
                })}
              </div>
            </PopoverContent>
          </Popover>
        </div>
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
              className="h-7 min-w-0 flex-1 rounded-sm border border-border-default bg-bg px-2 text-xs text-text-primary outline-hidden transition-colors duration-fast focus:border-accent"
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

      <div className="flex-1 overflow-y-auto scrollbar-gutter-stable">
        <SidebarReferences query={debouncedQuery} active={filterMode === 'snippets'} />
        <ConversationList isJournalScope={isJournalScope} isSelectionMode={isSelectionMode} selectedConversationIds={selectedConversationIds} toggleConversationSelection={toggleConversationSelection} synthesis={synthesis} exportActions={exportActions} />
      </div>

      <div className="flex h-8 shrink-0 items-center justify-end border-t border-border-subtle px-4">
        <span className="text-xs text-text-muted">
          {conversations.length} {isJournalScope
            ? (conversations.length === 1 ? 'entry' : 'entries')
            : (conversations.length === 1 ? 'conversation' : 'conversations')}
        </span>
      </div>

      {isSpacesOpen && <SpacesPanel editor={spaceEditor} newSpace={newSpace} anchorRef={sidebarRef} onClose={() => setIsSpacesOpen(false)} />}
    </div>
  );
}
