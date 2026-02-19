import { useCallback, useEffect, useMemo, useState } from 'react';

import { Loader2, MessageSquare, Search, Sparkles, Bookmark } from 'lucide-react';

import { useDebounce } from '../../hooks/useDebounce';
import { VaultAPI } from '../../lib/api';
import { useConversationsStore } from '../../stores/conversationsStore';

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

const hexToRgb = (hexColor: string): { r: number; g: number; b: number } => {
  const hex = hexColor.replace('#', '');
  const normalized = hex.length === 3
    ? `${hex[0]}${hex[0]}${hex[1]}${hex[1]}${hex[2]}${hex[2]}`
    : hex;
  const num = Number.parseInt(normalized, 16);
  return {
    r: (num >> 16) & 255,
    g: (num >> 8) & 255,
    b: num & 255,
  };
};

const withAlpha = (hexColor: string, alpha: number): string => {
  const { r, g, b } = hexToRgb(hexColor);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
};

const scrollToMessage = (messageId: string) => {
  let attempts = 0;
  const maxAttempts = 12;

  const tick = () => {
    const element = document.getElementById(`message-${messageId}`);
    if (element) {
      element.scrollIntoView({ behavior: 'smooth', block: 'center' });
      element.classList.add('ring-2', 'ring-blue-400/70');
      window.setTimeout(() => {
        element.classList.remove('ring-2', 'ring-blue-400/70');
      }, 1200);
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
      if (event.key === 'Escape') {
        event.preventDefault();
        onClose();
        return;
      }

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
  }, [activateResult, combinedResults, isOpen, onClose, selectedIndex]);

  if (!isOpen) return null;

  return (
    <div
      className="fixed inset-0 z-50 bg-black/60 backdrop-blur-sm p-4 sm:p-8"
      role="dialog"
      aria-modal="true"
      aria-label="Conversation Spotlight"
      onClick={onClose}
    >
      <div
        className="mx-auto mt-8 max-w-3xl rounded-2xl border border-white/15 bg-[#0f1723] shadow-2xl overflow-hidden"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="border-b border-white/10 px-4 py-3 flex items-center gap-3">
          <Sparkles className="h-4 w-4 text-blue-300" />
          <div className="relative flex-1">
            <Search className="w-4 h-4 text-white/40 absolute left-3 top-1/2 -translate-y-1/2" />
            <input
              autoFocus
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Search conversations and references..."
              className="w-full pl-9 pr-3 py-2.5 rounded-lg border border-white/10 bg-white/[0.03] text-sm text-white/95 placeholder:text-white/40 focus:outline-none focus:ring-1 focus:ring-blue-400/70"
            />
          </div>
          <span className="text-[11px] text-white/45 border border-white/15 rounded px-2 py-1">
            ESC
          </span>
        </div>

        <div className="max-h-[68vh] overflow-y-auto">
          {isLoading ? (
            <div className="h-28 flex items-center justify-center text-white/50">
              <Loader2 className="h-5 w-5 animate-spin" />
            </div>
          ) : combinedResults.length === 0 ? (
            <div className="h-28 flex flex-col items-center justify-center text-white/45">
              <Search className="h-5 w-5 mb-2" />
              <p className="text-sm">No results</p>
            </div>
          ) : (
            <div className="py-1">
              {combinedResults.map((item, idx) => {
                const isSelected = idx === selectedIndex;

                if (item.kind === 'conversation') {
                  const accent = item.conversation.spaceId
                    ? spaceAccentById.get(item.conversation.spaceId) ?? null
                    : null;
                  const style = accent
                    ? {
                      borderLeft: `3px solid ${withAlpha(accent, 0.85)}`,
                      backgroundColor: isSelected ? withAlpha(accent, 0.2) : withAlpha(accent, 0.1),
                    }
                    : undefined;
                  return (
                    <button
                      key={item.key}
                      onClick={() => void activateResult(item)}
                      className={`w-full text-left px-4 py-3 border-b border-white/5 transition-colors ${
                        isSelected ? 'bg-blue-500/15' : 'hover:bg-white/5'
                      }`}
                      style={style}
                    >
                      <div className="flex items-center gap-2 text-sm text-white/90">
                        <MessageSquare className="h-4 w-4 text-blue-300" />
                        <span className="truncate">{item.conversation.title}</span>
                      </div>
                      <p className="mt-1 text-xs text-white/45 truncate">
                        {item.conversation.spaceId
                          ? `${spaceNameById.get(item.conversation.spaceId) ?? item.conversation.spaceId} • `
                          : ''}
                        Conversation • {item.conversation.updatedAt}
                      </p>
                    </button>
                  );
                }

                const accent = spaceAccentById.get(item.bookmark.spaceId) ?? null;
                const style = accent
                  ? {
                    borderLeft: `3px solid ${withAlpha(accent, 0.85)}`,
                    backgroundColor: isSelected ? withAlpha(accent, 0.2) : withAlpha(accent, 0.08),
                  }
                  : undefined;
                return (
                  <button
                    key={item.key}
                    onClick={() => void activateResult(item)}
                    className={`w-full text-left px-4 py-3 border-b border-white/5 transition-colors ${
                      isSelected ? 'bg-amber-500/15' : 'hover:bg-white/5'
                    }`}
                    style={style}
                  >
                    <div className="flex items-center gap-2 text-sm text-white/90">
                      <Bookmark className="h-4 w-4 text-amber-300" />
                      <span className="truncate">
                        {item.bookmark.title || item.bookmark.conversationTitle}
                      </span>
                    </div>
                    <p className="mt-1 text-xs text-white/45 truncate">
                      {(spaceNameById.get(item.bookmark.spaceId) ?? item.bookmark.spaceId)} • {item.bookmark.conversationTitle} • {item.bookmark.messagePreview}
                    </p>
                  </button>
                );
              })}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
