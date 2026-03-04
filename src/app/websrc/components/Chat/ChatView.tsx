import { useEffect, useRef, useState } from 'react';

import { useSearchParams } from 'react-router-dom';

import { ChatPanel } from './ChatPanel';
import { ConversationSidebar } from './ConversationSidebar';
import { ConversationSpotlight } from './ConversationSpotlight';
import { useDownloadedModels } from '../../hooks/useDownloadedModels';
import { useConversationsStore } from '../../stores/conversationsStore';

export function ChatView() {
  const { loadConversations, loadSpaces, selectConversation, activeConversationId } = useConversationsStore();
  const { fetchDownloadedModels } = useDownloadedModels();
  const [isSpotlightOpen, setIsSpotlightOpen] = useState(false);
  const [searchParams, setSearchParams] = useSearchParams();
  const selectingConversationRef = useRef<string | null>(null);

  useEffect(() => {
    // Load both conversations and downloaded models on mount
    const loadData = async () => {
      await Promise.all([loadSpaces(), fetchDownloadedModels()]);
      await loadConversations();
    };

    loadData();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []); // Load once on mount only

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const isCmdK = (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k';
      if (!isCmdK) return;

      event.preventDefault();
      setIsSpotlightOpen((open) => !open);
    };

    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  useEffect(() => {
    const requestedConversationId = searchParams.get('conversationId');
    if (!requestedConversationId) {
      return;
    }
    if (activeConversationId === requestedConversationId) {
      return;
    }
    if (selectingConversationRef.current === requestedConversationId) {
      return;
    }

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
    if (!requestedConversationId || requestedMessageId) {
      return;
    }
    if (activeConversationId !== requestedConversationId) {
      return;
    }

    const nextParams = new URLSearchParams(searchParams);
    nextParams.delete('conversationId');
    setSearchParams(nextParams, { replace: true });
  }, [activeConversationId, searchParams, setSearchParams]);

  useEffect(() => {
    const requestedConversationId = searchParams.get('conversationId');
    const requestedMessageId = searchParams.get('messageId');
    if (!requestedConversationId || !requestedMessageId) {
      return;
    }
    if (activeConversationId !== requestedConversationId) {
      return;
    }

    let attempts = 0;
    const maxAttempts = 16;
    const selector = `message-${requestedMessageId}`;
    const tryScroll = () => {
      const target = document.getElementById(selector);
      if (target) {
        target.scrollIntoView({ behavior: 'smooth', block: 'center' });
        target.classList.add('ring-2', 'ring-cyan-300/70');
        window.setTimeout(() => {
          target.classList.remove('ring-2', 'ring-cyan-300/70');
        }, 1300);
        return;
      }

      attempts += 1;
      if (attempts < maxAttempts) {
        window.setTimeout(tryScroll, 120);
      }
    };

    window.setTimeout(tryScroll, 80);

    const nextParams = new URLSearchParams(searchParams);
    nextParams.delete('conversationId');
    nextParams.delete('messageId');
    setSearchParams(nextParams, { replace: true });
  }, [activeConversationId, searchParams, setSearchParams]);

  return (
    <div className="flex h-full w-full min-w-0 overflow-hidden">
      <ConversationSidebar />
      <ChatPanel />
      <ConversationSpotlight
        isOpen={isSpotlightOpen}
        onClose={() => setIsSpotlightOpen(false)}
      />
    </div>
  );
}
