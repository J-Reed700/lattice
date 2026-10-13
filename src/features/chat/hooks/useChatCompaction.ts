import { useCallback, type RefObject } from 'react';

import { useCompactionStore } from '@/features/chat/stores/compactionStore';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import type { ConversationsState } from '@/shared/conversations/conversationsStore.types';
import { toast } from '@/stores/toastStore';

/** Where a /compact stands for the open chat, the record it left, and starting one. */
export function useChatCompaction({
  activeConversationId,
  conversations,
  scrollContainerRef,
  shouldAutoScrollRef,
}: {
  activeConversationId: string | null;
  conversations: ConversationsState['conversations'];
  scrollContainerRef: RefObject<HTMLDivElement | null>;
  shouldAutoScrollRef: RefObject<boolean>;
}) {

  const compactConversation = useConversationsStore((state) => state.compactConversation);
  // A /compact outlives this panel (it carries on across a switch to
  // Explorer and back), so where it stands lives in a store.
  const compactionRun = useCompactionStore((state) =>
    activeConversationId ? state.runs[activeConversationId] : undefined
  );
  const dismissCompaction = useCompactionStore((state) => state.dismiss);
  const isCompacting = compactionRun?.state === 'running';

  // The applied compaction for the active conversation, if any, drives the
  // "Context compacted" divider. A /compact from this session takes
  // precedence; otherwise the persisted record on the conversation, so the
  // divider survives a reload.
  const compactionRecord = activeConversationId
    ? (compactionRun?.state === 'done' ? compactionRun.record : null) ??
      conversations.find((conversation) => conversation.id === activeConversationId)
        ?.compaction ??
      null
    : null;

  const handleCompact = useCallback(
    async (conversationId: string) => {
      const runs = useCompactionStore.getState();
      if (!runs.start(conversationId)) return;
      // The status row lands at the end of the thread: take the reader there,
      // since a minute of summarizing with nothing in view reads as nothing.
      shouldAutoScrollRef.current = true;
      window.requestAnimationFrame(() => {
        const container = scrollContainerRef.current;
        container?.scrollTo({ top: container.scrollHeight, behavior: 'smooth' });
      });
      try {
        const outcome = await compactConversation(conversationId);
        if (outcome.ok) {
          useCompactionStore.getState().finish(conversationId, outcome.record);
          // No "lossless"/"complete" framing: compaction summarizes, and the
          // only thing it actually guarantees is that active requirements
          // survive and the raw messages are still searchable.
          toast.success('Context compacted', {
            message:
              'Older context compacted. Active requirements preserved; original messages remain searchable.',
          });
        } else {
          useCompactionStore.getState().fail(conversationId, outcome.error);
        }
      } catch (error) {
        useCompactionStore
          .getState()
          .fail(conversationId, error instanceof Error ? error.message : String(error));
      }
    },
    [compactConversation, scrollContainerRef, shouldAutoScrollRef]
  );

  return { compactionRun, dismissCompaction, isCompacting, compactionRecord, handleCompact };
}
