import { useCallback, useMemo, useState } from 'react';

import * as Dialog from '@radix-ui/react-dialog';
import { Command } from 'cmdk';
import {
  Bookmark,
  Clock,
  FileText,
  Folder,
  Loader2,
  MessageSquare,
  NotebookPen,
  Search,
  Trash2,
} from 'lucide-react';
import { useNavigate } from 'react-router-dom';

import { SpotlightGroup } from './SpotlightGroup';
import { SpotlightItem } from './SpotlightItem';
import {
  filterActions,
  SPOTLIGHT_ACTIONS,
  SPOTLIGHT_NAVIGATE,
  type SpotlightAction,
} from './spotlightActions';
import { useSpotlightResults } from './useSpotlightResults';
import { useSpotlight } from '../../hooks/useSpotlight';
import VaultAPI from '../../lib/api';
import { useConversationsStore } from '../../stores/conversationsStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { formatChatTimestamp } from '../../utils/dateUtils';
import { logger } from '../../utils/logger';
import { scrollToMessage } from '../../utils/scrollToMessage';

/**
 * Spotlight
 *
 * The unified command surface. One palette, one ⌘K, grouped corpus
 * results. Replaces the former CommandPalette + ConversationSpotlight
 * pair, and subsumes the dead SearchView.
 *
 * Structure in reading order:
 *   - Radix Dialog shell matches the former ConversationSpotlight:
 *     top:120px, max-w:640, rounded-lg, surface-raised, shadow-md,
 *     token-based fade+zoom open/close.
 *   - cmdk Command provides primitives. `shouldFilter` is true only
 *     when we have an empty query (so client-side action filtering
 *     works on the shelves) and false otherwise (server-side rankings
 *     own the order for corpus groups).
 *   - Empty query: recents + actions + navigate shelves — teaches the
 *     user what this surface can do without forcing them to type.
 *   - Non-empty query: grouped corpus results. Empty groups are hidden.
 *   - No results: soft message + link to /search?q= as the structured
 *     search escape hatch.
 *   - Footer: ↑↓ Navigate · ↵ Select · Esc Close.
 *
 * This component does not receive props — open state, recents and
 * keyboard bindings live in useSpotlight so any surface can trigger
 * the palette without prop drilling.
 */
export function Spotlight() {
  const navigate = useNavigate();
  const {
    isOpen,
    recentSearches,
    recentDocuments,
    close,
    addRecentSearch,
    addRecentDocument,
    clearRecentSearches,
  } = useSpotlight();

  const [query, setQuery] = useState('');

  const results = useSpotlightResults(query, isOpen);

  const selectConversation = useConversationsStore((s) => s.selectConversation);
  const spaces = useConversationsStore((s) => s.spaces);
  const currentTheme = useSettingsStore((s) => s.settings.display.theme);
  const updateDisplay = useSettingsStore((s) => s.updateDisplay);

  const spaceNameById = useMemo(() => {
    const map = new Map<string, string>();
    for (const space of spaces) {
      map.set(space.id, space.name);
    }
    return map;
  }, [spaces]);

  const filteredActions = useMemo(
    () => filterActions(SPOTLIGHT_ACTIONS, query),
    [query]
  );
  const filteredNavigate = useMemo(
    () => filterActions(SPOTLIGHT_NAVIGATE, query),
    [query]
  );

  const hasQuery = query.trim().length > 0;

  const isEmptyCorpus =
    hasQuery &&
    !results.isLoading &&
    results.documents.length === 0 &&
    results.conversations.length === 0 &&
    results.bookmarks.length === 0 &&
    results.journals.length === 0 &&
    results.folders.length === 0 &&
    filteredActions.length === 0 &&
    filteredNavigate.length === 0;

  const closeAndReset = useCallback(() => {
    close();
    setQuery('');
  }, [close]);

  const handleSelectAction = useCallback(
    (action: SpotlightAction) => {
      switch (action.id) {
        case 'action.new-conversation':
          navigate('/chat');
          break;
        case 'action.new-journal-entry':
          navigate('/journals');
          break;
        case 'action.add-files':
          void (async () => {
            try {
              const selected = await VaultAPI.selectMultipleFiles();
              if (selected && selected.length > 0) {
                for (const file of selected) {
                  const res = await VaultAPI.indexFile(file);
                  if (!res.ok) {
                    logger.error('Spotlight: indexFile failed', {
                      component: 'spotlight',
                      file,
                      error: res.error,
                    });
                  }
                }
              }
            } catch (error) {
              logger.error('Spotlight: add-files failed', {
                component: 'spotlight',
                error: error instanceof Error ? error.message : String(error),
              });
            }
          })();
          break;
        case 'action.add-folder':
          void (async () => {
            try {
              const selected = await VaultAPI.selectFolder();
              if (selected) {
                const res = await VaultAPI.startIndexing(selected, true);
                if (!res.ok) {
                  logger.error('Spotlight: startIndexing failed', {
                    component: 'spotlight',
                    path: selected,
                    error: res.error,
                  });
                }
              }
            } catch (error) {
              logger.error('Spotlight: add-folder failed', {
                component: 'spotlight',
                error: error instanceof Error ? error.message : String(error),
              });
            }
          })();
          break;
        case 'action.add-web-page':
          navigate('/ingest');
          break;
        case 'action.toggle-theme': {
          const nextTheme = currentTheme === 'dark' ? 'light' : 'dark';
          updateDisplay({ theme: nextTheme });
          break;
        }
        default:
          break;
      }
      closeAndReset();
    },
    [closeAndReset, currentTheme, navigate, updateDisplay]
  );

  const handleSelectNavigate = useCallback(
    (action: SpotlightAction) => {
      const targetById: Record<string, string> = {
        'nav.home': '/home',
        'nav.chat': '/chat',
        'nav.journals': '/journals',
        'nav.references': '/references',
        'nav.files': '/files',
        'nav.ingest': '/ingest',
        'nav.search': '/search',
        'nav.settings': '/settings',
      };
      const target = targetById[action.id];
      if (target) navigate(target);
      closeAndReset();
    },
    [closeAndReset, navigate]
  );

  const handleOpenDocument = useCallback(
    async (id: string, name: string, path: string | null | undefined) => {
      addRecentDocument(id, name);
      addRecentSearch(query);
      try {
        if (path) {
          const res = await VaultAPI.openFile(path);
          if (!res.ok) {
            logger.error('Spotlight: openFile failed', {
              component: 'spotlight',
              path,
              error: res.error,
            });
          }
        } else {
          const res = await VaultAPI.openFileById(id);
          if (!res.ok) {
            logger.error('Spotlight: openFileById failed', {
              component: 'spotlight',
              id,
              error: res.error,
            });
          }
        }
      } catch (error) {
        logger.error('Spotlight: open document failed', {
          component: 'spotlight',
          id,
          error: error instanceof Error ? error.message : String(error),
        });
      }
      closeAndReset();
    },
    [addRecentDocument, addRecentSearch, closeAndReset, query]
  );

  const handleRecentDocument = useCallback(
    async (docId: string, docName: string) => {
      addRecentDocument(docId, docName);
      try {
        const docResult = await VaultAPI.getDocument(docId);
        if (docResult.ok && docResult.data.filePath) {
          await VaultAPI.openFile(docResult.data.filePath);
        } else {
          await VaultAPI.openFileById(docId);
        }
      } catch (error) {
        logger.error('Spotlight: recent document open failed', {
          component: 'spotlight',
          docId,
          error: error instanceof Error ? error.message : String(error),
        });
      }
      closeAndReset();
    },
    [addRecentDocument, closeAndReset]
  );

  const handleRecentSearch = useCallback(
    (label: string) => {
      setQuery(label);
    },
    []
  );

  const handleSelectConversation = useCallback(
    async (id: string) => {
      await selectConversation(id);
      navigate('/chat');
      closeAndReset();
    },
    [closeAndReset, navigate, selectConversation]
  );

  const handleSelectBookmark = useCallback(
    async (conversationId: string, messageId: string) => {
      await selectConversation(conversationId);
      navigate('/chat');
      closeAndReset();
      scrollToMessage(messageId);
    },
    [closeAndReset, navigate, selectConversation]
  );

  const handleSelectJournal = useCallback(() => {
    navigate('/journals');
    closeAndReset();
  }, [closeAndReset, navigate]);

  const handleSelectFolder = useCallback(
    (path: string) => {
      navigate(`/files?folder=${encodeURIComponent(path)}`);
      closeAndReset();
    },
    [closeAndReset, navigate]
  );

  const handleDeepSearch = useCallback(() => {
    navigate(`/search?q=${encodeURIComponent(query)}`);
    closeAndReset();
  }, [closeAndReset, navigate, query]);

  // When the palette closes externally (Esc via useSpotlight, or cmdk's
  // own Dialog plumbing), make sure we also clear the query.
  const handleOpenChange = useCallback(
    (open: boolean) => {
      if (!open) closeAndReset();
    },
    [closeAndReset]
  );

  return (
    <Dialog.Root open={isOpen} onOpenChange={handleOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay
          className="fixed inset-0 z-50 bg-[hsl(var(--overlay))] data-[state=open]:animate-in data-[state=open]:duration-slow data-[state=open]:ease-out data-[state=closed]:animate-out data-[state=closed]:duration-base data-[state=closed]:ease-in data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0"
        />
        <Dialog.Content
          className="fixed left-1/2 top-[120px] z-50 w-[calc(100%-32px)] max-w-[640px] -translate-x-1/2 overflow-hidden rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] shadow-md outline-none data-[state=open]:animate-in data-[state=open]:duration-slow data-[state=open]:ease-out data-[state=closed]:animate-out data-[state=closed]:duration-base data-[state=closed]:ease-in data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95"
          aria-label="Spotlight"
        >
          <Dialog.Title className="sr-only">Search your vault</Dialog.Title>
          <Dialog.Description className="sr-only">
            Documents, conversations, journal entries, references, folders, actions, and navigation
          </Dialog.Description>

          <Command
            label="Spotlight"
            shouldFilter={!hasQuery}
            className="flex max-h-[60vh] flex-col"
          >
            <div className="flex items-center gap-3 border-b border-[hsl(var(--border-subtle))] px-4 py-3">
              <Search
                className="h-4 w-4 text-[hsl(var(--text-muted))]"
                strokeWidth={1.75}
                aria-hidden="true"
              />
              <Command.Input
                value={query}
                onValueChange={setQuery}
                placeholder="Search your vault, jump to a view, or run a command"
                className="flex-1 bg-transparent text-sm text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] focus:outline-none"
                autoFocus
              />
              {results.isLoading && (
                <Loader2
                  className="h-4 w-4 animate-spin text-[hsl(var(--text-muted))]"
                  aria-hidden="true"
                />
              )}
              <kbd className="rounded-sm border border-[hsl(var(--border-default))] px-1.5 py-0.5 font-mono text-xxs text-[hsl(var(--text-muted))]">
                Esc
              </kbd>
            </div>

            <Command.List className="flex-1 overflow-y-auto py-2">
              {isEmptyCorpus && (
                <Command.Empty className="flex flex-col items-center gap-2 px-6 py-10 text-center">
                  <p className="text-sm text-[hsl(var(--text-tertiary))]">
                    Nothing matches &ldquo;{query}&rdquo;
                  </p>
                  <button
                    type="button"
                    onClick={handleDeepSearch}
                    className="text-xs text-[hsl(var(--text-muted))] underline-offset-2 hover:text-[hsl(var(--text-secondary))] hover:underline focus:outline-none focus-visible:text-[hsl(var(--text-secondary))]"
                  >
                    Search the index
                  </button>
                </Command.Empty>
              )}

              {/* Empty query: recents first so returning users see history immediately */}
              <SpotlightGroup
                heading="Recent searches"
                hidden={hasQuery || recentSearches.length === 0}
              >
                {recentSearches.map((item) => (
                  <SpotlightItem
                    key={item.id}
                    value={`recent-search-${item.id}`}
                    icon={Clock}
                    label={item.label}
                    onSelect={() => handleRecentSearch(item.label)}
                  />
                ))}
                <SpotlightItem
                  value="clear-recent-searches"
                  icon={Trash2}
                  label="Clear recent searches"
                  onSelect={clearRecentSearches}
                />
              </SpotlightGroup>

              <SpotlightGroup
                heading="Recent documents"
                hidden={hasQuery || recentDocuments.length === 0}
              >
                {recentDocuments.map((doc) => (
                  <SpotlightItem
                    key={doc.id}
                    value={`recent-doc-${doc.id}`}
                    icon={FileText}
                    label={doc.label}
                    onSelect={() => void handleRecentDocument(doc.id, doc.label)}
                  />
                ))}
              </SpotlightGroup>

              {/* Corpus groups — visible only when a query has been typed */}
              <SpotlightGroup
                heading="Documents"
                hidden={!hasQuery || results.documents.length === 0}
              >
                {results.documents.map((result) => {
                  const filename =
                    (result.metadata && (result.metadata.filename as string | undefined)) ??
                    result.title ??
                    'Untitled';
                  const path =
                    result.path ??
                    (result.metadata && (result.metadata.filename as string | undefined)) ??
                    null;
                  const fileType = result.metadata
                    ? (result.metadata.file_type as string | undefined)
                    : undefined;
                  const secondary = path
                    ? `${fileType ? `${fileType.toUpperCase()} · ` : ''}${path}`
                    : result.content?.slice(0, 120);
                  return (
                    <SpotlightItem
                      key={result.id}
                      value={`document-${result.id}`}
                      icon={FileText}
                      label={filename}
                      secondary={secondary}
                      onSelect={() =>
                        void handleOpenDocument(
                          result.documentId ?? result.id,
                          filename,
                          path
                        )
                      }
                    />
                  );
                })}
              </SpotlightGroup>

              <SpotlightGroup
                heading="Conversations"
                hidden={!hasQuery || results.conversations.length === 0}
              >
                {results.conversations.map((conversation) => {
                  const spaceLabel = conversation.spaceId
                    ? (spaceNameById.get(conversation.spaceId) ?? '')
                    : '';
                  const preview =
                    conversation.lastMessagePreview?.trim() ||
                    `${conversation.messageCount} messages`;
                  const timestamp = formatChatTimestamp(conversation.updatedAt);
                  const secondary = [spaceLabel, preview, timestamp]
                    .filter(Boolean)
                    .join(' · ');
                  return (
                    <SpotlightItem
                      key={conversation.id}
                      value={`conversation-${conversation.id}`}
                      icon={MessageSquare}
                      label={conversation.title}
                      secondary={secondary}
                      onSelect={() =>
                        void handleSelectConversation(conversation.id)
                      }
                    />
                  );
                })}
              </SpotlightGroup>

              <SpotlightGroup
                heading="Journal entries"
                hidden={!hasQuery || results.journals.length === 0}
              >
                {results.journals.map((journal) => (
                  <SpotlightItem
                    key={journal.id}
                    value={`journal-${journal.id}`}
                    icon={NotebookPen}
                    label={journal.name}
                    secondary={journal.description ?? 'Journal'}
                    onSelect={handleSelectJournal}
                  />
                ))}
              </SpotlightGroup>

              <SpotlightGroup
                heading="References"
                hidden={!hasQuery || results.bookmarks.length === 0}
              >
                {results.bookmarks.map((bookmark) => {
                  const spaceLabel = spaceNameById.get(bookmark.spaceId) ?? '';
                  // Every bookmark today originates in chat; when journal
                  // capture lands we'll differentiate via bookmark origin
                  // metadata. Until then the chip is a constant.
                  const origin = 'CHAT';
                  const secondary = [
                    origin,
                    spaceLabel,
                    bookmark.conversationTitle,
                    bookmark.messagePreview,
                  ]
                    .filter(Boolean)
                    .join(' · ');
                  return (
                    <SpotlightItem
                      key={bookmark.id}
                      value={`bookmark-${bookmark.id}`}
                      icon={Bookmark}
                      label={bookmark.title ?? bookmark.conversationTitle}
                      secondary={secondary}
                      onSelect={() =>
                        void handleSelectBookmark(
                          bookmark.conversationId,
                          bookmark.messageId
                        )
                      }
                    />
                  );
                })}
              </SpotlightGroup>

              <SpotlightGroup
                heading="Folders"
                hidden={!hasQuery || results.folders.length === 0}
              >
                {results.folders.map((folder) => (
                  <SpotlightItem
                    key={folder.path}
                    value={`folder-${folder.path}`}
                    icon={Folder}
                    label={folder.path}
                    secondary={`${folder.documentCount} documents`}
                    onSelect={() => handleSelectFolder(folder.path)}
                  />
                ))}
              </SpotlightGroup>

              <SpotlightGroup
                heading="Actions"
                hidden={filteredActions.length === 0}
              >
                {filteredActions.map((action) => (
                  <SpotlightItem
                    key={action.id}
                    value={action.id}
                    icon={action.icon}
                    label={action.label}
                    secondary={action.secondary}
                    shortcut={action.shortcut}
                    onSelect={() => handleSelectAction(action)}
                  />
                ))}
              </SpotlightGroup>

              <SpotlightGroup
                heading="Navigate"
                hidden={filteredNavigate.length === 0}
              >
                {filteredNavigate.map((action) => (
                  <SpotlightItem
                    key={action.id}
                    value={action.id}
                    icon={action.icon}
                    label={action.label}
                    secondary={action.secondary}
                    shortcut={action.shortcut}
                    onSelect={() => handleSelectNavigate(action)}
                  />
                ))}
              </SpotlightGroup>
            </Command.List>

            <div className="flex items-center justify-between border-t border-[hsl(var(--border-subtle))] px-4 py-2 text-[hsl(var(--text-muted))]">
              <div className="flex items-center gap-3 text-xxs">
                <span className="flex items-center gap-1">
                  <kbd className="rounded-sm border border-[hsl(var(--border-default))] px-1 py-0.5 font-mono">
                    ↑↓
                  </kbd>
                  Navigate
                </span>
                <span className="flex items-center gap-1">
                  <kbd className="rounded-sm border border-[hsl(var(--border-default))] px-1 py-0.5 font-mono">
                    ↵
                  </kbd>
                  Select
                </span>
                <span className="flex items-center gap-1">
                  <kbd className="rounded-sm border border-[hsl(var(--border-default))] px-1 py-0.5 font-mono">
                    Esc
                  </kbd>
                  Close
                </span>
              </div>
            </div>
          </Command>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
