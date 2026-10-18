import { useEffect, useMemo, useState } from 'react';

import { motion, useReducedMotion } from 'framer-motion';
import { GitBranch, Loader2, MessageSquare, NotebookPen } from 'lucide-react';

import { ConversationActionsMenu, ConversationRenameField } from '@/features/chat/components/sidebar/ConversationActionsMenu';
import {
  formatJournalGroupLabel,
  formatShortRelativeTime,
  getLocalDayKey,
  getTimeBucket,
  SpaceKind,
  TIME_BUCKET_LABELS,
  TIME_BUCKET_ORDER,
  TimeBucketKey,
} from '@/features/chat/components/sidebar/sidebarUtils';
import { useConversationRename } from '@/features/chat/components/sidebar/useConversationRename';
import type { useConversationSynthesis } from '@/features/chat/components/sidebar/useConversationSynthesis';
import { useJournalsQuery } from '@/features/chat/components/sidebar/workspaceQueries';
import type { ConversationExportActions } from '@/features/chat/hooks/useConversationExport';
import { useForkLineage } from '@/features/chat/hooks/useForkLineage';
import { useGenerateConversationFlashcardDeck } from '@/features/learning/recall/flashcards/useFlashcards';
import { normalizeHexColor } from '@/features/spaces/model/spaces';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import { scrollToMessage } from '@/utils/chatMessageNavigation';
import { handleAsyncEvent } from '@/utils/promiseHandlers';

interface ConversationListProps {
  isJournalScope: boolean;
  isSelectionMode: boolean;
  selectedConversationIds: Set<string>;
  toggleConversationSelection: (id: string) => void;
  synthesis: ReturnType<typeof useConversationSynthesis>;
  /** The two ways out of a conversation. Mounted once, by the sidebar. */
  exportActions: ConversationExportActions;
}

export function ConversationList({ isJournalScope, isSelectionMode, selectedConversationIds, toggleConversationSelection, synthesis, exportActions }: ConversationListProps) {
  const prefersReducedMotion = useReducedMotion();
  const {
    spaces,
    conversations: allConversations,
    activeConversationId,
    isLoading,
    selectConversation,
    deleteConversation,
  } = useConversationsStore();
  // Explorer threads live in the Explorer, beside their folder.
  const conversations = useMemo(
    () => allConversations.filter((conversation) => !conversation.explorerRoot),
    [allConversations]
  );
  const { journals } = useJournalsQuery();
  const forkLineage = useForkLineage(conversations);
  const { synthesizingConversationId, continuingConversationId } = synthesis;
  const studyDeck = useGenerateConversationFlashcardDeck();
  const menuServices = { synthesis, exportActions, studyDeck };
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [openActionsId, setOpenActionsId] = useState<string | null>(null);
  const rename = useConversationRename();
  const { renamingId: renamingConversationId, begin: beginRenameConversation, cancel: cancelRenameConversation } = rename;
  useEffect(() => { if (isSelectionMode) cancelRenameConversation(); }, [cancelRenameConversation, isSelectionMode]);
  const spaceNameById = useMemo(() => {
    const map = new Map<string, string>();
    for (const space of spaces) {
      map.set(space.id, space.name);
    }
    return map;
  }, [spaces]);
  const spaceAccentById = useMemo(() => {
    const map = new Map<string, string | null>();
    for (const space of spaces) {
      map.set(space.id, normalizeHexColor(space.accentColor));
    }
    return map;
  }, [spaces]);
  const spaceKindById = useMemo(() => {
    const map = new Map<string, SpaceKind>();
    for (const space of spaces) {
      map.set(space.id, 'standard');
    }
    for (const journal of journals) {
      map.set(journal.id, 'journal');
    }
    return map;
  }, [journals, spaces]);
  const journalConversationGroups = useMemo(() => {
    if (!isJournalScope) {
      const now = new Date();
      const buckets = new Map<TimeBucketKey, typeof conversations>();
      for (const conversation of conversations) {
        const updatedAt = new Date(conversation.updatedAt);
        const baseDate = Number.isNaN(updatedAt.getTime()) ? now : updatedAt;
        const bucket = getTimeBucket(baseDate, now);
        const existing = buckets.get(bucket);
        if (existing) {
          existing.push(conversation);
        } else {
          buckets.set(bucket, [conversation]);
        }
      }

      return TIME_BUCKET_ORDER
        .filter((key) => buckets.has(key))
        .map((key) => {
          const items = [...(buckets.get(key) ?? [])].sort((a, b) => {
            const aTime = new Date(a.updatedAt).getTime();
            const bTime = new Date(b.updatedAt).getTime();
            return (Number.isNaN(bTime) ? 0 : bTime) - (Number.isNaN(aTime) ? 0 : aTime);
          });
          return {
            key,
            label: TIME_BUCKET_LABELS[key],
            items,
          };
        });
    }

    const groups = new Map<string, { label: string; dayValue: Date; items: typeof conversations }>();
    for (const conversation of conversations) {
      const updatedAt = new Date(conversation.updatedAt);
      const baseDate = Number.isNaN(updatedAt.getTime()) ? new Date() : updatedAt;
      const dayValue = new Date(baseDate.getFullYear(), baseDate.getMonth(), baseDate.getDate());
      const key = getLocalDayKey(dayValue);
      const existing = groups.get(key);
      if (!existing) {
        groups.set(key, {
          label: formatJournalGroupLabel(dayValue),
          dayValue,
          items: [conversation],
        });
      } else {
        existing.items.push(conversation);
      }
    }

    return Array.from(groups.entries())
      .sort((a, b) => b[1].dayValue.getTime() - a[1].dayValue.getTime())
      .map(([key, group]) => ({
        key,
        label: group.label,
        items: group.items,
      }));
  }, [conversations, isJournalScope]);

  const handleDelete = async (id: string) => {
    const target = conversations.find((conversation) => conversation.id === id);
    if (!target) {
      return;
    }

    setDeletingId(id);
    try {
      await deleteConversation(id);
    } finally {
      setDeletingId(null);
    }
  };

  /** Open the thread this one was branched from, at the turn it forked at. */
  const openForkParent = async (parentId: string, messageId: string | null) => {
    await selectConversation(parentId);
    if (messageId) scrollToMessage(messageId);
  };

  return (<>
    {isLoading && conversations.length === 0 ? (
      <div className="flex h-32 items-center justify-center text-text-muted">
        <Loader2 className="h-5 w-5 animate-spin" />
      </div>
    ) : conversations.length === 0 ? (
      <p className="px-4 py-6 text-sm text-text-secondary">
        {isJournalScope ? 'No entries yet.' : 'No conversations yet.'}
      </p>
    ) : (
      <nav className="pb-2" role="navigation" aria-label="Conversations">
        {journalConversationGroups.map((group) => (
          <section key={group.key}>
            {group.label && (
              <p className="px-4 pb-1 pt-4 text-xs font-medium text-text-muted">
                {group.label}
              </p>
            )}
            <div>
              {group.items.map((conversation) => {
                const isActive = conversation.id === activeConversationId;
                const isDeleting = deletingId === conversation.id;
                const accent = conversation.spaceId
                  ? spaceAccentById.get(conversation.spaceId) ?? null
                  : null;
                const conversationSpaceKind = conversation.spaceId
                  ? (spaceKindById.get(conversation.spaceId) ?? 'standard')
                  : 'standard';
                const spaceLabel = conversation.spaceId
                  ? `${spaceNameById.get(conversation.spaceId) ?? conversation.spaceId}${conversationSpaceKind === 'journal' ? ' · Journal' : ''
                  }`
                  : '';
                const forkParent = forkLineage.get(conversation.id) ?? null;
                const relativeTime = formatShortRelativeTime(conversation.updatedAt);
                const metaLine = [spaceLabel, relativeTime].filter(Boolean).join(' · ');
                const preview =
                  conversation.lastMessagePreview
                  ?? conversation.messages?.[conversation.messages.length - 1]?.content
                  ?? '';

                return (
                  <div
                    key={conversation.id}
                    onClick={handleAsyncEvent(async () => {
                      if (isSelectionMode) {
                        toggleConversationSelection(conversation.id);
                        return;
                      }
                      if (renamingConversationId === conversation.id) {
                        return;
                      }
                      await selectConversation(conversation.id);
                    })}
                    role="button"
                    tabIndex={0}
                    onKeyDown={handleAsyncEvent(async (e) => {
                      if (e.key === 'Enter' || e.key === ' ') {
                        if (isSelectionMode) {
                          e.preventDefault();
                          toggleConversationSelection(conversation.id);
                          return;
                        }
                        if (renamingConversationId === conversation.id) {
                          return;
                        }
                        e.preventDefault();
                        await selectConversation(conversation.id);
                      }
                    })}
                    aria-label={`Select conversation: ${conversation.title}`}
                    aria-current={isActive ? 'page' : undefined}
                    className={`group relative mx-2 w-[calc(100%-16px)] cursor-pointer rounded-lg px-2.5 py-2 text-left ${isActive ? 'bg-[hsl(var(--text-primary)/0.07)]' : 'row-hover'
                      }`}
                  >
                    {isActive && (
                      prefersReducedMotion ? (
                        <span
                          className="absolute inset-y-2 left-0 w-[2.5px] rounded-full bg-accent"
                          aria-hidden="true"
                        />
                      ) : (
                        <motion.span
                          layoutId="sidebar-active-bar"
                          className="absolute inset-y-2 left-0 w-[2.5px] rounded-full bg-accent"
                          aria-hidden="true"
                          transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
                        />
                      )
                    )}

                    <div className="flex items-center gap-2">
                      {isSelectionMode && (
                        <input
                          type="checkbox"
                          checked={selectedConversationIds.has(conversation.id)}
                          onChange={() => toggleConversationSelection(conversation.id)}
                          onClick={(e) => e.stopPropagation()}
                          className="h-3.5 w-3.5 shrink-0 cursor-pointer appearance-none rounded-sm border border-border-strong bg-transparent transition-colors duration-fast checked:border-accent checked:bg-accent checked:shadow-[inset_0_0_0_2px_hsl(var(--surface))] focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
                          aria-label={`Select conversation: ${conversation.title}`}
                        />
                      )}
                      {accent ? (
                        <span
                          className="h-1.5 w-1.5 shrink-0 rounded-full"
                          style={{ backgroundColor: accent }}
                          aria-label={conversation.spaceId ? `Space: ${spaceNameById.get(conversation.spaceId) ?? conversation.spaceId}` : undefined}
                        />
                      ) : isJournalScope ? (
                        <NotebookPen className="h-3.5 w-3.5 shrink-0 text-text-tertiary" />
                      ) : (
                        <MessageSquare className="h-3.5 w-3.5 shrink-0 text-text-tertiary" />
                      )}

                      {renamingConversationId === conversation.id ? (
                        <ConversationRenameField conversation={conversation} rename={rename} />
                      ) : (
                        <h3
                          className={`min-w-0 flex-1 truncate text-ui text-text-primary ${isActive ? 'font-medium' : 'font-normal'
                            }`}
                          onDoubleClick={(e) => {
                            e.stopPropagation();
                            beginRenameConversation(conversation.id, conversation.title);
                          }}
                          title={conversation.title}
                        >
                          {conversation.title}
                        </h3>
                      )}
                    </div>

                    {metaLine && (
                      <p className="mt-0.5 truncate text-xs text-text-muted">{metaLine}</p>
                    )}
                    {synthesizingConversationId === conversation.id && (
                      <p className="mt-1 flex items-center gap-1.5 text-xs text-accent">
                        <Loader2 className="h-3 w-3 animate-spin motion-reduce:animate-none" aria-hidden="true" />
                        Synthesizing to Journal…
                      </p>
                    )}
                    {/* A branch and its parent are near-identical threads until
                        one of them says which is which. Nothing is drawn when
                        the parent has been deleted — a dangling id is ordinary,
                        and a dead link would be worse than silence. */}
                    {forkParent && (
                      <button
                        type="button"
                        onClick={handleAsyncEvent(async (e) => {
                          e.stopPropagation();
                          await openForkParent(forkParent.id, forkParent.messageId);
                        })}
                        title={`Open "${forkParent.title}"`}
                        className="mt-0.5 flex max-w-full items-center gap-1 rounded-sm text-xs text-text-muted transition-colors duration-fast hover:text-accent focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
                      >
                        <GitBranch className="h-3 w-3 shrink-0" strokeWidth={1.6} aria-hidden="true" />
                        <span className="truncate">Branched from {forkParent.title}</span>
                      </button>
                    )}
                    {continuingConversationId === conversation.id && (
                      <p className="mt-0.5 flex items-center gap-1 text-xs text-text-muted" role="status">
                        <Loader2 className="h-3 w-3 shrink-0 animate-spin" aria-hidden="true" />
                        Summarizing into a new chat…
                      </p>
                    )}
                    {preview && (
                      <p className="truncate text-xs text-text-tertiary">{preview}</p>
                    )}

                    {!isSelectionMode && renamingConversationId !== conversation.id && (
                      <div
                        className={`absolute right-2 top-1.5 rounded-sm bg-surface-raised transition-opacity duration-fast ${openActionsId === conversation.id ? 'opacity-100' : 'pointer-events-none opacity-0 group-hover:pointer-events-auto group-hover:opacity-100 group-focus-within:pointer-events-auto group-focus-within:opacity-100'}`}
                        onClick={(e) => e.stopPropagation()}
                      >
                        <ConversationActionsMenu
                          conversation={conversation}
                          services={menuServices}
                          open={openActionsId === conversation.id}
                          onOpenChange={(open) => setOpenActionsId(open ? conversation.id : null)}
                          onRename={() => beginRenameConversation(conversation.id, conversation.title)}
                          onDelete={() => void handleDelete(conversation.id)}
                          deleting={isDeleting}
                        />
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          </section>
        ))}
      </nav>
    )}

  </>);
}
