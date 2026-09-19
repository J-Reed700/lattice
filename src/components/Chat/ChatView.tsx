import { useCallback, useEffect, useRef, useState } from 'react';

import { MessageSquarePlus, PanelLeft } from 'lucide-react';
import { useSearchParams } from 'react-router';

import { ChatPanel } from './ChatPanel';
import { ConversationSidebar } from './ConversationSidebar';
import { ConversationSpotlight } from './ConversationSpotlight';
import { ChatReaderPane } from './reader/ChatReaderPane';
import { useDownloadedModels } from '../../hooks/useDownloadedModels';
import { VaultAPI } from '../../lib/api';
import { useConversationsStore } from '../../stores/conversationsStore';
import { toast } from '../../stores/toastStore';
import { createDefaultConversationTitle } from '../../utils/conversationTitles';
import { NEW_ITEM_EVENT } from '../RootLayout';
import { IconButton } from '../ui';

/** Longest quote we will carry through a URL into the composer. */
const MAX_QUOTE_CHARS = 2000;

const GENERAL_SPACE_ID = 'space_general';

const decodeQuote = (raw: string): string => {
  try {
    return decodeURIComponent(raw).slice(0, MAX_QUOTE_CHARS);
  } catch {
    return raw.slice(0, MAX_QUOTE_CHARS);
  }
};

const SIDEBAR_COLLAPSED_KEY = 'chat.sidebar.collapsed';

function readSidebarCollapsed(): boolean {
  try {
    return localStorage.getItem(SIDEBAR_COLLAPSED_KEY) === '1';
  } catch {
    return false;
  }
}

function writeSidebarCollapsed(value: boolean): void {
  try {
    localStorage.setItem(SIDEBAR_COLLAPSED_KEY, value ? '1' : '0');
  } catch {
    // Preference only.
  }
}

export function ChatView() {
  const {
    loadConversations,
    loadSpaces,
    selectConversation,
    createConversation,
    activeConversationId,
    conversations,
    setComposerDraft,
    loadConversationLinkedDocuments,
  } = useConversationsStore();
  const { fetchDownloadedModels } = useDownloadedModels();
  const [isSpotlightOpen, setIsSpotlightOpen] = useState(false);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(readSidebarCollapsed);
  const [searchParams, setSearchParams] = useSearchParams();
  const selectingConversationRef = useRef<string | null>(null);
  const creatingRef = useRef(false);
  const rowRef = useRef<HTMLDivElement>(null);
  /**
   * How much room the chat and the docked reader have between them.
   *
   * Measured from the row rather than the window because that is what the two
   * of them actually share: collapsing the sidebar hands the reader 200-odd
   * pixels the viewport knows nothing about.
   */
  const [row, setRow] = useState({ width: 0, available: 0 });
  /** A `?documentId=` request waiting for the conversation it belongs to. */
  const [pendingDocumentLink, setPendingDocumentLink] = useState<{
    documentId: string;
    skipConversationId: string | null;
  } | null>(null);

  useEffect(() => {
    const loadData = async () => {
      await Promise.all([loadSpaces(), fetchDownloadedModels()]);
      await loadConversations();
    };
    void loadData();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []); // Load once on mount only

  useEffect(() => {
    writeSidebarCollapsed(sidebarCollapsed);
  }, [sidebarCollapsed]);

  useEffect(() => {
    const element = rowRef.current;
    if (!element || typeof ResizeObserver === 'undefined') return;

    const measure = () => {
      const width = element.clientWidth;
      // The sidebar is the first child; the rest is the chat plus the reader.
      const sidebar = element.firstElementChild?.getBoundingClientRect().width ?? 0;
      const available = Math.max(0, width - sidebar);
      setRow((current) =>
        current.width === width && current.available === available
          ? current
          : { width, available }
      );
    };

    const observer = new ResizeObserver(measure);
    observer.observe(element);
    // Collapsing the sidebar does not resize the row, so the effect re-runs on
    // it and measures again.
    measure();
    return () => observer.disconnect();
  }, [sidebarCollapsed]);

  const handleNewConversation = useCallback(async () => {
    if (creatingRef.current) return;
    creatingRef.current = true;
    try {
      await createConversation(createDefaultConversationTitle());
    } catch {
      // Errors surface through the conversations store.
    } finally {
      creatingRef.current = false;
    }
  }, [createConversation]);

  // ⌘N (RootLayout) and `?new=1` deep links both start a conversation here.
  useEffect(() => {
    const onNew = () => {
      void handleNewConversation();
    };
    window.addEventListener(NEW_ITEM_EVENT, onNew);
    return () => window.removeEventListener(NEW_ITEM_EVENT, onNew);
  }, [handleNewConversation]);

  useEffect(() => {
    if (searchParams.get('new') !== '1') return;
    const next = new URLSearchParams(searchParams);
    next.delete('new');
    setSearchParams(next, { replace: true });
    void handleNewConversation();
  }, [handleNewConversation, searchParams, setSearchParams]);

  // `?documentId=` / `?quote=` — the contract for "Ask about this" from the
  // Library and reading surfaces both open conversations here.
  // Reading the params and acting on them are two steps on purpose. `?new=1`
  // starts a conversation asynchronously, so at the moment the params are read
  // `activeConversationId` is still the *previous* conversation (or null).
  // Linking there would attach the document to the wrong thread, or drop it
  // silently — the params are consumed either way. So the request is parked
  // until a conversation that is not the one we started from is open.
  useEffect(() => {
    const requestedDocumentId = searchParams.get('documentId');
    const requestedQuote = searchParams.get('quote');
    if (!requestedDocumentId && !requestedQuote) return;

    const startedNewConversation = searchParams.get('new') === '1';
    const next = new URLSearchParams(searchParams);
    next.delete('documentId');
    next.delete('quote');
    // `new` too: this effect runs after the `?new=1` one in the same commit and
    // its write lands last, so leaving `new` in would put it back in the URL
    // and start a second conversation on the next render.
    next.delete('new');
    setSearchParams(next, { replace: true });

    if (requestedQuote) {
      const quote = decodeQuote(requestedQuote);
      if (quote.trim()) {
        const quoted = quote
          .split('\n')
          .map((line) => `> ${line}`)
          .join('\n');
        setComposerDraft(`${quoted}\n\n`);
      }
    }

    if (!requestedDocumentId) return;
    setPendingDocumentLink({
      documentId: requestedDocumentId,
      // Non-null only when a fresh conversation is on its way.
      skipConversationId: startedNewConversation ? activeConversationId ?? null : null,
    });
  }, [activeConversationId, searchParams, setComposerDraft, setSearchParams]);

  useEffect(() => {
    if (!pendingDocumentLink || !activeConversationId) return;
    if (
      pendingDocumentLink.skipConversationId !== null &&
      pendingDocumentLink.skipConversationId === activeConversationId
    ) {
      // The new conversation has not landed yet; wait for it.
      return;
    }

    const { documentId } = pendingDocumentLink;
    // Cleared first: this effect re-runs on `conversations`, and the work
    // must happen exactly once.
    setPendingDocumentLink(null);

    void (async () => {
      const conversationId = activeConversationId;
      const conversation = conversations.find((item) => item.id === conversationId);
      const spaceId = conversation?.spaceId ?? GENERAL_SPACE_ID;

      // Space membership is the one scoping lever the backend enforces, so
      // that is what we set. No space is created here: silently narrowing
      // every future answer in a thread is not something a URL should do.
      const linked = await VaultAPI.setDocumentSpaceMembership(documentId, spaceId, true);
      if (!linked.ok) {
        toast.error("Couldn't attach that document", { message: linked.error });
        return;
      }
      await loadConversationLinkedDocuments(conversationId);

      const document = await VaultAPI.getDocument(documentId);
      const fileName = document.ok ? document.data.fileName : null;
      if (!fileName) return;
      // Only claim scoping when retrieval really is scoped.
      toast.success(
        spaceId === GENERAL_SPACE_ID ? `Asking about ${fileName}.` : `Scoped to ${fileName}.`
      );
    })();
  }, [
    activeConversationId,
    conversations,
    loadConversationLinkedDocuments,
    pendingDocumentLink,
  ]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const mod = event.metaKey || event.ctrlKey;
      // Shift is required: plain Mod+K belongs to the global command palette.
      if (mod && event.shiftKey && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        setIsSpotlightOpen((open) => !open);
        return;
      }
      if (mod && !event.shiftKey && event.key === '\\') {
        event.preventDefault();
        setSidebarCollapsed((value) => !value);
      }
    };

    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  useEffect(() => {
    const requestedConversationId = searchParams.get('conversationId');
    if (!requestedConversationId) return;
    if (activeConversationId === requestedConversationId) return;
    if (selectingConversationRef.current === requestedConversationId) return;

    selectingConversationRef.current = requestedConversationId;
    void selectConversation(requestedConversationId).finally(() => {
      if (selectingConversationRef.current === requestedConversationId) {
        selectingConversationRef.current = null;
      }
    });
  }, [activeConversationId, searchParams, selectConversation]);

  useEffect(() => {
    const requestedConversationId = searchParams.get('conversationId');
    const requestedMessageId = searchParams.get('messageId');
    if (!requestedConversationId || requestedMessageId) return;
    if (activeConversationId !== requestedConversationId) return;

    const nextParams = new URLSearchParams(searchParams);
    nextParams.delete('conversationId');
    setSearchParams(nextParams, { replace: true });
  }, [activeConversationId, searchParams, setSearchParams]);

  useEffect(() => {
    const requestedConversationId = searchParams.get('conversationId');
    const requestedMessageId = searchParams.get('messageId');
    if (!requestedConversationId || !requestedMessageId) return;
    if (activeConversationId !== requestedConversationId) return;

    let attempts = 0;
    const maxAttempts = 16;
    const selector = `message-${requestedMessageId}`;
    let retryTimeoutId: number | undefined;
    let highlightTimeoutId: number | undefined;
    let highlightedTarget: HTMLElement | null = null;

    const clearRequestedLocation = () => {
      const nextParams = new URLSearchParams(searchParams);
      nextParams.delete('conversationId');
      nextParams.delete('messageId');
      setSearchParams(nextParams, { replace: true });
    };

    const tryScroll = () => {
      const target = document.getElementById(selector);
      if (target) {
        target.scrollIntoView({ behavior: 'smooth', block: 'center' });
        target.classList.add('chat-message-highlighted');
        highlightedTarget = target;
        highlightTimeoutId = window.setTimeout(() => {
          target.classList.remove('chat-message-highlighted');
          clearRequestedLocation();
        }, 1500);
        return;
      }

      attempts += 1;
      if (attempts < maxAttempts) {
        retryTimeoutId = window.setTimeout(tryScroll, 120);
      } else {
        clearRequestedLocation();
      }
    };

    retryTimeoutId = window.setTimeout(tryScroll, 80);

    return () => {
      if (retryTimeoutId !== undefined) window.clearTimeout(retryTimeoutId);
      if (highlightTimeoutId !== undefined) window.clearTimeout(highlightTimeoutId);
      highlightedTarget?.classList.remove('chat-message-highlighted');
    };
  }, [activeConversationId, searchParams, setSearchParams]);

  return (
    <div ref={rowRef} className="flex h-full w-full min-w-0 overflow-hidden">
      {sidebarCollapsed ? (
        <aside className="flex h-full w-12 shrink-0 flex-col items-center gap-1 border-r border-border-subtle bg-surface py-2">
          <IconButton label="Show sidebar" shortcut="⌘\" tooltipSide="right" onClick={() => setSidebarCollapsed(false)}>
            <PanelLeft />
          </IconButton>
          <IconButton label="New conversation" shortcut="⌘N" tooltipSide="right" onClick={() => void handleNewConversation()}>
            <MessageSquarePlus />
          </IconButton>
        </aside>
      ) : (
        <ConversationSidebar onCollapse={() => setSidebarCollapsed(true)} />
      )}
      <ChatPanel />
      <ChatReaderPane rowWidth={row.width} availableWidth={row.available} />
      <ConversationSpotlight isOpen={isSpotlightOpen} onClose={() => setIsSpotlightOpen(false)} />
    </div>
  );
}
