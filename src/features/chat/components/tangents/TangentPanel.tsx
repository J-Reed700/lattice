import { useCallback, useRef, useState, type ComponentType } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { MessageSquarePlus, Trash2 } from 'lucide-react';

import { IconButton } from '@/components/ui/IconButton';
import type { ChatPanelProps } from '@/features/chat/components/ChatPanel';
import { usePromoteTangentMutation } from '@/features/chat/hooks/useConversationTangents';
import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';
import type { ConversationTangentDto } from '@/lib/bindings';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import { toast } from '@/stores/toastStore';

import { TangentConversation } from './TangentConversation';

const renderEmptyState = () => (
  <p className="text-sm leading-relaxed text-text-muted">Ask about this passage. You can return to the main conversation whenever you’re ready.</p>
);

export function TangentPanel({ tangent, chat: Chat, onPromoted }: {
  tangent: ConversationTangentDto;
  /** The shared chat, which the tangent shows in place of a composer of its own. */
  chat: ComponentType<ChatPanelProps>;
  onPromoted: () => void;
}) {
  const id = tangent.conversationId;
  const base = useConversationsStore();
  const queryClient = useQueryClient();
  const { mutateAsync: promoteTangent } = usePromoteTangentMutation();
  const [mutating, setMutating] = useState(false);
  const mutationPending = useRef(false);
  const isSending = base.inFlightGenerations.has(id);
  const refreshTangents = useCallback(() => {
    void queryClient.invalidateQueries({ queryKey: conversationKeys.tangents(tangent.parentConversationId) });
  }, [queryClient, tangent.parentConversationId]);

  const finish = async (action: 'promote' | 'delete') => {
    if (mutationPending.current || isSending) return;
    if (action === 'delete' && !window.confirm('Delete this tangent and its messages?')) return;
    mutationPending.current = true;
    setMutating(true);
    try {
      if (action === 'promote') {
        await promoteTangent(tangent);
        toast.success('Tangent is now a conversation', { action: { label: 'Open conversation', onClick: () => { void base.selectConversation(id); } } });
      } else {
        // The shared delete action also cancels listeners and evicts caches.
        if (!await base.deleteConversation(id)) throw new Error(conversationUiStore.getState().error ?? 'Please try again.');
      }
      queryClient.setQueryData<ConversationTangentDto[]>(conversationKeys.tangents(tangent.parentConversationId), current => current?.filter(item => item.conversationId !== id));
      onPromoted();
    } catch (error) {
      toast.error(action === 'promote' ? "Couldn't make this a conversation" : "Couldn't delete the tangent", { message: error instanceof Error ? error.message : String(error) });
    } finally {
      mutationPending.current = false;
      setMutating(false);
    }
  };

  return <>
    <div className="shrink-0 border-b border-border-subtle p-4">
      <p className="mb-2 text-[11px] font-medium uppercase tracking-wide text-text-muted">Starting point</p>
      <blockquote className="max-h-32 overflow-y-auto whitespace-pre-wrap wrap-break-word border-l-2 border-accent pl-3 font-serif text-sm leading-relaxed text-text-secondary">{tangent.selectedText}</blockquote>
      <div className="mt-3 flex items-center justify-between gap-2">
        <button type="button" onClick={() => void finish('promote')} disabled={isSending || mutating}
          className="inline-flex items-center gap-1.5 rounded px-1 py-1 text-xs text-text-secondary hover:text-accent disabled:opacity-50 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring">
          <MessageSquarePlus className="h-3.5 w-3.5" aria-hidden="true" />Make conversation
        </button>
        <IconButton label="Delete tangent" disabled={isSending || mutating} onClick={() => void finish('delete')}><Trash2 /></IconButton>
      </div>
    </div>
    <TangentConversation conversationId={id} inheritedCount={tangent.contextMessageCount} onTurnEnd={refreshTangents}>
      <Chat embedded autoFocus renderEmptyState={renderEmptyState} />
    </TangentConversation>
  </>;
}
