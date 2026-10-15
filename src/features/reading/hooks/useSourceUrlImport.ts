import { useEffect, useMemo, useState } from 'react';

import VaultAPI from '@/lib/api';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import { toast } from '@/stores/toastStore';

/**
 * Importing a web source's URL into the library from the reader: the space it
 * lands in, defaulting to the open conversation's, and the import itself.
 */
export function useSourceUrlImport(importableUrl: string | null, activeConversationId: string | null) {
  const conversations = useConversationsStore((state) => state.conversations);
  const spaces = useConversationsStore((state) => state.spaces);
  const selectedSpaceId = useConversationsStore((state) => state.selectedSpaceId);
  const loadConversationLinkedDocuments = useConversationsStore(
    (state) => state.loadConversationLinkedDocuments
  );
  const [isImportingUrl, setIsImportingUrl] = useState(false);
  const activeConversation = useMemo(
    () => conversations.find((conversation) => conversation.id === activeConversationId) ?? null,
    [conversations, activeConversationId]
  );
  const defaultImportSpaceId = activeConversation?.spaceId ?? selectedSpaceId ?? null;
  const [targetImportSpaceId, setTargetImportSpaceId] = useState<string>('');
  const allowUnscopedImport = !activeConversationId;
  const spaceNameById = useMemo(() => {
    const map = new Map<string, string>();
    for (const space of spaces) {
      map.set(space.id, space.name);
    }
    return map;
  }, [spaces]);

  useEffect(() => {
    if (defaultImportSpaceId) {
      setTargetImportSpaceId(defaultImportSpaceId);
      return;
    }
    if (!allowUnscopedImport && spaces.length > 0) {
      setTargetImportSpaceId(spaces[0].id);
      return;
    }
    setTargetImportSpaceId('');
  }, [allowUnscopedImport, defaultImportSpaceId, spaces]);

  const handleImportSourceUrl = async () => {
    if (!importableUrl || isImportingUrl) {
      return;
    }

    const scopeId = targetImportSpaceId.trim() || undefined;
    const conversationId = activeConversationId ?? undefined;
    const scopedSpaceName =
      (scopeId && spaceNameById.get(scopeId)) ||
      (defaultImportSpaceId ? spaceNameById.get(defaultImportSpaceId) : undefined);

    setIsImportingUrl(true);
    try {
      const result = await VaultAPI.ingestWebUrl(importableUrl, {
        spaceId: scopeId,
        conversationId,
      });
      if (!result.ok) {
        toast.error("Couldn't import URL", { message: result.error });
        return;
      }

      const imported = result.data;
      toast.success('Source imported', {
        message: `${imported.title} (${imported.wordCount.toLocaleString()} words)${scopedSpaceName ? ` · ${scopedSpaceName}` : ''}`,
        action: {
          label: 'Open',
          onClick: () => {
            void (async () => {
              const openResult = await VaultAPI.openFileById(imported.documentId);
              if (!openResult.ok) {
                toast.error("Saved, but couldn't open", {
                  message: openResult.error,
                });
              }
            })();
          },
        },
      });
      if (conversationId) {
        void loadConversationLinkedDocuments(conversationId);
      }
    } finally {
      setIsImportingUrl(false);
    }
  };

  return {
    spaces,
    targetImportSpaceId,
    setTargetImportSpaceId,
    allowUnscopedImport,
    isImportingUrl,
    handleImportSourceUrl,
  };
}
