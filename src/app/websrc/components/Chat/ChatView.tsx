import { useEffect, useState } from 'react';

import { ChatPanel } from './ChatPanel';
import { ConversationSidebar } from './ConversationSidebar';
import { ConversationSpotlight } from './ConversationSpotlight';
import { useDownloadedModels } from '../../hooks/useDownloadedModels';
import { useConversationsStore } from '../../stores/conversationsStore';

export function ChatView() {
  const { loadConversations, loadSpaces } = useConversationsStore();
  const { fetchDownloadedModels } = useDownloadedModels();
  const [isSpotlightOpen, setIsSpotlightOpen] = useState(false);

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

  return (
    <div className="flex h-full w-full">
      <ConversationSidebar />
      <ChatPanel />
      <ConversationSpotlight
        isOpen={isSpotlightOpen}
        onClose={() => setIsSpotlightOpen(false)}
      />
    </div>
  );
}
