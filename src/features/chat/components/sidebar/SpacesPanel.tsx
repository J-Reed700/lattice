import { useCallback, useEffect, useState } from 'react';
import type { RefObject } from 'react';

import { motion, useReducedMotion } from 'framer-motion';
import {
  Archive,
  Check,
  ChevronRight,
  FolderPlus,
  Loader2,
  MessageSquare,
  Plus,
  Save,
  Settings2,
  X
} from 'lucide-react';

import {
  Button,
  Dialog,
  DialogContent,
  DialogTitle,
  Input,
  Switch,
  Textarea,
} from '@/components/ui';
import { SPACES_MODAL_LAYER_CLASSES } from '@/features/chat/components/sidebar/sidebarUtils';
import type { useNewSpaceForm } from '@/features/chat/components/sidebar/useNewSpaceForm';
import type { useSpaceEditor } from '@/features/spaces/hooks/useSpaceEditor';
import { GENERAL_SPACE_ID, normalizeHexColor } from '@/features/spaces/model/spaces';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import { handleAsyncEvent } from '@/utils/promiseHandlers';

const ARCHITECTURAL_SWATCHES = [
  { name: 'Terracotta', hex: '#aa503d' },
  { name: 'Ochre', hex: '#b58336' },
  { name: 'Sage', hex: '#52796f' },
  { name: 'Slate', hex: '#4b6584' },
  { name: 'Plum', hex: '#7c527a' },
  { name: 'Clay', hex: '#8f624c' },
  { name: 'Pine', hex: '#3d6352' },
  { name: 'Obsidian', hex: '#4a443f' },
] as const;

interface SpacesPanelProps {
  anchorRef: RefObject<HTMLDivElement | null>;
  onClose: () => void;
  editor: ReturnType<typeof useSpaceEditor>;
  newSpace: ReturnType<typeof useNewSpaceForm>;
}

export function SpacesPanel({ anchorRef, onClose, editor, newSpace }: SpacesPanelProps) {
  const prefersReducedMotion = useReducedMotion();
  const spaces = useConversationsStore((state) => state.spaces);
  const selectedSpaceId = useConversationsStore((state) => state.selectedSpaceId);
  const setSelectedSpace = useConversationsStore((state) => state.setSelectedSpace);
  const loadConversations = useConversationsStore((state) => state.loadConversations);
  const selectedSpace = spaces.find(space => space.id === selectedSpaceId) ?? null;
  const activeSpacesOrdered = spaces.filter(space => !space.isArchived);
  const standardSpaces = activeSpacesOrdered;
  const archivedSpaces = spaces.filter(space => space.isArchived);
  const [spacesPanelFrame, setSpacesPanelFrame] = useState<{ left: number; top: number; height: number; width: number } | null>(null);
  const handleSpaceSelect = async (spaceId: string | null) => { setSelectedSpace(spaceId); await loadConversations({ spaceId }); };
  const {
    isCreateSpaceOpen,
    setIsCreateSpaceOpen,
    newSpaceKindDraft,
    setNewSpaceKindDraft,
    newSpaceNameDraft,
    setNewSpaceNameDraft,
    isCreatingSpace,
    createSpace,
  } = newSpace;
  const {
    isSpaceEditorOpen,
    setIsSpaceEditorOpen,
    isSavingSpace,
    isArchivingSpace,
    isRestoringSpace,
    saveSpaceEnvironment,
    setSelectedSpaceArchived,
    restoreArchivedSpace,
    toggleSpaceDeepResearchDefault,
    spaceNameDraft,
    setSpaceNameDraft,
    spaceDescriptionDraft,
    setSpaceDescriptionDraft,
    spaceIconDraft,
    setSpaceIconDraft,
    spaceAccentDraft,
    setSpaceAccentDraft,
    spacePromptDraft,
    setSpacePromptDraft,
    spaceKbDefault,
    setSpaceKbDefault,
    spaceStrictSourcesDefault,
    setSpaceStrictSourcesDefault,
    spaceWebDefault,
    setSpaceWebDefault,
    spaceDeepResearchDefault,
  } = editor;

  const normalizedAccent = normalizeHexColor(spaceAccentDraft)?.toLowerCase() ?? null;

  const updateSpacesPanelFrame = useCallback(() => {
    const sidebarElement = anchorRef.current;
    if (!sidebarElement) return;

    const rect = sidebarElement.getBoundingClientRect();
    const availableWidth = Math.max(220, Math.floor(window.innerWidth - rect.right - 12));
    const width = Math.min(360, availableWidth);

    setSpacesPanelFrame({
      left: Math.round(rect.right),
      top: Math.round(rect.top),
      height: Math.round(rect.height),
      width,
    });
  }, [anchorRef]);

  useEffect(() => {
    updateSpacesPanelFrame();
    const handleLayoutChange = () => updateSpacesPanelFrame();
    window.addEventListener('resize', handleLayoutChange);
    window.addEventListener('scroll', handleLayoutChange, true);

    return () => {
      window.removeEventListener('resize', handleLayoutChange);
      window.removeEventListener('scroll', handleLayoutChange, true);
    };
  }, [updateSpacesPanelFrame]);

  return (
    <Dialog open onOpenChange={(open) => { if (!open) onClose(); }}>
      <DialogContent
        unstyled
        hideClose
        asChild
        aria-describedby={undefined}
        overlayClassName={`overlay-fade fixed inset-0 bg-[hsl(var(--overlay))] ${SPACES_MODAL_LAYER_CLASSES.backdrop}`}
      >
        <motion.aside
          className={`fixed border-r border-border-subtle bg-surface-overlay shadow-lg ${SPACES_MODAL_LAYER_CLASSES.panel}`}
          style={spacesPanelFrame ?? undefined}
          initial={prefersReducedMotion ? false : { opacity: 0, scale: 0.98 }}
          animate={{ opacity: 1, scale: 1 }}
          transition={{ duration: 0.15, ease: [0.22, 1, 0.36, 1] }}
        >
          <div className={`${SPACES_MODAL_LAYER_CLASSES.content} flex h-full flex-col overflow-hidden`}>
            <div className={`flex h-12 shrink-0 items-center justify-between border-b border-border-subtle px-4 ${SPACES_MODAL_LAYER_CLASSES.section}`}>
              <DialogTitle asChild>
                <h3 className="font-sans text-sm font-semibold tracking-[-0.015em] text-text-primary">Spaces</h3>
              </DialogTitle>
              <button
                type="button"
                onClick={() => onClose()}
                className="rounded-lg p-1.5 text-text-muted transition-colors duration-fast hover:bg-surface-sunken hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
                aria-label="Close spaces panel"
                title="Close"
              >
                <X className="h-4 w-4" />
              </button>
            </div>

            <div className={`flex min-h-0 flex-1 flex-col overflow-y-auto px-4 py-3 ${SPACES_MODAL_LAYER_CLASSES.section}`}>
              <div className="flex items-center gap-1.5">
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  onClick={() => setIsCreateSpaceOpen((open) => !open)}
                  className="h-7 gap-1.5 px-2 text-xs text-text-secondary hover:text-text-primary"
                >
                  <FolderPlus className="h-3.5 w-3.5" />
                  {isCreateSpaceOpen ? 'Cancel' : 'New space'}
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  onClick={() => setIsSpaceEditorOpen((open) => !open)}
                  disabled={!selectedSpace}
                  className="h-7 gap-1.5 px-2 text-xs text-text-secondary hover:text-text-primary"
                >
                  <Settings2 className="h-3.5 w-3.5" />
                  Edit space
                </Button>
              </div>

              {isCreateSpaceOpen && (
                <div className="mt-3 space-y-2.5 rounded-xl border border-border-subtle bg-surface p-3">
                  <Input
                    size="sm"
                    value={newSpaceNameDraft}
                    onChange={(e) => setNewSpaceNameDraft(e.target.value)}
                    placeholder={
                      newSpaceKindDraft === 'journal'
                        ? 'Journal name (e.g. Food Research, Weekly Notes)'
                        : 'Space name (e.g. Product, Research, Personal)'
                    }
                  />
                  <div className="flex h-7 items-center gap-0.5 rounded-lg bg-[hsl(var(--text-primary)/0.06)] p-0.5">
                    <button
                      type="button"
                      onClick={() => setNewSpaceKindDraft('standard')}
                      className={`h-full flex-1 rounded-md text-xs font-medium transition-colors duration-fast ${
                        newSpaceKindDraft === 'standard'
                          ? 'bg-surface text-text-primary shadow-sm ring-1 ring-border-subtle dark:bg-surface-overlay'
                          : 'text-text-tertiary hover:text-text-primary'
                      }`}
                    >
                      Standard
                    </button>
                    <button
                      type="button"
                      onClick={() => setNewSpaceKindDraft('journal')}
                      className={`h-full flex-1 rounded-md text-xs font-medium transition-colors duration-fast ${
                        newSpaceKindDraft === 'journal'
                          ? 'bg-surface text-text-primary shadow-sm ring-1 ring-border-subtle dark:bg-surface-overlay'
                          : 'text-text-tertiary hover:text-text-primary'
                      }`}
                    >
                      Journal
                    </button>
                  </div>
                  <div className="flex items-center justify-end gap-1.5">
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      onClick={() => {
                        setIsCreateSpaceOpen(false);
                        setNewSpaceNameDraft('');
                        setNewSpaceKindDraft('standard');
                      }}
                    >
                      Cancel
                    </Button>
                    <Button
                      type="button"
                      size="sm"
                      onClick={handleAsyncEvent(createSpace)}
                      disabled={isCreatingSpace}
                      className="gap-1"
                    >
                      {isCreatingSpace ? (
                        <Loader2 className="h-3.5 w-3.5 animate-spin" />
                      ) : (
                        <Plus className="h-3.5 w-3.5" />
                      )}
                      Create
                    </Button>
                  </div>
                </div>
              )}

              <div className="mt-3 flex min-h-0 flex-1 flex-col">
                <div className="min-h-0 flex-1 space-y-0.5 overflow-y-auto pr-1">
                  <button
                    type="button"
                    onClick={handleAsyncEvent(() => handleSpaceSelect(null))}
                    className={`relative w-full rounded-lg px-3 py-2 text-left transition-[background-color,box-shadow] duration-fast ${
                      selectedSpaceId === null
                        ? 'bg-surface shadow-sm ring-1 ring-border-subtle dark:bg-surface-overlay'
                        : 'row-hover'
                    }`}
                  >
                    <div className="flex items-start gap-2.5">
                      <div className="flex h-6 w-6 shrink-0 items-center justify-center text-text-tertiary">
                        <MessageSquare className="h-4 w-4" />
                      </div>
                      <div className="min-w-0">
                        <p className="truncate text-ui font-medium text-text-primary">All spaces</p>
                        <p className="mt-0.5 text-xs text-text-muted">
                          Every conversation, all scopes
                        </p>
                      </div>
                    </div>
                  </button>

                  {standardSpaces.length > 0 && (
                    <p className="px-2 pb-1 pt-4 text-xs font-medium text-text-muted">
                      Spaces
                    </p>
                  )}

                  {standardSpaces.map((space) => {
                    const isSelected = selectedSpaceId === space.id;
                    const accent = normalizeHexColor(space.accentColor);

                    return (
                      <button
                        key={space.id}
                        type="button"
                        onClick={handleAsyncEvent(() => handleSpaceSelect(space.id))}
                        className={`relative w-full rounded-lg px-3 py-2 text-left transition-[background-color,box-shadow] duration-fast ${
                          isSelected
                            ? 'bg-surface shadow-sm ring-1 ring-border-subtle dark:bg-surface-overlay'
                            : 'row-hover'
                        }`}
                        title={space.description ?? space.name}
                      >
                        <div className="flex items-start gap-2.5">
                          <div className="flex h-6 w-6 shrink-0 items-center justify-center text-sm text-text-tertiary">
                            {space.icon || (
                              accent ? (
                                <span
                                  className="h-2.5 w-2.5 rounded-full"
                                  style={{ backgroundColor: accent }}
                                  aria-hidden="true"
                                />
                              ) : (
                                <MessageSquare className="h-4 w-4" />
                              )
                            )}
                          </div>
                          <div className="min-w-0">
                            <p className="truncate text-ui font-medium text-text-primary">{space.name}</p>
                            {space.description ? (
                              <p className="mt-0.5 line-clamp-2 text-xs text-text-muted">
                                {space.description}
                              </p>
                            ) : null}
                          </div>
                        </div>
                      </button>
                    );
                  })}
                </div>
              </div>

              {isSpaceEditorOpen && selectedSpace && (
                <div className="mt-3 space-y-3 border-t border-border-subtle pt-3">
                  <p className="text-xs font-medium text-text-primary">
                    {selectedSpace.name}
                  </p>

                  <details open className="group border-b border-border-subtle pb-3">
                    <summary className="flex cursor-pointer list-none items-center gap-1.5 text-xs font-medium text-text-secondary transition-colors duration-fast hover:text-text-primary [&::-webkit-details-marker]:hidden">
                      <ChevronRight
                        className="h-3.5 w-3.5 text-text-muted transition-transform duration-fast group-open:rotate-90"
                        strokeWidth={1.75}
                        aria-hidden="true"
                      />
                      Basics
                    </summary>
                    <div className="mt-2.5 space-y-2.5">
                      <div className="grid grid-cols-[64px_1fr] gap-2">
                        <Input
                          size="sm"
                          value={spaceIconDraft}
                          onChange={(e) => setSpaceIconDraft(e.target.value)}
                          placeholder="Icon"
                        />
                        <Input
                          size="sm"
                          value={spaceNameDraft}
                          onChange={(e) => setSpaceNameDraft(e.target.value)}
                          placeholder="Space name"
                        />
                      </div>

                      <div className="space-y-2">
                        <div className="flex items-center justify-between">
                          <span className="text-xs text-text-muted">Accent color</span>
                          <span
                            className="h-3.5 w-3.5 rounded-full border border-border-subtle"
                            style={{ backgroundColor: normalizedAccent ?? 'transparent' }}
                            aria-hidden="true"
                          />
                        </div>
                        <div className="flex flex-wrap items-center gap-1.5" role="group" aria-label="Architectural color swatches">
                          {ARCHITECTURAL_SWATCHES.map((swatch) => {
                            const selected = normalizedAccent === swatch.hex;
                            return (
                              <button
                                key={swatch.hex}
                                type="button"
                                aria-label={`Accent color ${swatch.name}`}
                                aria-pressed={selected}
                                title={`${swatch.name} (${swatch.hex})`}
                                onClick={() => setSpaceAccentDraft(swatch.hex)}
                                className={`flex h-6 w-6 items-center justify-center rounded-full border transition-transform duration-fast hover:scale-105 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-bg ${
                                  selected ? 'border-text-primary ring-1 ring-text-primary/40' : 'border-black/10 dark:border-white/15'
                                }`}
                                style={{ backgroundColor: swatch.hex }}
                              >
                                {selected && <Check className="h-3 w-3 text-white drop-shadow-xs" strokeWidth={2.5} aria-hidden="true" />}
                              </button>
                            );
                          })}
                        </div>
                        <div className="flex items-center gap-1.5">
                          <Input
                            size="sm"
                            value={spaceAccentDraft}
                            onChange={(e) => setSpaceAccentDraft(e.target.value)}
                            placeholder="#aa503d"
                            aria-label="Custom hex color"
                            className="font-mono text-xs"
                          />
                          <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() => setSpaceAccentDraft('')}
                            aria-label="Clear accent color"
                            title="Clear accent color"
                            className="h-7 shrink-0 px-2.5 text-xs"
                          >
                            Clear
                          </Button>
                        </div>
                      </div>

                      <Input
                        size="sm"
                        value={spaceDescriptionDraft}
                        onChange={(e) => setSpaceDescriptionDraft(e.target.value)}
                        placeholder="Description"
                      />
                    </div>
                  </details>

                  <details className="group border-b border-border-subtle pb-3">
                    <summary className="flex cursor-pointer list-none items-center gap-1.5 text-xs font-medium text-text-secondary transition-colors duration-fast hover:text-text-primary [&::-webkit-details-marker]:hidden">
                      <ChevronRight
                        className="h-3.5 w-3.5 text-text-muted transition-transform duration-fast group-open:rotate-90"
                        strokeWidth={1.75}
                        aria-hidden="true"
                      />
                      Defaults
                    </summary>
                    <div className="mt-2.5 space-y-3">
                      <Textarea
                        value={spacePromptDraft}
                        onChange={(e) => setSpacePromptDraft(e.target.value)}
                        placeholder="System prompt for this space"
                        rows={4}
                        className="text-xs"
                      />
                      <div className="space-y-2 rounded-lg border border-border-subtle bg-surface p-2.5">
                        <div className="flex items-center justify-between gap-2">
                          <span className="text-xs text-text-secondary">Search documents by default</span>
                          <Switch
                            aria-label="Search documents by default"
                            checked={spaceKbDefault}
                            onCheckedChange={(checked) => setSpaceKbDefault(() => checked)}
                          />
                        </div>
                        <div className="flex items-center justify-between gap-2" title="Answer strictly from retrieved sources; no general knowledge">
                          <span className="text-xs text-text-secondary">Sources only</span>
                          <Switch
                            aria-label="Sources only"
                            checked={spaceStrictSourcesDefault}
                            onCheckedChange={(checked) => setSpaceStrictSourcesDefault(() => checked)}
                          />
                        </div>
                        <div className="flex items-center justify-between gap-2">
                          <span className="text-xs text-text-secondary">Web by default</span>
                          <Switch
                            aria-label="Web by default"
                            checked={spaceWebDefault}
                            onCheckedChange={(checked) => setSpaceWebDefault(() => checked)}
                          />
                        </div>
                        <div className="flex items-center justify-between gap-2">
                          <span className="text-xs text-text-secondary">Deep research by default</span>
                          <Switch
                            aria-label="Deep research by default"
                            checked={spaceDeepResearchDefault}
                            onCheckedChange={() => toggleSpaceDeepResearchDefault()}
                          />
                        </div>
                      </div>
                      {spaceDeepResearchDefault && (
                        <div
                          role="status"
                          aria-live="polite"
                          className="rounded-lg border border-warning-muted bg-warning-muted px-2.5 py-1.5 text-xs text-warning-fg"
                        >
                          Deep research is on for this space. Responses will be slower.
                        </div>
                      )}
                    </div>
                  </details>

                  <div className="flex items-center gap-2">
                    <Button
                      type="button"
                      size="sm"
                      onClick={handleAsyncEvent(saveSpaceEnvironment)}
                      disabled={isSavingSpace}
                      className="gap-1.5"
                    >
                      {isSavingSpace ? (
                        <Loader2 className="h-3.5 w-3.5 animate-spin" />
                      ) : (
                        <Save className="h-3.5 w-3.5" />
                      )}
                      Save
                    </Button>
                    {selectedSpace.id !== GENERAL_SPACE_ID && (
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        onClick={handleAsyncEvent(() => setSelectedSpaceArchived(!selectedSpace.isArchived))}
                        disabled={isArchivingSpace}
                        className="gap-1.5 text-text-secondary hover:text-text-primary"
                      >
                        {isArchivingSpace ? (
                          <Loader2 className="h-3.5 w-3.5 animate-spin" />
                        ) : (
                          <Archive className="h-3.5 w-3.5" />
                        )}
                        {selectedSpace.isArchived ? 'Restore' : 'Archive'}
                      </Button>
                    )}
                  </div>

                  {archivedSpaces.length > 0 && (
                    <details className="group border-b border-border-subtle pb-3">
                      <summary className="flex cursor-pointer list-none items-center gap-1.5 text-xs text-text-muted transition-colors duration-fast hover:text-text-secondary [&::-webkit-details-marker]:hidden">
                        <ChevronRight
                          className="h-3.5 w-3.5 transition-transform duration-fast group-open:rotate-90"
                          strokeWidth={1.75}
                          aria-hidden="true"
                        />
                        Archived spaces · {archivedSpaces.length}
                      </summary>
                      <div className="mt-2 space-y-1.5">
                        {archivedSpaces.slice(0, 6).map((space) => (
                          <div
                            key={space.id}
                            className="flex items-center justify-between gap-2 rounded-lg border border-border-subtle bg-surface px-2.5 py-1.5"
                          >
                            <span className="truncate text-xs text-text-secondary">
                              {space.icon ? `${space.icon} ` : ''}{space.name}
                            </span>
                            <Button
                              type="button"
                              variant="outline"
                              size="sm"
                              onClick={handleAsyncEvent(() => restoreArchivedSpace(space.id))}
                              disabled={isRestoringSpace}
                              className="h-6 px-2 text-xs"
                            >
                              Restore
                            </Button>
                          </div>
                        ))}
                      </div>
                    </details>
                  )}
                </div>
              )}
            </div>

            <div className={`border-t border-border-subtle px-4 py-2.5 ${SPACES_MODAL_LAYER_CLASSES.section}`}>
              <p className="text-center text-xs text-text-muted">
                {activeSpacesOrdered.length} space{activeSpacesOrdered.length !== 1 ? 's' : ''}
              </p>
            </div>
          </div>
        </motion.aside>
      </DialogContent>
    </Dialog>
  );
}
