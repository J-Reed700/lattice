import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import {
  Check,
  Ellipsis,
  Layers,
  Pin,
  Plus,
  RotateCcw,
  Search,
  SlidersHorizontal,
  X,
} from 'lucide-react';

import { Popover, PopoverAnchor, PopoverContent } from '@/components/ui/popover';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { conversationUiStore, useConversationUiStore } from '@/features/chat/stores/conversationUiStore';
import { useSpaceMutations, useSpacesQuery } from '@/features/spaces/api/queries';
import {
  buildSpaceToolPreferencesJson,
  GENERAL_SPACE_ID,
  normalizeHexColor,
  uniqueName,
} from '@/features/spaces/model/spaces';
import { useAppNavigate } from '@/hooks/useAppNavigate';
import { cn } from '@/lib/utils';
import { useOptionalConversationsStore } from '@/shared/conversations/conversationsStore';
import { toast } from '@/stores/toastStore';
import type { ConversationSpaceDto } from '@/types';
import { handleAsyncEvent } from '@/utils/promiseHandlers';

import {
  MAX_RAIL_SPACES,
  readPinnedRailSpaceIds,
  resolveVisibleRailSpaces,
  togglePinnedRailSpace,
  writePinnedRailSpaceIds,
} from './railSpacesStorage';

const FALLBACK_DOT_COLORS = [
  '#b45309',
  '#0f766e',
  '#4b6584',
  '#be123c',
  '#4d7c0f',
  '#0369a1',
] as const;

function spaceDotColor(space: ConversationSpaceDto, index: number): string {
  const normalized = normalizeHexColor(space.accentColor);
  if (normalized) return normalized;
  if (space.id === GENERAL_SPACE_ID) return 'hsl(var(--text-tertiary))';
  return FALLBACK_DOT_COLORS[index % FALLBACK_DOT_COLORS.length];
}

function SpaceGlyph({
  space,
  index,
  isActive,
}: {
  space: ConversationSpaceDto;
  index: number;
  isActive: boolean;
}) {
  if (space.icon?.trim()) {
    return (
      <span
        className="relative flex h-4 w-4 shrink-0 items-center justify-center text-[13px] leading-none"
        aria-hidden="true"
      >
        {space.icon.trim()}
      </span>
    );
  }

  return (
    <span
      aria-hidden="true"
      className={cn(
        'relative h-2 w-2 shrink-0 rounded-full transition-transform duration-fast',
        isActive && 'scale-110 ring-2 ring-accent/35 ring-offset-1 ring-offset-chrome',
      )}
      style={{ backgroundColor: spaceDotColor(space, index) }}
    />
  );
}

export interface RailSpacesSectionProps {
  /** Whether the sidebar is collapsed to the compact 60px rail. */
  compact: boolean;
  /** Whether the user is currently viewing the Chat route. */
  isOnChat: boolean;
}

/**
 * RailSpacesSection — calm, capped Spaces switcher embedded in the left rail.
 * Displays at most 5 spaces directly in the rail, synced with `conversationUiStore`,
 * with a Popover for searching all spaces, pinning/unpinning up to 5, and quick creation.
 */
export function RailSpacesSection({ compact, isOnChat }: RailSpacesSectionProps) {
  const navigate = useAppNavigate();
  const { data: spacesData } = useSpacesQuery();
  const mutations = useSpaceMutations();

  const selectedSpaceId = useConversationUiStore((state) => state.selectedSpaceId);
  const setSelectedSpaceFromStore = useOptionalConversationsStore((state) => state.setSelectedSpace);
  const loadConversationsFromStore = useOptionalConversationsStore((state) => state.loadConversations);

  const [pinnedIds, setPinnedIds] = useState<string[] | null>(readPinnedRailSpaceIds);
  const [popoverOpen, setPopoverOpen] = useState(false);
  const [filterQuery, setFilterQuery] = useState('');
  const [isCreatingFormOpen, setIsCreatingFormOpen] = useState(false);
  const [newSpaceName, setNewSpaceName] = useState('');
  const [pinLimitNotice, setPinLimitNotice] = useState(false);
  const createInputRef = useRef<HTMLInputElement>(null);

  const openSpaces = useMemo(
    () =>
      [...(spacesData ?? [])]
        .filter((space) => !space.isArchived)
        .sort((a, b) => a.sortOrder - b.sortOrder || a.name.localeCompare(b.name)),
    [spacesData],
  );

  const resolved = useMemo(
    () => resolveVisibleRailSpaces(openSpaces, pinnedIds),
    [openSpaces, pinnedIds],
  );

  const filteredSpaces = useMemo(() => {
    const q = filterQuery.trim().toLowerCase();
    if (!q) return openSpaces;
    return openSpaces.filter(
      (space) =>
        space.name.toLowerCase().includes(q) ||
        (space.description?.toLowerCase().includes(q) ?? false),
    );
  }, [filterQuery, openSpaces]);

  useEffect(() => {
    if (!popoverOpen) {
      setFilterQuery('');
      setIsCreatingFormOpen(false);
      setNewSpaceName('');
      setPinLimitNotice(false);
    }
  }, [popoverOpen]);

  useEffect(() => {
    if (isCreatingFormOpen) {
      createInputRef.current?.focus();
    }
  }, [isCreatingFormOpen]);

  const wide = (classes: string) => (compact ? null : classes);

  const applySpaceSelection = useCallback(
    (spaceId: string | null) => {
      if (setSelectedSpaceFromStore) {
        setSelectedSpaceFromStore(spaceId);
      } else {
        conversationUiStore.setState({ selectedSpaceId: spaceId });
      }
      if (isOnChat) {
        if (loadConversationsFromStore) {
          void loadConversationsFromStore({ spaceId });
        }
      } else {
        navigate('/chat');
      }
    },
    [isOnChat, loadConversationsFromStore, navigate, setSelectedSpaceFromStore],
  );

  const handleRailSpaceClick = useCallback(
    (space: ConversationSpaceDto) => {
      if (isOnChat && selectedSpaceId === space.id) {
        applySpaceSelection(null);
      } else {
        applySpaceSelection(space.id);
      }
    },
    [applySpaceSelection, isOnChat, selectedSpaceId],
  );

  const handlePopoverSpaceSelect = useCallback(
    (spaceId: string | null) => {
      setPopoverOpen(false);
      applySpaceSelection(spaceId);
    },
    [applySpaceSelection],
  );

  const handleTogglePin = useCallback(
    (spaceId: string) => {
      const { next, atLimit } = togglePinnedRailSpace(spaceId, openSpaces, pinnedIds);
      if (atLimit) {
        setPinLimitNotice(true);
        return;
      }
      setPinLimitNotice(false);
      setPinnedIds(next);
      writePinnedRailSpaceIds(next);
    },
    [openSpaces, pinnedIds],
  );

  const handleResetPins = useCallback(() => {
    setPinLimitNotice(false);
    setPinnedIds(null);
    writePinnedRailSpaceIds(null);
  }, []);

  const handleCreateSpace = useCallback(async () => {
    if (mutations.create.isPending) return;
    const name = uniqueName(
      newSpaceName,
      openSpaces.map((space) => space.name),
      'Space',
    );
    try {
      const created = await mutations.create.mutateAsync({
        name,
        description: null,
        icon: null,
        accentColor: null,
        spacePrompt: null,
        toolPreferencesJson: buildSpaceToolPreferencesJson({
          knowledgeBase: false,
          strictSources: false,
          webSearch: false,
          deepResearchMode: false,
        }),
      });
      if (pinnedIds !== null && pinnedIds.length < MAX_RAIL_SPACES) {
        const nextPinned = [...pinnedIds, created.id];
        setPinnedIds(nextPinned);
        writePinnedRailSpaceIds(nextPinned);
      }
      setNewSpaceName('');
      setIsCreatingFormOpen(false);
      setPopoverOpen(false);
      applySpaceSelection(created.id);
      toast.success('Space created', { message: name });
    } catch (error) {
      toast.error("Couldn't create space", {
        message: error instanceof Error ? error.message : String(error),
      });
    }
  }, [applySpaceSelection, mutations.create, newSpaceName, openSpaces, pinnedIds]);

  const atPinCap = resolved.visibleIds.size >= MAX_RAIL_SPACES;

  const popoverBody = (
    <PopoverContent
      side="right"
      align="center"
      sideOffset={10}
      collisionPadding={16}
      className="w-[308px] rounded-xl bg-surface-overlay p-0 shadow-lg"
    >
      {/* Header */}
      <div className="flex items-center justify-between gap-2 border-b border-border-subtle px-3.5 py-2.5">
        <div className="min-w-0">
          <div className="text-ui font-medium text-text-primary">Spaces</div>
          <p className="text-[11px] tabular-nums text-text-muted">
            {openSpaces.length === 0
              ? 'Group chats and sources by topic'
              : `Showing ${resolved.visible.length} of ${openSpaces.length} in sidebar`}
          </p>
        </div>
        {resolved.isCustomized ? (
          <button
            type="button"
            onClick={handleResetPins}
            className="inline-flex h-6 items-center gap-1 rounded-md px-1.5 text-[11px] text-text-tertiary transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
          >
            <RotateCcw className="h-3 w-3" strokeWidth={1.6} />
            Reset
          </button>
        ) : null}
      </div>

      {/* Filter input */}
      {openSpaces.length > 0 ? (
        <div className="border-b border-border-subtle px-2.5 py-2">
          <div className="relative flex items-center">
            <Search
              className="pointer-events-none absolute left-2.5 h-3.5 w-3.5 text-text-muted"
              strokeWidth={1.6}
            />
            <input
              type="text"
              value={filterQuery}
              onChange={(event) => setFilterQuery(event.target.value)}
              placeholder="Filter spaces…"
              aria-label="Filter spaces"
              className="h-8 w-full rounded-lg border border-border-subtle bg-surface pl-8 pr-7 text-xs text-text-primary placeholder:text-text-muted focus:border-border-default focus:outline-hidden focus:ring-2 focus:ring-ring/40"
            />
            {filterQuery ? (
              <button
                type="button"
                onClick={() => setFilterQuery('')}
                aria-label="Clear space filter"
                className="absolute right-1.5 flex h-5 w-5 items-center justify-center rounded-sm text-text-muted hover:text-text-primary"
              >
                <X className="h-3 w-3" strokeWidth={1.75} />
              </button>
            ) : null}
          </div>
        </div>
      ) : null}

      {/* Spaces list */}
      <div className="max-h-[340px] overflow-y-auto p-1.5">
        {openSpaces.length > 0 && (!filterQuery.trim() || 'all spaces'.includes(filterQuery.trim().toLowerCase())) ? (
          <button
            type="button"
            onClick={() => handlePopoverSpaceSelect(null)}
            className={cn(
              'mb-1 flex h-8 w-full items-center gap-2.5 rounded-lg px-2 text-left text-ui transition-colors duration-fast focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring',
              selectedSpaceId === null
                ? 'bg-accent-muted font-medium text-text-primary'
                : 'text-text-secondary hover:bg-[hsl(var(--text-primary)/0.055)] hover:text-text-primary',
            )}
          >
            <Layers
              className={cn('h-3.5 w-3.5 shrink-0', selectedSpaceId === null ? 'text-accent' : 'text-text-muted')}
              strokeWidth={1.6}
            />
            <span className="flex-1 truncate">All spaces</span>
            {selectedSpaceId === null ? (
              <Check className="h-3.5 w-3.5 shrink-0 text-accent" strokeWidth={1.8} />
            ) : null}
          </button>
        ) : null}

        {openSpaces.length === 0 ? (
          <p className="px-2.5 py-4 text-center text-xs leading-relaxed text-text-muted">
            No spaces yet. Create one below to keep a project’s chats and documents together.
          </p>
        ) : filteredSpaces.length === 0 ? (
          <p className="px-2.5 py-4 text-center text-xs text-text-muted">No matching spaces.</p>
        ) : (
          <div className="flex flex-col gap-0.5">
            {filteredSpaces.map((space, index) => {
              const isSelected = selectedSpaceId === space.id;
              const isPinned = resolved.visibleIds.has(space.id);
              const pinDisabled = !isPinned && atPinCap;
              return (
                <div
                  key={space.id}
                  className={cn(
                    'group flex items-center gap-1 rounded-lg pr-1 transition-colors duration-fast',
                    isSelected
                      ? 'bg-accent-muted/85'
                      : 'hover:bg-[hsl(var(--text-primary)/0.055)]',
                  )}
                >
                  <button
                    type="button"
                    onClick={() => handlePopoverSpaceSelect(space.id)}
                    className="flex min-w-0 flex-1 items-center gap-2.5 rounded-lg px-2 py-1.5 text-left focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
                  >
                    <SpaceGlyph space={space} index={index} isActive={isSelected} />
                    <span className="min-w-0 flex-1">
                      <span
                        className={cn(
                          'block truncate text-ui',
                          isSelected ? 'font-medium text-text-primary' : 'text-text-secondary group-hover:text-text-primary',
                        )}
                      >
                        {space.name}
                      </span>
                      {space.description ? (
                        <span className="block truncate text-[11px] text-text-muted">
                          {space.description}
                        </span>
                      ) : null}
                    </span>
                    {isSelected ? (
                      <Check className="h-3.5 w-3.5 shrink-0 text-accent" strokeWidth={1.8} />
                    ) : null}
                  </button>

                  <button
                    type="button"
                    onClick={() => handleTogglePin(space.id)}
                    disabled={pinDisabled}
                    aria-label={isPinned ? `Unpin ${space.name} from sidebar` : `Pin ${space.name} to sidebar`}
                    aria-pressed={isPinned}
                    title={
                      isPinned
                        ? 'Pinned in sidebar — click to unpin'
                        : pinDisabled
                          ? `Sidebar limit reached (${MAX_RAIL_SPACES} max) — unpin a space first`
                          : 'Pin to sidebar'
                    }
                    className={cn(
                      'flex h-7 w-7 shrink-0 items-center justify-center rounded-md transition-colors duration-fast focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring',
                      isPinned
                        ? 'text-accent hover:bg-accent/12'
                        : pinDisabled
                          ? 'cursor-not-allowed text-text-disabled opacity-50'
                          : 'text-text-muted opacity-75 hover:bg-[hsl(var(--text-primary)/0.07)] hover:text-text-primary hover:opacity-100',
                    )}
                  >
                    <Pin
                      className={cn('h-3.5 w-3.5', isPinned && 'fill-current')}
                      strokeWidth={isPinned ? 1.7 : 1.5}
                    />
                  </button>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Fixed cap explanation strip (always visible without scrolling) */}
      {pinLimitNotice || (atPinCap && openSpaces.length > MAX_RAIL_SPACES) ? (
        <div className="border-t border-border-subtle bg-surface-sunken/35 px-3.5 py-1.5 text-[11px] text-text-muted">
          Up to {MAX_RAIL_SPACES} spaces pinned in sidebar. Unpin one to swap.
        </div>
      ) : null}

      {/* Footer: inline quick create */}
      <div className="border-t border-border-subtle bg-surface-sunken/50 p-2">
        {isCreatingFormOpen ? (
          <form
            onSubmit={handleAsyncEvent(async (event) => {
              event.preventDefault();
              await handleCreateSpace();
            })}
            className="flex items-center gap-1.5"
          >
            <input
              ref={createInputRef}
              type="text"
              value={newSpaceName}
              onChange={(event) => setNewSpaceName(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Escape') {
                  event.stopPropagation();
                  setIsCreatingFormOpen(false);
                }
              }}
              placeholder="Space name…"
              aria-label="New space name"
              disabled={mutations.create.isPending}
              className="h-8 min-w-0 flex-1 rounded-lg border border-border-default bg-surface px-2.5 text-xs text-text-primary placeholder:text-text-muted focus:outline-hidden focus:ring-2 focus:ring-ring/40"
            />
            <button
              type="submit"
              disabled={mutations.create.isPending}
              className="inline-flex h-8 shrink-0 items-center rounded-lg bg-action px-2.5 text-xs font-medium text-action-fg transition-colors duration-fast hover:bg-action-hover disabled:opacity-50"
            >
              {mutations.create.isPending ? 'Adding…' : 'Add'}
            </button>
          </form>
        ) : (
          <button
            type="button"
            onClick={() => setIsCreatingFormOpen(true)}
            className="flex h-8 w-full items-center gap-2 rounded-lg px-2 text-xs font-medium text-text-secondary transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
          >
            <Plus className="h-3.5 w-3.5 text-accent" strokeWidth={1.75} />
            <span>New space</span>
          </button>
        )}
      </div>
    </PopoverContent>
  );

  return (
    <Popover open={popoverOpen} onOpenChange={setPopoverOpen}>
      <PopoverAnchor asChild>
        <section aria-label="Spaces" className="flex w-full flex-col gap-px">
          {/* Expanded header row */}
          <div
            className={cn(
              'mb-1 hidden items-center justify-between px-2.5',
              wide('xl:flex'),
            )}
          >
            <span className="text-[11px] font-medium tracking-[0.03em] text-text-muted">
              Spaces
            </span>
            <div className="flex items-center gap-1">
              {selectedSpaceId ? (
                <button
                  type="button"
                  onClick={() => applySpaceSelection(null)}
                  aria-label="Show all spaces"
                  className="rounded px-1.5 py-0.5 text-[11px] text-accent transition-colors duration-fast hover:bg-accent-muted focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
                >
                  All
                </button>
              ) : null}
              <button
                type="button"
                onClick={() => setPopoverOpen((open) => !open)}
                aria-label="Manage sidebar spaces"
                aria-expanded={popoverOpen}
                aria-haspopup="dialog"
                title="Manage spaces"
                className="flex h-5 w-5 items-center justify-center rounded-md text-text-muted transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.07)] hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
              >
                <SlidersHorizontal className="h-3 w-3" strokeWidth={1.7} />
              </button>
            </div>
          </div>

          {/* Empty state */}
          {openSpaces.length === 0 ? (
            <Tooltip delayDuration={500}>
              <TooltipTrigger asChild>
                <button
                  type="button"
                  onClick={() => {
                    setPopoverOpen(true);
                    setIsCreatingFormOpen(true);
                  }}
                  aria-label="New space"
                  aria-expanded={popoverOpen}
                  aria-haspopup="dialog"
                  className={cn(
                    'group relative flex h-8 w-full items-center justify-center gap-2.5 rounded-lg px-2.5 text-text-muted transition-colors duration-fast hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring',
                    wide('xl:justify-start'),
                  )}
                >
                  <span aria-hidden="true" className="row-hover absolute inset-0 rounded-lg" />
                  <Plus className="relative h-3.5 w-3.5 shrink-0" strokeWidth={1.6} />
                  <span className={cn('relative hidden text-ui', wide('xl:block'))}>New space</span>
                </button>
              </TooltipTrigger>
              <TooltipContent side="right" sideOffset={10} className={compact ? undefined : 'xl:hidden'}>
                New space
              </TooltipContent>
            </Tooltip>
          ) : (
            <>
              {/* Visible spaces (max 5) */}
              {resolved.visible.map((space, index) => {
                const isSelected = selectedSpaceId === space.id;
                const isActivePill = isOnChat && isSelected;
                return (
                  <Tooltip key={space.id} delayDuration={500}>
                    <TooltipTrigger asChild>
                      <button
                        type="button"
                        onClick={() => handleRailSpaceClick(space)}
                        aria-label={`Space: ${space.name}`}
                        aria-pressed={isSelected}
                        className={cn(
                          'group relative flex h-8 w-full items-center justify-center gap-2.5 rounded-lg px-2.5 transition-colors duration-fast focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring',
                          wide('xl:justify-start'),
                          isActivePill
                            ? 'bg-surface text-text-primary shadow-sm ring-1 ring-border-subtle dark:bg-surface-overlay'
                            : isSelected
                              ? 'text-text-primary'
                              : 'text-text-tertiary hover:text-text-primary',
                        )}
                      >
                        {!isActivePill ? (
                          <span aria-hidden="true" className="row-hover absolute inset-0 rounded-lg" />
                        ) : null}
                        <span className="relative flex h-4 w-4 shrink-0 items-center justify-center">
                          <SpaceGlyph space={space} index={index} isActive={isSelected} />
                        </span>
                        <span
                          className={cn(
                            'relative hidden min-w-0 flex-1 truncate text-left text-ui',
                            wide('xl:block'),
                            isSelected && 'font-medium',
                          )}
                        >
                          {space.name}
                        </span>
                        {isActivePill ? (
                          <Check
                            aria-hidden="true"
                            className={cn('relative hidden h-3 w-3 shrink-0 text-accent', wide('xl:block'))}
                            strokeWidth={2}
                          />
                        ) : isSelected ? (
                          <span
                            aria-hidden="true"
                            className={cn('relative hidden h-1.5 w-1.5 shrink-0 rounded-full bg-accent/65', wide('xl:block'))}
                          />
                        ) : null}
                      </button>
                    </TooltipTrigger>
                    <TooltipContent side="right" sideOffset={10} className={compact ? undefined : 'xl:hidden'}>
                      <span className="flex items-center gap-1.5">
                        <span>{space.name}</span>
                        {isActivePill ? (
                          <span className="text-[11px] text-accent">Active</span>
                        ) : isSelected ? (
                          <span className="text-[11px] text-text-muted">Chat filter</span>
                        ) : null}
                      </span>
                    </TooltipContent>
                  </Tooltip>
                );
              })}

              {/* Single responsive overflow / manage trigger */}
              {resolved.hiddenCount > 0 ? (
                <Tooltip delayDuration={500}>
                  <TooltipTrigger asChild>
                    <button
                      type="button"
                      onClick={() => setPopoverOpen((open) => !open)}
                      aria-label={`More spaces (${resolved.hiddenCount} more)`}
                      aria-expanded={popoverOpen}
                      aria-haspopup="dialog"
                      className={cn(
                        'group relative flex h-8 w-full items-center justify-center gap-2.5 rounded-lg px-2 text-text-muted transition-colors duration-fast hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring',
                        wide('xl:justify-start xl:px-2.5'),
                      )}
                    >
                      <span aria-hidden="true" className="row-hover absolute inset-0 rounded-lg" />
                      <span className={cn('relative text-[11px] font-medium tabular-nums', wide('xl:hidden'))}>
                        +{resolved.hiddenCount}
                      </span>
                      <Ellipsis className={cn('relative hidden h-4 w-4 shrink-0', wide('xl:block'))} strokeWidth={1.6} />
                      <span className={cn('relative hidden flex-1 truncate text-left text-ui', wide('xl:block'))}>
                        More spaces
                      </span>
                      <span
                        className={cn(
                          'relative hidden rounded-md bg-[hsl(var(--text-primary)/0.06)] px-1.5 py-0.5 text-[11px] font-medium tabular-nums text-text-tertiary',
                          wide('xl:inline-block'),
                        )}
                      >
                        +{resolved.hiddenCount}
                      </span>
                    </button>
                  </TooltipTrigger>
                  <TooltipContent side="right" sideOffset={10} className={compact ? undefined : 'xl:hidden'}>
                    {resolved.hiddenCount} more spaces — manage
                  </TooltipContent>
                </Tooltip>
              ) : (
                <Tooltip delayDuration={500}>
                  <TooltipTrigger asChild>
                    <button
                      type="button"
                      onClick={() => setPopoverOpen((open) => !open)}
                      aria-label="Manage spaces"
                      aria-expanded={popoverOpen}
                      aria-haspopup="dialog"
                      className={cn(
                        'group relative flex h-8 w-full items-center justify-center rounded-lg px-2 text-text-muted transition-colors duration-fast hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring',
                        wide('xl:hidden'),
                      )}
                    >
                      <span aria-hidden="true" className="row-hover absolute inset-0 rounded-lg" />
                      <SlidersHorizontal className="relative h-3.5 w-3.5" strokeWidth={1.6} />
                    </button>
                  </TooltipTrigger>
                  <TooltipContent side="right" sideOffset={10}>
                    Manage spaces
                  </TooltipContent>
                </Tooltip>
              )}
            </>
          )}
        </section>
      </PopoverAnchor>
      {popoverBody}
    </Popover>
  );
}
