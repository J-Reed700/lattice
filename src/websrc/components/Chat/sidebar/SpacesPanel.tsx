import { useCallback, useEffect, useState } from 'react';
import type { RefObject } from 'react';

import { motion, useReducedMotion } from 'framer-motion';
import {
  Archive,
  FolderPlus,
  Loader2,
  MessageSquare,
  Plus,
  Save,
  Settings2,
  X
} from 'lucide-react';
import { createPortal } from 'react-dom';

import { useConversationsStore } from '@/stores/conversationsStore';
import { handleAsyncEvent } from '@/utils/promiseHandlers';

import { normalizeHexColor, SPACES_MODAL_LAYER_CLASSES } from './sidebarUtils';

import type { useSpaceEditor } from './useSpaceEditor';

interface SpacesPanelProps { anchorRef: RefObject<HTMLDivElement | null>; onClose: () => void; editor: ReturnType<typeof useSpaceEditor> }
export function SpacesPanel({ anchorRef, onClose, editor }: SpacesPanelProps) {
  const prefersReducedMotion = useReducedMotion();
  const { spaces, selectedSpaceId, setSelectedSpace, loadConversations } = useConversationsStore();
  const selectedSpace = spaces.find(space => space.id === selectedSpaceId) ?? null;
  const activeSpacesOrdered = spaces.filter(space => !space.isArchived);
  const standardSpaces = activeSpacesOrdered;
  const archivedSpaces = spaces.filter(space => space.isArchived);
  const [spacesPanelFrame, setSpacesPanelFrame] = useState<{ left: number; top: number; height: number; width: number } | null>(null);
  const handleSpaceSelect = async (spaceId: string | null) => { setSelectedSpace(spaceId); await loadConversations({ spaceId }); };
  const {
    isSpaceEditorOpen,
    setIsSpaceEditorOpen,
    isCreateSpaceOpen,
    setIsCreateSpaceOpen,
    newSpaceKindDraft,
    setNewSpaceKindDraft,
    newSpaceNameDraft,
    setNewSpaceNameDraft,
    isCreatingSpace,
    isSavingSpace,
    isArchivingSpace,
    isRestoringSpace,
    availableSpaceModels,
    createSpace,
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
    spaceModelDraft,
    setSpaceModelDraft,
    spacePromptDraft,
    setSpacePromptDraft,
    spaceKbDefault,
    setSpaceKbDefault,
    spaceWebDefault,
    setSpaceWebDefault,
    spaceDeepResearchDefault,
  } = editor;
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

  return createPortal(
    <>
      <motion.button
        type="button"
        aria-label="Close spaces panel"
        onClick={() => onClose()}
        className={`fixed inset-0 bg-[hsl(var(--overlay))] ${SPACES_MODAL_LAYER_CLASSES.backdrop}`}
        initial={prefersReducedMotion ? false : { opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.15, ease: [0.22, 1, 0.36, 1] }}
      />
      <motion.aside
        className={`fixed border-r border-subtle bg-surface-raised shadow-md ${SPACES_MODAL_LAYER_CLASSES.panel}`}
        style={spacesPanelFrame ?? undefined}
        initial={prefersReducedMotion ? false : { opacity: 0, scale: 0.98 }}
        animate={{ opacity: 1, scale: 1 }}
        transition={{ duration: 0.15, ease: [0.22, 1, 0.36, 1] }}
      >
        <div className={`${SPACES_MODAL_LAYER_CLASSES.content} flex h-full flex-col overflow-hidden`}>
          <div className={`flex items-center justify-between border-b border-subtle px-4 py-3 ${SPACES_MODAL_LAYER_CLASSES.section}`}>
            <h3 className="font-serif text-sm font-semibold text-[hsl(var(--text-primary))]">Spaces</h3>
            <button
              onClick={() => onClose()}
              className="rounded-sm p-1.5 text-[hsl(var(--text-muted))] transition-colors duration-fast hover:bg-surface hover:text-[hsl(var(--text-primary))]"
              aria-label="Close spaces panel"
              title="Close"
            >
              <X className="h-4 w-4" />
            </button>
          </div>

          <div className={`flex min-h-0 flex-1 flex-col overflow-y-auto px-4 py-3 ${SPACES_MODAL_LAYER_CLASSES.section}`}>
            <div className="flex items-center gap-2">
              <button
                onClick={() => setIsCreateSpaceOpen((open) => !open)}
                className="inline-flex h-7 items-center gap-1.5 rounded-sm px-2 text-xs text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:bg-surface hover:text-[hsl(var(--text-primary))]"
              >
                <FolderPlus className="w-3.5 h-3.5" />
                {isCreateSpaceOpen ? 'Cancel' : 'New space'}
              </button>
              <button
                onClick={() => setIsSpaceEditorOpen((open) => !open)}
                disabled={!selectedSpace}
                className="inline-flex h-7 items-center gap-1.5 rounded-sm px-2 text-xs text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:bg-surface hover:text-[hsl(var(--text-primary))] disabled:cursor-not-allowed disabled:opacity-40"
              >
                <Settings2 className="w-3.5 h-3.5" />
                Edit space
              </button>
            </div>

            {isCreateSpaceOpen && (
              <div className="mt-3 space-y-2 border-t border-subtle pt-3">
                <input
                  value={newSpaceNameDraft}
                  onChange={(e) => setNewSpaceNameDraft(e.target.value)}
                  placeholder={
                    newSpaceKindDraft === 'journal'
                      ? 'Journal name (e.g. Food Research, Weekly Notes)'
                      : 'Space name (e.g. Product, Research, Personal)'
                  }
                  className="w-full rounded-sm border border-border-default bg-surface-raised px-2 py-1.5 text-xs text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring))]"
                />
                <div className="flex items-center gap-1.5">
                  <button
                    type="button"
                    onClick={() => setNewSpaceKindDraft('standard')}
                    className={`rounded-sm border px-2 py-1 text-xs transition-colors duration-fast ${newSpaceKindDraft === 'standard'
                      ? 'border-[hsl(var(--accent))] text-[hsl(var(--accent))]'
                      : 'border-border-default text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))]'
                      }`}
                  >
                    Standard
                  </button>
                  <button
                    type="button"
                    onClick={() => setNewSpaceKindDraft('journal')}
                    className={`rounded-sm border px-2 py-1 text-xs transition-colors duration-fast ${newSpaceKindDraft === 'journal'
                      ? 'border-[hsl(var(--accent))] text-[hsl(var(--accent))]'
                      : 'border-border-default text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))]'
                      }`}
                  >
                    Journal
                  </button>
                </div>
                <div className="flex items-center justify-end gap-1.5">
                  <button
                    type="button"
                    onClick={() => {
                      setIsCreateSpaceOpen(false);
                      setNewSpaceNameDraft('');
                      setNewSpaceKindDraft('standard');
                    }}
                    className="inline-flex items-center gap-1 rounded-sm border border-border-default px-2 py-1 text-xs text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:text-[hsl(var(--text-primary))]"
                  >
                    Cancel
                  </button>
                  <button
                    onClick={handleAsyncEvent(createSpace)}
                    disabled={isCreatingSpace}
                    className="inline-flex items-center gap-1 rounded-sm bg-[hsl(var(--accent))] px-2 py-1 text-xs text-[hsl(var(--accent-fg))] transition-colors duration-fast hover:bg-[hsl(var(--accent-hover))] disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    {isCreatingSpace ? (
                      <Loader2 className="w-3.5 h-3.5 animate-spin" />
                    ) : (
                      <Plus className="w-3.5 h-3.5" />
                    )}
                    Create
                  </button>
                </div>
              </div>
            )}

            <div className="mt-3 flex min-h-0 flex-1 flex-col">

              <div className="flex-1 min-h-0 space-y-0.5 overflow-y-auto pr-1">
                <button
                  onClick={handleAsyncEvent(() => handleSpaceSelect(null))}
                  className={`relative w-full rounded-sm px-3 py-2 text-left transition-colors duration-fast ${selectedSpaceId === null ? 'bg-surface' : 'hover:bg-surface'
                    }`}
                >
                  {selectedSpaceId === null && (
                    <span
                      className="absolute inset-y-0 left-0 w-0.5 bg-[hsl(var(--accent))]"
                      aria-hidden="true"
                    />
                  )}
                  <div className="flex items-start gap-2.5">
                    <div className="flex h-6 w-6 shrink-0 items-center justify-center text-[hsl(var(--text-tertiary))]">
                      <MessageSquare className="h-4 w-4" />
                    </div>
                    <div className="min-w-0">
                      <p className="truncate text-sm text-[hsl(var(--text-primary))]">All spaces</p>
                      <p className="mt-0.5 text-xs text-[hsl(var(--text-muted))]">
                        Every conversation, all scopes
                      </p>
                    </div>
                  </div>
                </button>

                {standardSpaces.length > 0 && (
                  <p className="px-2 pt-4 pb-1 text-xs font-medium text-[hsl(var(--text-secondary))]">
                    Spaces
                  </p>
                )}

                {standardSpaces.map((space) => {
                  const isSelected = selectedSpaceId === space.id;
                  const accent = normalizeHexColor(space.accentColor);

                  return (
                    <button
                      key={space.id}
                      onClick={handleAsyncEvent(() => handleSpaceSelect(space.id))}
                      className={`relative w-full rounded-sm px-3 py-2 text-left transition-colors duration-fast ${isSelected ? 'bg-surface' : 'hover:bg-surface'
                        }`}
                      title={space.description ?? space.name}
                    >
                      {isSelected && (
                        <span
                          className="absolute inset-y-0 left-0 w-0.5"
                          style={{ backgroundColor: accent ?? undefined }}
                          aria-hidden="true"
                        />
                      )}
                      {isSelected && !accent && (
                        <span
                          className="absolute inset-y-0 left-0 w-0.5 bg-[hsl(var(--accent))]"
                          aria-hidden="true"
                        />
                      )}
                      <div className="flex items-start gap-2.5">
                        <div className="flex h-6 w-6 shrink-0 items-center justify-center text-sm text-[hsl(var(--text-tertiary))]">
                          {space.icon || (
                            accent ? (
                              <span
                                className="h-2 w-2 rounded-full"
                                style={{ backgroundColor: accent }}
                                aria-hidden="true"
                              />
                            ) : (
                              <MessageSquare className="h-4 w-4" />
                            )
                          )}
                        </div>
                        <div className="min-w-0">
                          <p className="truncate text-sm text-[hsl(var(--text-primary))]">{space.name}</p>
                          {space.description ? (
                            <p className="mt-0.5 line-clamp-2 text-xs text-[hsl(var(--text-muted))]">
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
              <div className="mt-3 space-y-3 border-t border-subtle pt-3">
                <p className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                  {selectedSpace.name}
                </p>

                <details open className="border-b border-subtle pb-3">
                  <summary className="cursor-pointer text-xs text-[hsl(var(--text-secondary))]">Basics</summary>
                  <div className="mt-2 space-y-2">
                    <div className="grid grid-cols-[72px_1fr] gap-2">
                      <input
                        value={spaceIconDraft}
                        onChange={(e) => setSpaceIconDraft(e.target.value)}
                        placeholder="Icon"
                        className="w-full rounded-sm border border-border-default bg-surface px-2 py-1.5 text-xs text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring))]"
                      />
                      <input
                        value={spaceNameDraft}
                        onChange={(e) => setSpaceNameDraft(e.target.value)}
                        placeholder="Space name"
                        className="w-full rounded-sm border border-border-default bg-surface px-2 py-1.5 text-xs text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring))]"
                      />
                    </div>
                    <div className="grid grid-cols-[92px_1fr_56px] gap-2">
                      <input
                        type="color"
                        value={normalizeHexColor(spaceAccentDraft) ?? '#aa503d'}
                        onChange={(e) => setSpaceAccentDraft(e.target.value)}
                        className="h-8 w-full rounded-sm border border-border-default bg-surface p-1"
                        title="Accent color"
                      />
                      <input
                        value={spaceAccentDraft}
                        onChange={(e) => setSpaceAccentDraft(e.target.value)}
                        placeholder="#aa503d"
                        className="w-full rounded-sm border border-border-default bg-surface px-2 py-1.5 text-xs text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring))]"
                      />
                      <button
                        type="button"
                        onClick={() => setSpaceAccentDraft('')}
                        aria-label="Clear accent color"
                        title="Clear accent color"
                        className="rounded-sm border border-border-default px-2 py-1 text-xs text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:text-[hsl(var(--text-primary))]"
                      >
                        Clear
                      </button>
                    </div>
                    <input
                      value={spaceDescriptionDraft}
                      onChange={(e) => setSpaceDescriptionDraft(e.target.value)}
                      placeholder="Description"
                      className="w-full rounded-sm border border-border-default bg-surface px-2 py-1.5 text-xs text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring))]"
                    />
                  </div>
                </details>

                <details className="border-b border-subtle pb-3">
                  <summary className="cursor-pointer text-xs text-[hsl(var(--text-secondary))]">Defaults</summary>
                  <div className="mt-2 space-y-2">
                    <input
                      value={spaceModelDraft}
                      onChange={(e) => setSpaceModelDraft(e.target.value)}
                      placeholder="Default model id (optional)"
                      list="space-model-options"
                      className="w-full rounded-sm border border-border-default bg-surface px-2 py-1.5 text-xs text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring))]"
                    />
                    <datalist id="space-model-options">
                      {availableSpaceModels.map((modelId) => (
                        <option key={modelId} value={modelId} />
                      ))}
                    </datalist>
                    <textarea
                      value={spacePromptDraft}
                      onChange={(e) => setSpacePromptDraft(e.target.value)}
                      placeholder="System prompt for this space"
                      rows={4}
                      className="w-full rounded-sm border border-border-default bg-surface px-2 py-1.5 text-xs text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring))] resize-y"
                    />
                    <div className="flex items-center gap-2">
                      <button
                        type="button"
                        onClick={() => setSpaceKbDefault((value) => !value)}
                        className={`px-2 py-1 text-xs rounded-sm border transition-colors duration-fast ${spaceKbDefault
                          ? 'border-[hsl(var(--accent))] text-[hsl(var(--accent))]'
                          : 'border-border-default text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))]'
                          }`}
                      >
                        Search documents by default
                      </button>
                      <button
                        type="button"
                        onClick={() => setSpaceWebDefault((value) => !value)}
                        className={`px-2 py-1 text-xs rounded-sm border transition-colors duration-fast ${spaceWebDefault
                          ? 'border-[hsl(var(--accent))] text-[hsl(var(--accent))]'
                          : 'border-border-default text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))]'
                          }`}
                      >
                        Web by default
                      </button>
                      <button
                        type="button"
                        onClick={toggleSpaceDeepResearchDefault}
                        className={`px-2 py-1 text-xs rounded-sm border transition-colors duration-fast ${spaceDeepResearchDefault
                          ? 'border-[hsl(var(--accent))] text-[hsl(var(--accent))]'
                          : 'border-border-default text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))]'
                          }`}
                      >
                        Deep research by default
                      </button>
                    </div>
                    {spaceDeepResearchDefault && (
                      <div
                        role="status"
                        aria-live="polite"
                        className="rounded-sm border border-[hsl(var(--warning-muted))] bg-[hsl(var(--warning-muted))] px-2 py-1.5 text-xs text-[hsl(var(--warning-fg))]"
                      >
                        Deep research is on for this space. Responses will be slower.
                      </div>
                    )}
                  </div>
                </details>

                <div className="flex items-center gap-1.5">
                  <button
                    onClick={handleAsyncEvent(saveSpaceEnvironment)}
                    disabled={isSavingSpace}
                    className="inline-flex items-center gap-1 rounded-sm bg-[hsl(var(--accent))] px-2.5 py-1 text-xs text-[hsl(var(--accent-fg))] transition-colors duration-fast hover:bg-[hsl(var(--accent-hover))] disabled:opacity-50 disabled:cursor-not-allowed"
                  >
                    {isSavingSpace ? (
                      <Loader2 className="w-3.5 h-3.5 animate-spin" />
                    ) : (
                      <Save className="w-3.5 h-3.5" />
                    )}
                    Save
                  </button>
                  {selectedSpace.id !== 'space_general' && (
                    <button
                      onClick={handleAsyncEvent(() => setSelectedSpaceArchived(!selectedSpace.isArchived))}
                      disabled={isArchivingSpace}
                      className="inline-flex items-center gap-1 rounded-sm px-2.5 py-1 text-xs text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:bg-surface hover:text-[hsl(var(--text-primary))] disabled:opacity-50 disabled:cursor-not-allowed"
                    >
                      {isArchivingSpace ? (
                        <Loader2 className="w-3.5 h-3.5 animate-spin" />
                      ) : (
                        <Archive className="w-3.5 h-3.5" />
                      )}
                      {selectedSpace.isArchived ? 'Restore' : 'Archive'}
                    </button>
                  )}
                </div>

                {archivedSpaces.length > 0 && (
                  <details className="border-b border-subtle pb-3">
                    <summary className="cursor-pointer text-xs text-[hsl(var(--text-muted))]">
                      Archived spaces · {archivedSpaces.length}
                    </summary>
                    <div className="mt-2 space-y-1">
                      {archivedSpaces.slice(0, 6).map((space) => (
                        <div
                          key={space.id}
                          className="flex items-center justify-between rounded-sm border border-subtle bg-surface px-2 py-1"
                        >
                          <span className="text-xs text-[hsl(var(--text-secondary))] truncate">
                            {space.icon ? `${space.icon} ` : ''}{space.name}
                          </span>
                          <button
                            onClick={handleAsyncEvent(() => restoreArchivedSpace(space.id))}
                            disabled={isRestoringSpace}
                            className="rounded-sm border border-border-default px-1.5 py-0.5 text-xs text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))] disabled:opacity-50 disabled:cursor-not-allowed transition-colors duration-fast"
                          >
                            Restore
                          </button>
                        </div>
                      ))}
                    </div>
                  </details>
                )}
              </div>
            )}
          </div>

          <div className={`border-t border-subtle px-4 py-3 ${SPACES_MODAL_LAYER_CLASSES.section}`}>
            <p className="text-xs text-[hsl(var(--text-muted))] text-center">
              {activeSpacesOrdered.length} space{activeSpacesOrdered.length !== 1 ? 's' : ''}
            </p>
          </div>
        </div>
      </motion.aside>
    </>,
    document.body
  );
}
