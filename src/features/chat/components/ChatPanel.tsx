import { useMemo, useRef, useState, type ReactNode } from 'react';

import { motion, useReducedMotion } from 'framer-motion';

import { ChatEmptyStateIngestDelta } from '@/features/chat/components/ChatEmptyStateIngestDelta';
import { chatPaletteCommands } from '@/features/chat/components/chatPaletteCommands';
import { ChatStarters } from '@/features/chat/components/ChatStarters';
import { CompactionStatus } from '@/features/chat/components/CompactionStatus';
import { Composer } from '@/features/chat/components/composer/Composer';
import { ConversationLinkedDocumentsPanel } from '@/features/chat/components/ConversationLinkedDocumentsPanel';
import { ConversationMemoryPanel } from '@/features/chat/components/ConversationMemoryPanel';
import { ConversationNavigator } from '@/features/chat/components/ConversationNavigator';
import { ImportFailuresNotice } from '@/features/chat/components/ImportFailuresNotice';
import { Message } from '@/features/chat/components/Message';
import { NoConversationOpen } from '@/features/chat/components/NoConversationOpen';
import { ConversationTangents } from '@/features/chat/components/tangents/ConversationTangents';
import { UtilityModelNotice } from '@/features/chat/components/UtilityModelNotice';
import { VirtualizedMessageList } from '@/features/chat/components/VirtualizedMessageList';
import { useAttachmentImport } from '@/features/chat/hooks/useAttachmentImport';
import { useChatCompaction } from '@/features/chat/hooks/useChatCompaction';
import { useChatSwitchActions } from '@/features/chat/hooks/useChatSwitchActions';
import { useComposer } from '@/features/chat/hooks/useComposer';
import { useThreadMessages } from '@/features/chat/hooks/useThreadMessages';
import { useThreadScroll } from '@/features/chat/hooks/useThreadScroll';
import { useCloudModelCatalogs } from '@/features/model/hooks/useCloudModels';
import { useDownloadedModels } from '@/features/model/hooks/useDownloadedModels';
import { degradedReasonFor, useLoadedModels } from '@/features/model/hooks/useLoadedModels';
import { selectIsChatWarming, useModelWarmupStore } from '@/features/model/stores/modelWarmupStore';
import { useRegisterPaletteCommands } from '@/features/palette/hooks/useRegisterPaletteCommands';
import { useSettingsQuery } from '@/features/settings/hooks/useSettingsQuery';
import { useOpenSpaces } from '@/features/spaces/components/SpacePickerPopover';
import { GENERAL_SPACE_ID } from '@/features/spaces/model/spaces';
import type { ChatModelChoice } from '@/lib/bindings';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import { toast } from '@/stores/toastStore';
import {
  buildChatModelOptions,
  isUnavailableChatModel,
  resolveDefaultChatOption,
  resolveEffectiveChatOption,
  sameModelRef,
} from '@/utils/chatModelSelection';
import { createDefaultConversationTitle } from '@/utils/conversationTitles';
import { handleAsyncEvent } from '@/utils/promiseHandlers';

interface ChatPanelProps {
  /** A line above the composer from the surface hosting this chat. */
  composerNotice?: ReactNode;
  /** Chips at the top of the composer for what the host sends with the next turn. */
  composerChips?: ReactNode;
}

const DEFAULT_CHOICE: ChatModelChoice = { kind: 'default' };

export function ChatPanel({ composerNotice, composerChips }: ChatPanelProps = {}) {
  const { downloadedModels } = useDownloadedModels();
  const { catalogs } = useCloudModelCatalogs();
  const loadedModels = useLoadedModels();
  const activeConversationId = useConversationsStore((state) => state.activeConversationId);
  const conversations = useConversationsStore((state) => state.conversations);
  const spaces = useConversationsStore((state) => state.spaces);
  const selectedSpaceId = useConversationsStore((state) => state.selectedSpaceId);
  const inFlightGenerations = useConversationsStore((state) => state.inFlightGenerations);
  const optimisticMessages = useConversationsStore((state) => state.optimisticMessages);
  const liveModel = useConversationsStore((state) => state.liveModel);
  const messageRetrieval = useConversationsStore((state) => state.messageRetrieval);
  const cancelGeneration = useConversationsStore((state) => state.cancelGeneration);
  const createConversation = useConversationsStore((state) => state.createConversation);
  const regenerateResponse = useConversationsStore((state) => state.regenerateResponse);
  const forkConversation = useConversationsStore((state) => state.forkConversation);
  const selectConversation = useConversationsStore((state) => state.selectConversation);
  const settings = useSettingsQuery().data;

  const isSending = activeConversationId
    ? inFlightGenerations.has(activeConversationId)
    : false;

  const [isCreatingConversation, setIsCreatingConversation] = useState(false);
  // The memory reader is an overlay off the palette, like the source reader:
  // a read-only look at what was recorded, never a place to edit it.
  const [isMemoryOpen, setIsMemoryOpen] = useState(false);
  const panelRef = useRef<HTMLDivElement>(null);
  const modelLabelRef = useRef<HTMLButtonElement>(null);
  const prefersReducedMotion = useReducedMotion();

  const { messages, messageContentById, getMessageKey, getPersistedMessageId } = useThreadMessages(
    activeConversationId,
    conversations,
    optimisticMessages
  );
  const {
    scrollContainerRef,
    messageListRef,
    shouldAutoScrollRef,
    visibleMessage,
    handleVisibleMessage,
    navigateToMessage,
    navigateToLatest,
    freshMessageKeys,
  } = useThreadScroll({ activeConversationId, messages, getMessageKey, prefersReducedMotion });

  const { compactionRun, dismissCompaction, isCompacting, compactionRecord, handleCompact } = useChatCompaction({
    activeConversationId,
    conversations,
    scrollContainerRef,
    shouldAutoScrollRef,
  });

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

  // This chat's model: its own choice, or the Settings default it follows.
  const llmSettings = settings?.llm;
  const modelChoice = activeConversation?.modelChoice ?? DEFAULT_CHOICE;
  const onDefaultModel = modelChoice.kind === 'default';
  const effectiveModel = useMemo(
    () => resolveEffectiveChatOption(modelChoice, llmSettings, downloadedModels, catalogs),
    [modelChoice, llmSettings, downloadedModels, catalogs]
  );
  const defaultModelOption = useMemo(
    () => resolveDefaultChatOption(llmSettings, downloadedModels, catalogs),
    [llmSettings, downloadedModels, catalogs]
  );
  const chatModelOptions = useMemo(
    () => buildChatModelOptions(llmSettings, downloadedModels, catalogs),
    [llmSettings, downloadedModels, catalogs]
  );
  const effectiveRef = effectiveModel && !isUnavailableChatModel(effectiveModel) ? effectiveModel.ref : null;
  // While a turn runs, the model it is pinned to says whether it fell back to
  // the CPU; otherwise the loaded models do. A choice changed mid-turn is not
  // the pinned model, so only a match counts.
  const pinnedModel = activeConversationId && isSending ? liveModel.get(activeConversationId) : undefined;
  const degradedReason = pinnedModel && sameModelRef(pinnedModel.model, effectiveRef)
    ? pinnedModel.degraded ?? null
    : degradedReasonFor(effectiveRef, loadedModels);
  const activeModelLabel = effectiveModel?.label ?? null;
  const hasChatModel = effectiveModel !== null;
  // Showing a picture again only helps a model that can see it.
  const chatModelSeesImages = Boolean(
    effectiveModel && !isUnavailableChatModel(effectiveModel) && effectiveModel.seesImages === true,
  );

  // Mask the input while the default model warms up, and while there is no
  // model to answer with. Warm-up is the default model's: a chat on a model
  // of its own neither waits for it nor fails with it.
  const isDefaultWarming = useModelWarmupStore(selectIsChatWarming);
  const defaultWarmupPhase = useModelWarmupStore((state) => state.chat.phase);
  const isChatWarming = onDefaultModel && isDefaultWarming;
  const chatWarmupPhase = onDefaultModel ? defaultWarmupPhase : 'idle';
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
  const { handleChangeSpace, handleSearchGeneral, handleChooseChatModel, handleUseDefault } = useChatSwitchActions({
    activeConversationId,
    applyFocusDocuments,
  });

  const handleCreateEmptyStateConversation = async () => {
    if (isCreatingConversation) return;
    setIsCreatingConversation(true);
    try {
      await createConversation(createDefaultConversationTitle());
    } catch (error) {
      // The controller's message names the actual fix (an unavailable space,
      // a database error); swallowing it left a dead button. A missing model
      // no longer stops a chat being made: its first turn says so.
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
    () =>
      chatPaletteCommands({
        activeConversationId,
        activeModelLabel,
        canRegenerate,
        canSwitchModel: chatModelOptions.length > 0,
        downloadedModels,
        forkConversation,
        chooseFiles,
        handleCompact,
        handleSearchGeneral,
        conversationSpaceName,
        isCompacting,
        isScopedToLinkedFiles,
        messageCount: messages.length,
        regenerateResponse,
        resolveEffectiveToolPreferences,
        setIsMemoryOpen,
        modelLabelRef,
      }),
    [
      activeConversationId,
      activeModelLabel,
      canRegenerate,
      chatModelOptions.length,
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
      <NoConversationOpen
        isCreatingConversation={isCreatingConversation}
        onCreate={handleCreateEmptyStateConversation}
      />
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
                const next = index < messages.length - 1 ? messages[index + 1] : null;
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
                      nextMessageId={next && 'id' in next ? next.id : undefined}
                      chatModelSeesImages={chatModelSeesImages}
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
        modelNotice={{
          hasChatModel,
          warmupPhase: chatWarmupPhase,
          retrievalUnavailableReason,
          unavailableModel: isUnavailableChatModel(effectiveModel)
            ? { reason: effectiveModel.reason, onUseDefault: () => void handleUseDefault() }
            : null,
        }}
        notice={composerNotice}
        chips={composerChips}
        space={showSpacePicker ? { id: conversationSpaceId, name: conversationSpaceName, onChange: handleAsyncEvent(handleChangeSpace) } : null}
        model={{
          choice: modelChoice,
          effective: effectiveModel,
          defaultOption: defaultModelOption,
          degraded: degradedReason,
          canChoose: chatModelOptions.length > 0,
          onSelect: handleChooseChatModel,
          onUseDefault: handleUseDefault,
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
