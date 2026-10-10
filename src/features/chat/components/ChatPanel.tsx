import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';

import { motion, useReducedMotion } from 'framer-motion';
import { Cpu, GitBranch, Library, MessageCircle, Paperclip, RefreshCw, Scissors, ScrollText } from 'lucide-react';

import { ChatEmptyStateIngestDelta } from '@/features/chat/components/ChatEmptyStateIngestDelta';
import { ChatStarters } from '@/features/chat/components/ChatStarters';
import { CompactionStatus } from '@/features/chat/components/CompactionStatus';
import { Composer } from '@/features/chat/components/composer/Composer';
import { ConversationLinkedDocumentsPanel } from '@/features/chat/components/ConversationLinkedDocumentsPanel';
import { ConversationMemoryPanel } from '@/features/chat/components/ConversationMemoryPanel';
import { ConversationNavigator } from '@/features/chat/components/ConversationNavigator';
import { ImportFailuresNotice } from '@/features/chat/components/ImportFailuresNotice';
import { Message } from '@/features/chat/components/Message';
import { ConversationTangents } from '@/features/chat/components/tangents/ConversationTangents';
import { UtilityModelNotice } from '@/features/chat/components/UtilityModelNotice';
import { VirtualizedMessageList, type VirtualizedMessageListHandle } from '@/features/chat/components/VirtualizedMessageList';
import { useAttachmentImport } from '@/features/chat/hooks/useAttachmentImport';
import { useComposer } from '@/features/chat/hooks/useComposer';
import { useCompactionStore } from '@/features/chat/stores/compactionStore';
import { useDownloadedModels } from '@/features/model/hooks/useDownloadedModels';
import { selectIsChatWarming, useModelWarmupStore } from '@/features/model/stores/modelWarmupStore';
import { useRegisterPaletteCommands } from '@/features/palette/hooks/useRegisterPaletteCommands';
import { useSettingsQuery } from '@/features/settings/hooks/useSettingsQuery';
import { useOpenSpaces } from '@/features/spaces/components/SpacePickerPopover';
import { GENERAL_SPACE_ID } from '@/features/spaces/model/spaces';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import { toast } from '@/stores/toastStore';
import { resolveChatModel } from '@/utils/chatModelSelection';
import { createDefaultConversationTitle } from '@/utils/conversationTitles';

interface ChatPanelProps {
  /** A line above the composer from the surface hosting this chat. */
  composerNotice?: ReactNode;
  /** Chips at the top of the composer for what the host sends with the next turn. */
  composerChips?: ReactNode;
}

export function ChatPanel({ composerNotice, composerChips }: ChatPanelProps = {}) {
  const { activeModel, downloadedModels, setActiveChatModel } = useDownloadedModels();
  const activeConversationId = useConversationsStore((state) => state.activeConversationId);
  const conversations = useConversationsStore((state) => state.conversations);
  const spaces = useConversationsStore((state) => state.spaces);
  const selectedSpaceId = useConversationsStore((state) => state.selectedSpaceId);
  const inFlightGenerations = useConversationsStore((state) => state.inFlightGenerations);
  const optimisticMessages = useConversationsStore((state) => state.optimisticMessages);
  const messageRetrieval = useConversationsStore((state) => state.messageRetrieval);
  const cancelGeneration = useConversationsStore((state) => state.cancelGeneration);
  const createConversation = useConversationsStore((state) => state.createConversation);
  const regenerateResponse = useConversationsStore((state) => state.regenerateResponse);
  const forkConversation = useConversationsStore((state) => state.forkConversation);
  const compactConversation = useConversationsStore((state) => state.compactConversation);
  const moveConversationToSpace = useConversationsStore((state) => state.moveConversationToSpace);
  const selectConversation = useConversationsStore((state) => state.selectConversation);
  const settings = useSettingsQuery().data;

  const isSending = activeConversationId
    ? inFlightGenerations.has(activeConversationId)
    : false;

  const [isCreatingConversation, setIsCreatingConversation] = useState(false);
  // The memory reader is an overlay off the palette, like the source reader:
  // a read-only look at what was recorded, never a place to edit it.
  const [isMemoryOpen, setIsMemoryOpen] = useState(false);
  // A /compact outlives this panel (it carries on across a switch to
  // Explorer and back), so where it stands lives in a store.
  const compactionRun = useCompactionStore((state) =>
    activeConversationId ? state.runs[activeConversationId] : undefined
  );
  const dismissCompaction = useCompactionStore((state) => state.dismiss);
  const isCompacting = compactionRun?.state === 'running';
  const panelRef = useRef<HTMLDivElement>(null);
  const modelLabelRef = useRef<HTMLButtonElement>(null);
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const messageListRef = useRef<VirtualizedMessageListHandle>(null);
  const previousConversationIdRef = useRef<string | null>(null);
  const previousMessageCountRef = useRef(0);
  const shouldAutoScrollRef = useRef(true);
  const [visibleMessage, setVisibleMessage] = useState<{ conversationId: string | null; index: number }>({ conversationId: null, index: 0 });
  const knownMessageIdsRef = useRef<{ conversationId: string | null; ids: Set<string> }>({
    conversationId: null,
    ids: new Set(),
  });
  const prefersReducedMotion = useReducedMotion();

  const messages = useMemo(() => {
    if (!activeConversationId) return [];

    const realMessages =
      conversations.find((conversation) => conversation.id === activeConversationId)?.messages ?? [];
    const optimisticForConversation = Array.from(optimisticMessages.values()).filter(
      (message) =>
        message.conversationId === activeConversationId || message.conversationId === 'temp'
    );
    return [...realMessages, ...optimisticForConversation];
  }, [activeConversationId, conversations, optimisticMessages]);

  /**
   * Raw content of the loaded messages, for resolving memory evidence spans.
   *
   * The memory panel needs the stored text, not the rendered markdown: its
   * spans are UTF-8 byte offsets into what the backend holds.
   */
  const messageContentById = useMemo(() => {
    const byId = new Map<string, string>();
    for (const message of messages) {
      if ('id' in message && typeof message.content === 'string') {
        byId.set(message.id, message.content);
      }
    }
    return byId;
  }, [messages]);

  const getMessageKey = useCallback(
    (message: typeof messages[number]): string =>
      'tempId' in message ? message.tempId : message.id,
    []
  );
  const getPersistedMessageId = useCallback(
    (message: typeof messages[number]): string | null => 'id' in message ? message.id : null,
    []
  );

  const handleVisibleMessage = useCallback((index: number) => {
    setVisibleMessage({ conversationId: activeConversationId, index });
  }, [activeConversationId]);

  const navigateToMessage = useCallback((index: number) => {
    // Jump to the beginning even when the answer is several screens tall.
    // Disable following first so a streaming update cannot pull us back down.
    shouldAutoScrollRef.current = false;
    messageListRef.current?.scrollToIndex(index, { align: 'start', behavior: 'auto' });
  }, []);

  const navigateToLatest = useCallback(() => {
    shouldAutoScrollRef.current = true;
    messageListRef.current?.scrollToIndex(messages.length - 1, { align: 'end', behavior: 'auto' });
  }, [messages.length]);

  const freshMessageKeys = useMemo(() => {
    const fresh = new Set<string>();
    const tracker = knownMessageIdsRef.current;
    if (tracker.conversationId !== activeConversationId) {
      tracker.conversationId = activeConversationId ?? null;
      tracker.ids = new Set(messages.map(getMessageKey));
      return fresh;
    }
    for (const message of messages) {
      const key = getMessageKey(message);
      if (!tracker.ids.has(key)) {
        fresh.add(key);
        tracker.ids.add(key);
      }
    }
    return fresh;
  }, [activeConversationId, getMessageKey, messages]);

  useEffect(() => {
    const container = scrollContainerRef.current;
    if (!container) return;

    const previousConversationId = previousConversationIdRef.current;
    const isConversationChange = previousConversationId !== activeConversationId;
    const isInitialMessageLoad = previousMessageCountRef.current === 0;
    previousConversationIdRef.current = activeConversationId ?? null;
    previousMessageCountRef.current = messages.length;

    if (isConversationChange) {
      shouldAutoScrollRef.current = true;
    } else if (!shouldAutoScrollRef.current) {
      return;
    }

    // The global `prefers-reduced-motion` rule in index.css covers CSS
    // transitions, not programmatic scrolling: this ride has to be opted out
    // of here or it happens on every message.
    messageListRef.current?.scrollToIndex(messages.length - 1, {
      align: 'end',
      // The conversation can arrive before its messages. Treat that first
      // batch as a jump too; smooth scrolling through estimated long rows
      // can stop partway down the history and compete with a navigation click.
      behavior: isConversationChange || isInitialMessageLoad || prefersReducedMotion ? 'auto' : 'smooth',
    });
  }, [messages, activeConversationId, prefersReducedMotion]);

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
    [compactConversation]
  );

  const handleCancel = async () => {
    if (!activeConversationId || !isSending) return;
    await cancelGeneration(activeConversationId);
  };

  const activeConversation = useMemo(
    () => conversations.find((conversation) => conversation.id === activeConversationId) ?? null,
    [conversations, activeConversationId]
  );
  const conversationSpaceId = activeConversation?.spaceId ?? null;
  const isScopedToLinkedFiles = Boolean(
    conversationSpaceId && conversationSpaceId !== GENERAL_SPACE_ID
  );
  /**
   * Which space the empty state's suggested questions come from. The chat being
   * shown decides; before one exists, the sidebar's selection is the next best
   * answer, because that is the space the first message will land in.
   */
  const startersSpaceId = conversationSpaceId ?? selectedSpaceId ?? null;

  const attachments = useAttachmentImport({
    dropTargetRef: panelRef,
    conversationId: activeConversationId ?? null,
    scopedSpaceId: isScopedToLinkedFiles ? conversationSpaceId : null,
  });

  const openSpaces = useOpenSpaces();
  const conversationSpaceName =
    spaces.find((space) => space.id === conversationSpaceId)?.name ?? null;
  // With General alone there is nothing to choose and nothing to explain.
  const showSpacePicker = Boolean(
    activeConversationId && conversationSpaceName && (openSpaces.length > 1 || isScopedToLinkedFiles)
  );

  // Mask the input during warmup, and while there is no model to answer with.
  const isChatWarming = useModelWarmupStore(selectIsChatWarming);
  const chatWarmupPhase = useModelWarmupStore((state) => state.chat.phase);

  const llmSettings = settings?.llm;
  const resolvedModel = resolveChatModel(llmSettings, activeModel?.model_id ?? null);
  const activeModelLabel = resolvedModel && (downloadedModels.find(model => model.model_id === resolvedModel)?.model_name || resolvedModel);
  const hasChatModel = activeModelLabel !== null;
  const isChatUnavailable = isChatWarming || !hasChatModel;

  // Report the completed turn. Initial retrieval can fail and a later tool
  // search can recover while generation is still running.
  const retrievalUnavailableReason = useMemo(() => {
    if (!activeConversationId || isSending) return null;
    const conversationMessages =
      conversations.find((conversation) => conversation.id === activeConversationId)?.messages ?? [];
    for (let index = conversationMessages.length - 1; index >= 0; index -= 1) {
      const candidate = conversationMessages[index];
      if (candidate.role !== 'assistant') continue;
      const trace = messageRetrieval.get(candidate.id);
      if (trace && (trace.files > 0 || trace.passages > 0)) return null;
      return trace?.unavailableReason ?? null;
    }
    return null;
  }, [activeConversationId, conversations, isSending, messageRetrieval]);

  const composer = useComposer({
    conversationId: activeConversationId ?? null,
    conversationSpaceId,
    conversationSpaceName,
    isSending,
    isCompacting,
    isChatUnavailable,
    canCompact: Boolean(activeConversationId) && !isCompacting && messages.length > 0,
    onCompact: (conversationId) => void handleCompact(conversationId),
    attachments,
  });
  const { applyFocusDocuments, resolveEffectiveToolPreferences } = composer;
  const { chooseFiles } = attachments;

  /**
   * Move this chat to another space, which changes the documents every later
   * answer can draw on. The toast names the space: General is a space like any
   * other, and calling a move there "searching your whole vault" promised
   * documents filed elsewhere that it cannot reach.
   */
  const handleChangeSpace = useCallback(
    async (spaceId: string, spaceName: string) => {
      if (!activeConversationId) return;
      // A document pinned in the old space is not in the new one, and focus
      // fails closed: keeping it would leave the chat asking nothing at all.
      applyFocusDocuments([]);
      await moveConversationToSpace(activeConversationId, spaceId);
      toast.success(`Now searching ${spaceName}`, {
        message: 'Answers from here on use the documents filed there.',
      });
    },
    [activeConversationId, applyFocusDocuments, moveConversationToSpace]
  );

  const handleSearchGeneral = useCallback(
    () => handleChangeSpace(GENERAL_SPACE_ID, 'General'),
    [handleChangeSpace]
  );

  const handleSwitchActiveModel = useCallback(
    async (modelId: string, modelLabel: string) => {
      const previousModelId = activeModel?.model_id ?? null;
      const previousLabel = activeModel?.model_name ?? previousModelId;
      try {
        await setActiveChatModel(modelId);
      } catch (error) {
        toast.error("Couldn't switch model", {
          message: error instanceof Error ? error.message : String(error),
        });
        return;
      }
      // The switch is app-wide, so always offer the way back — same as the
      // "Try with another model" toast on a message.
      toast.success(`Switched to ${modelLabel}`, {
        ...(previousModelId && previousModelId !== modelId
          ? {
              action: {
                label: 'Switch back',
                onClick: () => {
                  void setActiveChatModel(previousModelId).then(() => {
                    toast.success(`Back to ${previousLabel}`);
                  });
                },
              },
            }
          : {}),
      });
    },
    [activeModel, setActiveChatModel]
  );

  const handleCreateEmptyStateConversation = async () => {
    if (isCreatingConversation) return;
    setIsCreatingConversation(true);
    try {
      await createConversation(createDefaultConversationTitle());
    } catch (error) {
      // The controller's message names the actual fix ("Select a local model in
      // Settings or configure Ollama"); swallowing it left a dead button.
      toast.error("Couldn't start a conversation", {
        message: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setIsCreatingConversation(false);
    }
  };

  const lastMessage = messages.length > 0 ? messages[messages.length - 1] : null;
  const canRegenerate = Boolean(
    activeConversationId && !isSending && lastMessage?.role === 'assistant'
  );
  const paletteCommands = useMemo(
    () => [
      {
        id: 'chat.regenerate',
        label: 'Regenerate answer',
        group: 'Chat',
        icon: RefreshCw,
        enabled: canRegenerate,
        run: () => {
          if (!activeConversationId) return;
          // Re-run the turn the composer is describing, not a default one.
          void regenerateResponse(
            activeConversationId,
            resolveEffectiveToolPreferences()
          ).then((outcome) => {
            if (outcome === 'answered' || outcome === 'cancelled') return;
            toast.error("Couldn't regenerate", {
              message:
                outcome === 'busy'
                  ? 'This conversation is still answering.'
                  : 'Your question is back in the composer.',
            });
          });
        },
      },
      {
        id: 'chat.branch',
        label: 'Branch this conversation',
        group: 'Chat',
        icon: GitBranch,
        enabled: Boolean(activeConversationId) && messages.length > 0,
        run: () => {
          if (activeConversationId) {
            void forkConversation(activeConversationId).then((newId) => {
              if (newId) toast.success('Branched', { message: "You're in the new conversation." });
            });
          }
        },
      },
      {
        id: 'chat.compact',
        label: 'Compact context',
        group: 'Chat',
        icon: Scissors,
        enabled: Boolean(activeConversationId) && !isCompacting && messages.length > 0,
        description: 'Fold older messages into a summary (/compact).',
        run: () => {
          if (activeConversationId) void handleCompact(activeConversationId);
        },
      },
      {
        id: 'chat.memory',
        label: 'Show conversation memory',
        group: 'Chat',
        icon: ScrollText,
        enabled: Boolean(activeConversationId),
        description: 'What was recorded, and the quotation behind each item.',
        run: () => {
          setIsMemoryOpen(true);
        },
      },
      {
        id: 'chat.switch-model',
        label: 'Switch chat model',
        group: 'Chat',
        icon: Cpu,
        // The picker hangs off the active-model label, so the verb only works
        // when that label is on screen. No verb that does nothing.
        enabled:
          Boolean(activeModelLabel) &&
          downloadedModels.some((model) => model.model_type === 'language_model'),
        run: () => {
          modelLabelRef.current?.click();
        },
      },
      {
        id: 'chat.add-files',
        label: 'Add files to this conversation',
        group: 'Chat',
        icon: Paperclip,
        enabled: Boolean(activeConversationId),
        description: 'Or drop them onto the conversation.',
        run: () => {
          void chooseFiles();
        },
      },
      {
        id: 'chat.search-general',
        label: 'Move this chat to General',
        group: 'Chat',
        icon: Library,
        description: conversationSpaceName
          ? `It searches ${conversationSpaceName} now.`
          : undefined,
        enabled: isScopedToLinkedFiles,
        run: () => {
          void handleSearchGeneral();
        },
      },
    ],
    [
      activeConversationId,
      activeModelLabel,
      canRegenerate,
      downloadedModels,
      forkConversation,
      chooseFiles,
      handleCompact,
      handleSearchGeneral,
      conversationSpaceName,
      isCompacting,
      isScopedToLinkedFiles,
      messages.length,
      regenerateResponse,
      resolveEffectiveToolPreferences,
    ]
  );
  useRegisterPaletteCommands(paletteCommands);

  if (!activeConversationId) {
    return (
      <div className="flex h-full min-h-0 min-w-0 flex-1 items-center justify-center bg-bg">
        <div className="max-w-md px-8 text-center">
          <div className="mx-auto mb-4 flex h-11 w-11 items-center justify-center rounded-xl bg-[hsl(var(--text-primary)/0.05)] text-text-tertiary shadow-[inset_0_0_0_1px_hsl(var(--text-primary)/0.05)]">
            <MessageCircle className="h-5 w-5" strokeWidth={1.6} />
          </div>
          <p className="font-serif text-[19px] font-medium tracking-[-0.015em] text-text-primary">No conversation open.</p>
          <p className="mt-1.5 text-ui leading-relaxed text-text-muted">Pick one from the list, or start a new one and ask your library a question.</p>
          <button
            type="button"
            onClick={() => { void handleCreateEmptyStateConversation(); }}
            disabled={isCreatingConversation}
            aria-label="Create new conversation"
            className="pressable mt-5 inline-flex h-8 items-center justify-center gap-2 rounded-md bg-action px-3 text-ui font-medium text-action-fg shadow-action transition-[background-color,scale] duration-fast hover:bg-action-hover disabled:cursor-not-allowed disabled:opacity-50"
          >
            New conversation
            <kbd className="kbd bg-[hsl(var(--action-fg)/0.14)] text-[hsl(var(--action-fg)/0.8)] shadow-none">⌘N</kbd>
          </button>
        </div>
      </div>
    );
  }

  return (
    <ConversationTangents key={activeConversationId} conversationId={activeConversationId} toolPreferences={composer.toolPreferences} unavailable={isChatUnavailable}>
    <div
      ref={panelRef}
      data-thread={messages.length > 0 || undefined}
      className="chat-panel relative flex h-full min-h-0 min-w-0 flex-1 flex-col bg-bg"
    >
      {attachments.isDragging && (
        <div className="pointer-events-none absolute inset-0 z-30 flex items-center justify-center border-2 border-dashed border-[hsl(var(--accent))] bg-bg/80 transition-opacity duration-fast">
          <p className="text-sm text-[hsl(var(--text-secondary))]">
            Drop files to add them to this conversation.
          </p>
        </div>
      )}
      <ImportFailuresNotice />
      <UtilityModelNotice />
      <ConversationMemoryPanel
        conversationId={activeConversationId}
        isOpen={isMemoryOpen}
        onClose={() => setIsMemoryOpen(false)}
        messageContentById={messageContentById}
        onOpenConversation={(id) => { setIsMemoryOpen(false); void selectConversation(id); }}
      />
      {/* Thread scroll region */}
      <div className="relative flex min-h-0 flex-1">
      <div
        ref={scrollContainerRef}
        className={`min-h-0 min-w-0 flex-1 overflow-y-auto ${messages.length >= 2 ? 'pr-12' : ''}`}
        onScroll={(event) => {
          const container = event.currentTarget;
          const distanceFromBottom =
            container.scrollHeight - container.scrollTop - container.clientHeight;
          shouldAutoScrollRef.current = distanceFromBottom <= 96;
        }}
      >
        <div className="chat-column mx-auto w-full">
          {messages.length === 0 ? (
            <div className="flex min-h-[50vh] flex-col items-center justify-center px-6">
              <div className="w-full max-w-[520px]">
                <ChatStarters
                  spaceId={startersSpaceId}
                  onPick={composer.fill}
                />
                <ChatEmptyStateIngestDelta />
              </div>
            </div>
          ) : (
            <motion.div
              key={activeConversationId ?? 'empty'}
              initial={prefersReducedMotion ? false : { opacity: 0 }}
              animate={{ opacity: 1 }}
              transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
            >
              <VirtualizedMessageList
                ref={messageListRef}
                items={messages}
                scrollElementRef={scrollContainerRef}
                getKey={getMessageKey}
                getMessageId={getPersistedMessageId}
                onVisibleIndexChange={handleVisibleMessage}
                renderItem={(message, index) => {
                const key = getMessageKey(message);
                const previous = index > 0 ? messages[index - 1] : null;
                const showCompactionDivider =
                  Boolean(compactionRecord) &&
                  'id' in message &&
                  message.id === compactionRecord?.upToMessageId;
                return (
                  <>
                    <Message
                      message={message}
                      isFresh={!prefersReducedMotion && freshMessageKeys.has(key)}
                      isLastTurn={index === messages.length - 1}
                      previousMessageId={
                        previous && 'id' in previous ? previous.id : undefined
                      }
                    />
                    {showCompactionDivider && compactionRecord && (
                      <div
                        className="chat-beside-margin flex items-center gap-3 px-6 py-2"
                        role="separator"
                        aria-label="Context compacted"
                      >
                        <div className="h-px flex-1 bg-[hsl(var(--border-default))]" />
                        <span className="text-xxs uppercase tracking-wide text-[hsl(var(--text-muted))]">
                          Context compacted
                        </span>
                        <div className="h-px flex-1 bg-[hsl(var(--border-default))]" />
                      </div>
                    )}
                  </>
                );
              }}
              />
            </motion.div>
          )}

          {/* Where a /compact stands: running, then what it did or why not. */}
          {compactionRun && activeConversationId && (
            <CompactionStatus
              key={`${activeConversationId}-${compactionRun.startedAt}`}
              run={compactionRun}
              onDismiss={() => dismissCompaction(activeConversationId)}
            />
          )}

          {/* Sticky sources-in-this-conversation footer, above the composer */}
          <div className="chat-beside-margin px-6">
            <ConversationLinkedDocumentsPanel
              conversationId={activeConversationId}
              scopedSpaceName={isScopedToLinkedFiles ? conversationSpaceName : null}
              onMoveToGeneral={() => void handleSearchGeneral()}
            />
          </div>
        </div>
      </div>

        <ConversationNavigator
          key={activeConversationId}
          messages={messages}
          activeIndex={visibleMessage.conversationId === activeConversationId ? visibleMessage.index : messages.length - 1}
          getKey={getMessageKey}
          onNavigate={navigateToMessage}
          onLatest={navigateToLatest}
        />
      </div>

      <Composer
        composer={composer}
        attachments={attachments}
        modelNotice={{ hasChatModel, warmupPhase: chatWarmupPhase, retrievalUnavailableReason }}
        notice={composerNotice}
        chips={composerChips}
        space={showSpacePicker ? { id: conversationSpaceId, name: conversationSpaceName, onChange: handleChangeSpace } : null}
        model={{
          id: activeModel?.model_id ?? null,
          label: activeModelLabel,
          onSelect: handleSwitchActiveModel,
          triggerRef: modelLabelRef,
        }}
        isSending={isSending}
        isCompacting={isCompacting}
        isChatUnavailable={isChatUnavailable}
        onCancel={() => void handleCancel()}
      />
    </div>
    </ConversationTangents>
  );
}
