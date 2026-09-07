import { useCallback, useEffect, useMemo, useState } from 'react';

import * as Dialog from '@radix-ui/react-dialog';
import { motion, useReducedMotion } from 'framer-motion';
import { Bookmark, Loader2, MessageSquare, Search } from 'lucide-react';

import { useDebounce } from '../../hooks/useDebounce';
import { VaultAPI } from '../../lib/api';
import { useConversationsStore } from '../../stores/conversationsStore';
import { formatChatTimestamp } from '../../utils/dateUtils';

import type { ConversationDto, ConversationMessageBookmarkDto } from '../../types';

interface ConversationSpotlightProps {
  isOpen: boolean;
  onClose: () => void;
}

type SpotlightItem =
  | { kind: 'conversation'; key: string; conversation: ConversationDto }
  | { kind: 'bookmark'; key: string; bookmark: ConversationMessageBookmarkDto };

const normalizeHexColor = (value: string | null | undefined): string | null => {
  if (!value) return null;
  const trimmed = value.trim();
  if (!trimmed) return null;
  const candidate = trimmed.startsWith('#') ? trimmed : `#${trimmed}`;
  return /^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/.test(candidate) ? candidate.toLowerCase() : null;
};

const scrollToMessage = (messageId: string) => {
  let attempts = 0;
  const maxAttempts = 12;

  const tick = () => {
    const element = document.getElementById(`message-${messageId}`);
    if (element) {
      element.scrollIntoView({ behavior: 'smooth', block: 'center' });
      element.classList.add('chat-message-highlighted');
      window.setTimeout(() => {
        element.classList.remove('chat-message-highlighted');
      }, 1500);
      return;
    }

    attempts += 1;
    if (attempts < maxAttempts) {
      window.setTimeout(tick, 120);
    }
  };

  window.setTimeout(tick, 80);
};

export function ConversationSpotlight({ isOpen, onClose }: ConversationSpotlightProps) {
  const prefersReducedMotion = useReducedMotion();
  const [query, setQuery] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [conversations, setConversations] = useState<ConversationDto[]>([]);
  const [bookmarks, setBookmarks] = useState<ConversationMessageBookmarkDto[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const debouncedQuery = useDebounce(query, 180);
  const selectConversation = useConversationsStore((state) => state.selectConversation);
  const spaces = useConversationsStore((state) => state.spaces);
  const spaceAccentById = useMemo(() => {
    const map = new Map<string, string | null>();
    for (const space of spaces) {
      map.set(space.id, normalizeHexColor(space.accentColor));
    }
    return map;
  }, [spaces]);
  const spaceNameById = useMemo(() => {
    const map = new Map<string, string>();
    for (const space of spaces) {
      map.set(space.id, space.name);
    }
    return map;
  }, [spaces]);

  const combinedResults = useMemo<SpotlightItem[]>(
    () => [
      ...conversations.map((conversation) => ({
        kind: 'conversation' as const,
        key: `conversation:${conversation.id}`,
        conversation,
      })),
      ...bookmarks.map((bookmark) => ({
        kind: 'bookmark' as const,
        key: `bookmark:${bookmark.id}`,
        bookmark,
      })),
    ],
    [conversations, bookmarks]
  );

  useEffect(() => {
    if (!isOpen) return;
    setSelectedIndex(0);
  }, [isOpen, debouncedQuery]);

  useEffect(() => {
    if (!isOpen) return;

    let cancelled = false;
    const q = debouncedQuery.trim();

    const run = async () => {
      setIsLoading(true);
      const [conversationsResult, bookmarksResult] = await Promise.all([
        VaultAPI.listConversationsExplorer({
          query: q || undefined,
          includeArchived: true,
          limit: 20,
          offset: 0,
        }),
        VaultAPI.listMessageBookmarks({
          query: q || undefined,
          limit: 20,
          offset: 0,
        }),
      ]);

      if (cancelled) return;

      setConversations(
        conversationsResult.ok
          ? (Array.isArray(conversationsResult.data)
            ? []
            : conversationsResult.data.conversations)
          : []
      );

      setBookmarks(
        bookmarksResult.ok
          ? (Array.isArray(bookmarksResult.data)
            ? []
            : bookmarksResult.data.bookmarks)
          : []
      );

      setIsLoading(false);
    };

    void run();

    return () => {
      cancelled = true;
    };
  }, [debouncedQuery, isOpen]);

  const activateResult = useCallback(async (item: SpotlightItem) => {
    if (item.kind === 'conversation') {
      await selectConversation(item.conversation.id);
      onClose();
      return;
    }

    await selectConversation(item.bookmark.conversationId);
    onClose();
    scrollToMessage(item.bookmark.messageId);
  }, [onClose, selectConversation]);

  useEffect(() => {
    if (!isOpen) return;

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'ArrowDown') {
        event.preventDefault();
        setSelectedIndex((idx) => Math.min(idx + 1, Math.max(0, combinedResults.length - 1)));
        return;
      }

      if (event.key === 'ArrowUp') {
        event.preventDefault();
        setSelectedIndex((idx) => Math.max(0, idx - 1));
        return;
      }

      if (event.key === 'Enter' && combinedResults.length > 0) {
        event.preventDefault();
        void activateResult(combinedResults[selectedIndex] ?? combinedResults[0]);
      }
    };

    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [activateResult, combinedResults, isOpen, selectedIndex]);

  return (
    <Dialog.Root open={isOpen} onOpenChange={(open) => { if (!open) onClose(); }}>
      <Dialog.Portal>
        <Dialog.Overlay
          className="fixed inset-0 z-50 bg-[hsl(var(--overlay))] data-[state=open]:animate-in data-[state=open]:duration-slow data-[state=open]:ease-out data-[state=closed]:animate-out data-[state=closed]:duration-base data-[state=closed]:ease-in data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0"
        />
        <Dialog.Content
          className="fixed left-1/2 top-[120px] z-50 w-[calc(100%-32px)] max-w-[640px] -translate-x-1/2 overflow-hidden rounded-lg border border-subtle bg-surface-raised shadow-md outline-none data-[state=open]:animate-in data-[state=open]:duration-slow data-[state=open]:ease-out data-[state=closed]:animate-out data-[state=closed]:duration-base data-[state=closed]:ease-in data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95"
          aria-label="Conversation Spotlight"
        >
          <Dialog.Title className="sr-only">Search conversations and references</Dialog.Title>

          <div className="flex items-center gap-3 border-b border-subtle px-4 py-3">
            <Search className="h-4 w-4 text-[hsl(var(--text-muted))]" aria-hidden="true" />
            <input
              autoFocus
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Search conversations and references"
              className="flex-1 bg-transparent text-sm text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] focus:outline-none"
            />
            <kbd className="rounded-sm border border-border-default px-1.5 py-0.5 font-mono text-xxs text-[hsl(var(--text-muted))]">
              Esc
            </kbd>
          </div>

          <div className="max-h-[60vh] overflow-y-auto">
            {isLoading ? (
              <div className="flex h-28 items-center justify-center text-[hsl(var(--text-muted))]">
                <Loader2 className="h-4 w-4 animate-spin" />
              </div>
            ) : combinedResults.length === 0 ? (
              <div className="flex h-28 flex-col items-center justify-center gap-2 text-[hsl(var(--text-muted))]">
                <Search className="h-4 w-4" />
                <p className="text-sm">No matches.</p>
                <p className="text-xs">Try a different term.</p>
              </div>
            ) : (
              <ul role="listbox">
                {combinedResults.map((item, idx) => {
                  const isSelected = idx === selectedIndex;

                  if (item.kind === 'conversation') {
                    const accent = item.conversation.spaceId
                      ? spaceAccentById.get(item.conversation.spaceId) ?? null
                      : null;
                    return (
                      <li key={item.key} role="option" aria-selected={isSelected}>
                        <button
                          type="button"
                          onClick={() => void activateResult(item)}
                          onMouseEnter={() => setSelectedIndex(idx)}
                          className={`relative w-full px-4 py-3 text-left transition-colors duration-fast ${
                            isSelected ? 'bg-surface' : 'hover:bg-surface'
                          }`}
                        >
                          {isSelected && (
                            prefersReducedMotion ? (
                              <span
                                className="absolute inset-y-0 left-0 w-0.5 bg-[hsl(var(--accent))]"
                                aria-hidden="true"
                              />
                            ) : (
                              <motion.span
                                layoutId="spotlight-selection"
                                className="absolute inset-y-0 left-0 w-0.5 bg-[hsl(var(--accent))]"
                                aria-hidden="true"
                                transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
                              />
                            )
                          )}
                          <div className="flex items-center gap-2 text-sm text-[hsl(var(--text-primary))]">
                            <MessageSquare className="h-4 w-4 text-[hsl(var(--text-tertiary))]" aria-hidden="true" />
                            {accent && (
                              <span
                                className="h-2 w-2 shrink-0 rounded-full"
                                style={{ backgroundColor: accent }}
                                aria-hidden="true"
                              />
                            )}
                            <span className="truncate">{item.conversation.title}</span>
                          </div>
                          <p className="mt-0.5 truncate pl-6 text-xs text-[hsl(var(--text-muted))]">
                            {item.conversation.spaceId
                              ? `${spaceNameById.get(item.conversation.spaceId) ?? item.conversation.spaceId} · `
                              : ''}
                            Conversation · {formatChatTimestamp(item.conversation.updatedAt)}
                          </p>
                        </button>
                      </li>
                    );
                  }

                  const accent = spaceAccentById.get(item.bookmark.spaceId) ?? null;
                  return (
                    <li key={item.key} role="option" aria-selected={isSelected}>
                      <button
                        type="button"
                        onClick={() => void activateResult(item)}
                        onMouseEnter={() => setSelectedIndex(idx)}
                        className={`relative w-full px-4 py-3 text-left transition-colors duration-fast ${
                          isSelected ? 'bg-surface' : 'hover:bg-surface'
                        }`}
                      >
                        {isSelected && (
                          prefersReducedMotion ? (
                            <span
                              className="absolute inset-y-0 left-0 w-0.5 bg-[hsl(var(--accent))]"
                              aria-hidden="true"
                            />
                          ) : (
                            <motion.span
                              layoutId="spotlight-selection"
                              className="absolute inset-y-0 left-0 w-0.5 bg-[hsl(var(--accent))]"
                              aria-hidden="true"
                              transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
                            />
                          )
                        )}
                        <div className="flex items-center gap-2 text-sm text-[hsl(var(--text-primary))]">
                          <Bookmark className="h-4 w-4 text-[hsl(var(--accent))]" aria-hidden="true" />
                          {accent && (
                            <span
                              className="h-2 w-2 shrink-0 rounded-full"
                              style={{ backgroundColor: accent }}
                              aria-hidden="true"
                            />
                          )}
                          <span className="truncate">
                            {item.bookmark.title || item.bookmark.conversationTitle}
                          </span>
                        </div>
                        <p className="mt-0.5 truncate pl-6 text-xs text-[hsl(var(--text-muted))]">
                          {(spaceNameById.get(item.bookmark.spaceId) ?? item.bookmark.spaceId)} · {item.bookmark.conversationTitle} · {item.bookmark.messagePreview}
                        </p>
                      </button>
                    </li>
                  );
                })}
              </ul>
            )}
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
