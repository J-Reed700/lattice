import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { useQuery, useQueryClient } from '@tanstack/react-query';
import { ArrowLeft, ArrowUp, ExternalLink, GitBranch, Square, Trash2, X } from 'lucide-react';
import { useNavigate } from 'react-router';

import { IconButton } from '@/components/ui/IconButton';
import { fetchBookmarks, fetchConversationDetail, fetchMessages } from '@/features/chat/api/conversationQueryData';
import { Message } from '@/features/chat/components/Message';
import { deriveMessageMetadata } from '@/features/chat/controller/messageMetadata';
import { reconcilePersistedFailedMessages } from '@/features/chat/controller/optimisticMessages';
import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';
import { learningTangentsKey } from '@/features/learning/api/learningQueryKeys';
import { CitationVisibilityToggle } from '@/features/reading/components/CitationVisibilityToggle';
import type { LearningTangentDto } from '@/lib/bindings';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import { ConversationSnapshotProvider, useConversationsStore } from '@/shared/conversations/conversationsStore';
import type { RegenerateOptions } from '@/shared/conversations/conversationsStore.types';
import { toast } from '@/stores/toastStore';
import type { ConversationMessage } from '@/types/conversation';

const EMPTY_MESSAGES: ConversationMessage[] = [];

const STARTER_PROMPTS = [
  'Explain this intuitively',
  'Give me a concrete example',
  'How does this connect to the module goal?',
];

export interface CourseTangentChatProps {
  tangent: LearningTangentDto;
  currentModuleId?: string | null;
  currentLessonId?: string | null;
  onBackToList: () => void;
  onClose: () => void;
  onJumpToLesson?: (moduleId: string, lessonId: string) => void;
}

export function CourseTangentChat({
  tangent,
  currentModuleId,
  currentLessonId,
  onBackToList,
  onClose,
  onJumpToLesson,
}: CourseTangentChatProps) {
  const id = tangent.conversationId;
  const navigate = useNavigate();
  const base = useConversationsStore();
  const queryClient = useQueryClient();
  const [input, setInput] = useState('');
  const [mutating, setMutating] = useState(false);
  const submitting = useRef(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const transcriptRef = useRef<HTMLDivElement>(null);
  const followBottom = useRef(true);

  const messagesQuery = useQuery({
    queryKey: conversationKeys.messages(id),
    queryFn: () => fetchMessages(id),
  });
  const detailQuery = useQuery({
    queryKey: conversationKeys.detail(id),
    queryFn: () => fetchConversationDetail(id),
  });
  const bookmarksQuery = useQuery({
    queryKey: conversationKeys.bookmarks(id),
    queryFn: () => fetchBookmarks(id),
  });

  const persisted = messagesQuery.data ?? EMPTY_MESSAGES;
  const metadata = useMemo(() => deriveMessageMetadata(persisted), [persisted]);
  const isSending = base.inFlightGenerations.has(id);

  useEffect(() => {
    textareaRef.current?.focus();
  }, [id]);

  useEffect(() => {
    void base.selectConversation(id);
  }, [base, id]);

  useEffect(() => {
    if (!messagesQuery.data) return;
    conversationUiStore.setState((state) => ({
      optimisticMessages: reconcilePersistedFailedMessages(
        state.optimisticMessages,
        id,
        messagesQuery.data!,
      ),
    }));
  }, [id, messagesQuery.data]);

  const messages = useMemo(
    () => [
      ...persisted.slice(Math.max(1, tangent.contextMessageCount)),
      ...Array.from(base.optimisticMessages.values()).filter(
        (message) => message.conversationId === id,
      ),
    ],
    [base.optimisticMessages, id, persisted, tangent.contextMessageCount],
  );

  useEffect(() => {
    const element = transcriptRef.current;
    if (element && followBottom.current) {
      element.scrollTop = element.scrollHeight;
    }
  }, [messages]);

  const scope = useMemo(
    () => ({
      ...base,
      activeConversationId: id,
      conversations: detailQuery.data ? [{ ...detailQuery.data, messages: persisted }] : [],
      lastMessageSources: metadata.sources,
      messageVerification: metadata.verification,
      messageRetrieval: metadata.retrieval,
      messageTurn: metadata.turn,
      messageBookmarks: bookmarksQuery.data ?? [],
      messageBookmarkMap: new Map(
        (bookmarksQuery.data ?? []).map((bookmark) => [bookmark.messageId, bookmark]),
      ),
      composerDraft: null,
      setComposerDraft: (value: string | null) => setInput(value ?? ''),
      regenerateResponse: async (conversationId: string, options?: RegenerateOptions) => {
        const outcome = await base.regenerateResponse(conversationId, options);
        if (outcome === 'failed') {
          setInput(
            [...persisted].reverse().find((message) => message.role === 'user')?.content ?? '',
          );
        }
        return outcome;
      },
    }),
    [base, bookmarksQuery.data, detailQuery.data, id, metadata, persisted],
  );

  const send = useCallback(
    async (overrideText?: string) => {
      const question = (overrideText ?? input).trim();
      if (
        !question ||
        isSending ||
        submitting.current ||
        !detailQuery.data ||
        !messagesQuery.isSuccess
      ) {
        return;
      }
      submitting.current = true;
      setInput('');
      followBottom.current = true;
      try {
        const outcome = await base.sendMessage(question, id);
        const current = (
          queryClient.getQueryData<ConversationMessage[]>(conversationKeys.messages(id)) ?? []
        ).slice(Math.max(1, tangent.contextMessageCount));
        const hasQuestion =
          current.some((m) => m.role === 'user' && m.content === question) ||
          Array.from(conversationUiStore.getState().optimisticMessages.values()).some(
            (m) => m.conversationId === id && m.role === 'user' && m.content === question,
          );
        if (outcome === 'busy' || (outcome !== 'answered' && !hasQuestion)) {
          setInput(question);
        }
      } catch (error) {
        setInput(question);
        toast.error("Couldn't send in this tangent", {
          message: error instanceof Error ? error.message : String(error),
        });
      } finally {
        submitting.current = false;
        void queryClient.invalidateQueries({
          queryKey: learningTangentsKey(tangent.programId),
        });
      }
    },
    [base, detailQuery.data, id, input, isSending, messagesQuery.isSuccess, queryClient, tangent.contextMessageCount, tangent.programId],
  );

  const handleOpenInChat = async () => {
    try {
      base.setSelectedSpace(tangent.spaceId);
      await base.selectConversation(tangent.conversationId);
      navigate(`/chat?conversationId=${encodeURIComponent(tangent.conversationId)}`);
    } catch (error) {
      toast.error("Couldn't open in chat", {
        message: error instanceof Error ? error.message : String(error),
      });
    }
  };

  const handleDelete = async () => {
    if (isSending || mutating) return;
    if (!window.confirm('Delete this tangent and its messages?')) return;
    setMutating(true);
    try {
      const deleted = await base.deleteConversation(id);
      if (!deleted) throw new Error(conversationUiStore.getState().error ?? 'Please try again.');
      void queryClient.invalidateQueries({
        queryKey: learningTangentsKey(tangent.programId),
      });
      void queryClient.invalidateQueries({
        queryKey: learningTangentsKey(null),
      });
      void queryClient.invalidateQueries({
        queryKey: learningTangentsKey(),
      });
      toast.success('Tangent deleted');
      onBackToList();
    } catch (error) {
      toast.error("Couldn't delete the tangent", {
        message: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setMutating(false);
    }
  };

  const isDifferentLesson = Boolean(
    onJumpToLesson &&
      (tangent.moduleId !== currentModuleId || tangent.lessonId !== currentLessonId),
  );

  const breadcrumb = `${tangent.moduleTitle} › ${tangent.lessonTitle}${
    tangent.sectionTitle ? ` › ${tangent.sectionTitle}` : ''
  }`;

  return (
    <div className="flex h-full min-h-0 flex-1 flex-col bg-surface">
      <header className="tangent-panel-header flex h-12 shrink-0 items-center gap-2 border-b border-border-subtle px-3">
        <IconButton label="All course tangents" onClick={onBackToList}>
          <ArrowLeft className="h-4 w-4" />
        </IconButton>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-1.5 text-xs font-medium text-text-primary">
            <GitBranch className="h-3.5 w-3.5 text-accent shrink-0" aria-hidden="true" />
            <span className="truncate">
              Tangent · {tangent.sectionTitle ?? tangent.lessonTitle}
            </span>
          </div>
          <p className="truncate text-[11px] text-text-muted">
            {tangent.programTitle} · {tangent.moduleTitle}
          </p>
        </div>
        <CitationVisibilityToggle compact className="h-8 min-h-8 w-8 justify-center px-0" />
        <IconButton label="Open in full Chat" onClick={() => void handleOpenInChat()}>
          <ExternalLink className="h-4 w-4" />
        </IconButton>
        <IconButton label="Close tangents drawer" onClick={onClose}>
          <X className="h-4 w-4" />
        </IconButton>
      </header>

      <div className="tangent-starting-point shrink-0 border-b border-border-subtle bg-surface/50 p-3.5">
        <div className="flex items-center justify-between gap-2">
          <span className="truncate text-[11px] font-medium text-text-muted">
            {breadcrumb}
          </span>
          {isDifferentLesson && (
            <button
              type="button"
              onClick={() => onJumpToLesson?.(tangent.moduleId, tangent.lessonId)}
              className="shrink-0 text-xs text-accent hover:underline"
            >
              Jump to lesson
            </button>
          )}
        </div>
        <blockquote className="tangent-starting-point-quote mt-2 max-h-28 overflow-y-auto whitespace-pre-wrap wrap-break-word border-l-2 border-accent pl-3 font-serif text-xs leading-relaxed text-text-secondary">
          {tangent.selectedText}
        </blockquote>
        <div className="mt-2.5 flex items-center justify-between gap-2 border-t border-border-subtle/60 pt-2 text-xs">
          <button
            type="button"
            onClick={() => void handleOpenInChat()}
            className="inline-flex items-center gap-1 text-[11px] text-text-secondary hover:text-accent"
          >
            <ExternalLink className="h-3 w-3" />
            Open in Chat
          </button>
          <IconButton
            label="Delete tangent"
            disabled={isSending || mutating}
            onClick={() => void handleDelete()}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </IconButton>
        </div>
      </div>

      <div
        ref={transcriptRef}
        className="min-h-0 flex-1 overflow-y-auto p-3"
        onScroll={(event) => {
          const element = event.currentTarget;
          followBottom.current =
            element.scrollHeight - element.scrollTop - element.clientHeight < 80;
        }}
      >
        {(messagesQuery.isPending || detailQuery.isPending) && (
          <p role="status" className="p-4 text-sm text-text-muted">
            Opening tangent…
          </p>
        )}
        {(messagesQuery.isError ||
          detailQuery.isError ||
          (detailQuery.isSuccess && !detailQuery.data)) && (
          <div role="alert" className="p-4 text-sm">
            <p>Couldn’t open this tangent.</p>
            <button
              type="button"
              className="mt-2 text-accent underline"
              onClick={() => {
                void messagesQuery.refetch();
                void detailQuery.refetch();
              }}
            >
              Try again
            </button>
          </div>
        )}

        {messagesQuery.isSuccess && messages.length === 0 && !isSending && (
          <div className="space-y-3 p-2 text-center sm:p-4">
            <p className="text-sm leading-relaxed text-text-muted">
              Ask a question or explore a side-topic about this study guide passage.
            </p>
            <div className="flex flex-col gap-1.5 pt-1">
              {STARTER_PROMPTS.map((prompt) => (
                <button
                  key={prompt}
                  type="button"
                  onClick={() => void send(prompt)}
                  className="rounded-lg border border-border bg-surface px-3 py-2 text-left text-xs text-text-secondary hover:border-accent hover:text-accent focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent"
                >
                  {prompt} →
                </button>
              ))}
            </div>
          </div>
        )}

        <ConversationSnapshotProvider value={scope}>
          {messages.map((message, index) => (
            <Message
              key={'id' in message ? message.id : message.tempId}
              message={message}
              isLastTurn={index === messages.length - 1}
              previousMessageId={
                persisted[Math.max(1, tangent.contextMessageCount) + index - 1]?.id
              }
            />
          ))}
        </ConversationSnapshotProvider>
      </div>

      <form
        className="shrink-0 border-t border-border-subtle p-3"
        onSubmit={(event) => {
          event.preventDefault();
          void send();
        }}
      >
        <label htmlFor={`tangent-composer-${id}`} className="sr-only">
          Ask in this tangent
        </label>
        <textarea
          ref={textareaRef}
          id={`tangent-composer-${id}`}
          value={input}
          rows={3}
          placeholder="Ask a question about this passage…"
          className="w-full resize-none rounded-lg border border-border-subtle bg-surface px-3 py-2 text-sm outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
          onChange={(event) => setInput(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) {
              event.preventDefault();
              void send();
            }
          }}
        />
        <div className="mt-2 flex items-center justify-between gap-2">
          <span className="text-[11px] text-text-muted">
            {isSending ? 'Generating answer…' : 'Press Enter to send'}
          </span>
          {isSending ? (
            <IconButton
              label="Stop tangent response"
              onClick={() => void base.cancelGeneration(id)}
            >
              <Square className="h-4 w-4" />
            </IconButton>
          ) : (
            <button
              type="submit"
              aria-label="Send tangent message"
              disabled={!input.trim() || mutating || !messagesQuery.isSuccess || !detailQuery.data}
              className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-accent text-[hsl(var(--accent-fg))] disabled:opacity-40 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
            >
              <ArrowUp className="h-4 w-4" />
            </button>
          )}
        </div>
      </form>
    </div>
  );
}
