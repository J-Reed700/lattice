import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { useQuery, useQueryClient } from '@tanstack/react-query';
import { ArrowUp, MessageSquarePlus, Square, Trash2 } from 'lucide-react';

import { IconButton } from '@/components/ui/IconButton';
import { Message } from '@/features/chat/components/Message';
import { deriveMessageMetadata } from '@/hooks/conversations/messageMetadata';
import { reconcilePersistedFailedMessages } from '@/hooks/conversations/optimisticMessages';
import { conversationKeys } from '@/hooks/queries/conversationKeys';
import { fetchBookmarks, fetchConversationDetail, fetchMessages, unwrap } from '@/hooks/queries/conversationQueryData';
import { VaultAPI } from '@/lib/api';
import type { ConversationTangentDto } from '@/lib/bindings';
import { ConversationSnapshotProvider, useConversationsStore } from '@/stores/conversationsStore';
import { conversationUiStore } from '@/stores/conversationUiStore';
import { toast } from '@/stores/toastStore';
import type { ConversationMessage, ToolPreferences } from '@/types/conversation';

const EMPTY_MESSAGES: ConversationMessage[] = [];

export function TangentPanel({ tangent, toolPreferences, unavailable, drafts, onPromoted }: {
  tangent: ConversationTangentDto;
  toolPreferences: ToolPreferences;
  unavailable: boolean;
  drafts: Map<string, string>;
  onPromoted: () => void;
}) {
  const id = tangent.conversationId;
  const base = useConversationsStore();
  const queryClient = useQueryClient();
  const [input, setInput] = useState(() => drafts.get(id) ?? '');
  const [mutating, setMutating] = useState(false);
  const submitting = useRef(false);
  const mutationPending = useRef(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const transcriptRef = useRef<HTMLDivElement>(null);
  const followBottom = useRef(true);
  const messagesQuery = useQuery({ queryKey: conversationKeys.messages(id), queryFn: () => fetchMessages(id) });
  const detailQuery = useQuery({ queryKey: conversationKeys.detail(id), queryFn: () => fetchConversationDetail(id) });
  const bookmarksQuery = useQuery({ queryKey: conversationKeys.bookmarks(id), queryFn: () => fetchBookmarks(id) });
  const persisted = messagesQuery.data ?? EMPTY_MESSAGES;
  const metadata = useMemo(() => deriveMessageMetadata(persisted), [persisted]);
  const isSending = base.inFlightGenerations.has(id);
  const saveDraft = useCallback((value: string) => { drafts.set(id, value); setInput(value); }, [drafts, id]);

  useEffect(() => { textareaRef.current?.focus(); }, []);
  useEffect(() => {
    if (!messagesQuery.data) return;
    conversationUiStore.setState(state => ({
      optimisticMessages: reconcilePersistedFailedMessages(state.optimisticMessages, id, messagesQuery.data!),
    }));
  }, [id, messagesQuery.data]);

  const messages = useMemo(() => [
    ...persisted.slice(tangent.contextMessageCount),
    ...Array.from(base.optimisticMessages.values()).filter(message => message.conversationId === id),
  ], [base.optimisticMessages, id, persisted, tangent.contextMessageCount]);

  useEffect(() => {
    const element = transcriptRef.current;
    if (element && followBottom.current) element.scrollTop = element.scrollHeight;
  }, [messages]);

  // Only the view of the controller changes. All actions and streaming state
  // still belong to the one shared ConversationsProvider above both panels.
  const scope = useMemo(() => ({
    ...base,
    activeConversationId: id,
    conversations: detailQuery.data ? [{ ...detailQuery.data, messages: persisted }] : [],
    lastMessageSources: metadata.sources,
    messageVerification: metadata.verification,
    messageRetrieval: metadata.retrieval,
    messageTurn: metadata.turn,
    messageBookmarks: bookmarksQuery.data ?? [],
    messageBookmarkMap: new Map((bookmarksQuery.data ?? []).map(bookmark => [bookmark.messageId, bookmark])),
    composerDraft: null,
    setComposerDraft: (value: string | null) => saveDraft(value ?? ''),
    regenerateResponse: async (conversationId: string, preferences?: ToolPreferences) => {
      const outcome = await base.regenerateResponse(conversationId, preferences ?? toolPreferences);
      if (outcome === 'failed') saveDraft([...persisted].reverse().find(message => message.role === 'user')?.content ?? '');
      return outcome;
    },
  }), [base, bookmarksQuery.data, detailQuery.data, id, metadata, persisted, saveDraft, toolPreferences]);

  const send = async () => {
    const question = input.trim();
    if (!question || isSending || submitting.current || unavailable || !detailQuery.data || !messagesQuery.isSuccess) return;
    submitting.current = true;
    saveDraft('');
    followBottom.current = true;
    try {
      const outcome = await base.sendMessage(question, id, toolPreferences);
      const current = (queryClient.getQueryData<ConversationMessage[]>(conversationKeys.messages(id)) ?? []).slice(tangent.contextMessageCount);
      const hasQuestion = current.some(message => message.role === 'user' && message.content === question)
        || Array.from(conversationUiStore.getState().optimisticMessages.values()).some(message => message.conversationId === id && message.role === 'user' && message.content === question);
      if (outcome === 'busy' || (outcome !== 'answered' && !hasQuestion)) {
        drafts.set(id, question);
        setInput(question);
      }
    } catch (error) {
      saveDraft(question);
      toast.error("Couldn't send in this tangent", { message: error instanceof Error ? error.message : String(error) });
    } finally {
      submitting.current = false;
      void queryClient.invalidateQueries({ queryKey: conversationKeys.tangents(tangent.parentConversationId) });
    }
  };

  const finish = async (action: 'promote' | 'delete') => {
    if (mutationPending.current || isSending) return;
    if (action === 'delete' && !window.confirm('Delete this tangent and its messages?')) return;
    mutationPending.current = true;
    setMutating(true);
    try {
      if (action === 'promote') {
        const conversation = unwrap(await VaultAPI.promoteConversationTangent(id));
        queryClient.setQueryData(conversationKeys.detail(id), conversation);
        await queryClient.invalidateQueries({ queryKey: conversationKeys.lists });
        toast.success('Tangent is now a conversation', { action: { label: 'Open conversation', onClick: () => { void base.selectConversation(id); } } });
      } else {
        // The shared delete action also cancels listeners and evicts caches.
        if (!await base.deleteConversation(id)) throw new Error(conversationUiStore.getState().error ?? 'Please try again.');
      }
      queryClient.setQueryData<ConversationTangentDto[]>(conversationKeys.tangents(tangent.parentConversationId), current => current?.filter(item => item.conversationId !== id));
      drafts.delete(id);
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
    <div ref={transcriptRef} className="min-h-0 flex-1 overflow-y-auto" onScroll={event => {
      const element = event.currentTarget;
      followBottom.current = element.scrollHeight - element.scrollTop - element.clientHeight < 80;
    }}>
      {(messagesQuery.isPending || detailQuery.isPending) && <p role="status" className="p-4 text-sm text-text-muted">Opening tangent…</p>}
      {(messagesQuery.isError || detailQuery.isError || (detailQuery.isSuccess && !detailQuery.data)) && <div role="alert" className="p-4 text-sm">
        <p>Couldn’t open this tangent.</p><button type="button" className="mt-2 text-accent" onClick={() => { void messagesQuery.refetch(); void detailQuery.refetch(); }}>Try again</button>
      </div>}
      {messagesQuery.isSuccess && messages.length === 0 && <p className="p-4 text-sm leading-relaxed text-text-muted">Ask about this passage. You can return to the main conversation whenever you’re ready.</p>}
      <ConversationSnapshotProvider value={scope}>
        {messages.map((message, index) => <Message key={'id' in message ? message.id : message.tempId} message={message}
          isLastTurn={index === messages.length - 1}
          previousMessageId={persisted[tangent.contextMessageCount + index - 1]?.id} />)}
      </ConversationSnapshotProvider>
    </div>
    <form className="shrink-0 border-t border-border-subtle p-3" onSubmit={event => { event.preventDefault(); void send(); }}>
      <label htmlFor={`tangent-input-${id}`} className="sr-only">Ask in this tangent</label>
      <textarea ref={textareaRef} id={`tangent-input-${id}`} value={input} rows={3}
        placeholder="Ask about this passage…" className="w-full resize-none rounded-lg border border-border-subtle bg-surface px-3 py-2 text-sm outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
        onChange={event => saveDraft(event.target.value)} onKeyDown={event => {
          if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) { event.preventDefault(); void send(); }
        }} />
      <div className="mt-2 flex items-center justify-between gap-2">
        <span className="text-[11px] text-text-muted">{unavailable ? 'Choose an available chat model to reply.' : 'Saved with this conversation'}</span>
        {isSending ? <IconButton label="Stop tangent response" onClick={() => void base.cancelGeneration(id)}><Square /></IconButton> : (
          <button type="submit" aria-label="Send tangent message" disabled={!input.trim() || unavailable || mutating || !messagesQuery.isSuccess || !detailQuery.data}
            className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-accent text-[hsl(var(--accent-fg))] disabled:opacity-40 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"><ArrowUp className="h-4 w-4" /></button>
        )}
      </div>
    </form>
  </>;
}
