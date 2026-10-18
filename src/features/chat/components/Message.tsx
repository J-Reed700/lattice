import { memo, useEffect, useId, useMemo, useRef, useState } from 'react';

import { Paperclip } from 'lucide-react';
import { useNavigate } from 'react-router';

import { TiptapViewer } from '@/components/TiptapEditor';
import { ClaimActionsPopover } from '@/features/chat/components/actions/ClaimActionsPopover';
import { CitationFootnote } from '@/features/chat/components/CitationFootnote';
import { CitationHoverCard } from '@/features/chat/components/CitationHoverCard';
import { ClaimHoverCard } from '@/features/chat/components/ClaimHoverCard';
import { EvidenceMargin } from '@/features/chat/components/EvidenceMargin';
import { MessageHeader } from '@/features/chat/components/message/MessageHeader';
import { useMessageActions } from '@/features/chat/components/message/useMessageActions';
import { useMessageMetadata } from '@/features/chat/components/message/useMessageMetadata';
import { useMessageVerification } from '@/features/chat/components/message/useMessageVerification';
import { VerificationDetails } from '@/features/chat/components/message/VerificationDetails';
import { MessageActions } from '@/features/chat/components/MessageActions';
import { MessageEditor } from '@/features/chat/components/MessageEditor';
import { TangentSelection } from '@/features/chat/components/tangents/TangentSelection';
import { TurnRecord } from '@/features/chat/components/turn/TurnRecord';
import { useMessageEvidence } from '@/features/chat/hooks/useMessageEvidence';
import { useRevealStreamingRef } from '@/features/chat/hooks/useRevealStreamingRef';
import { openInFolder, revealInFolder } from '@/features/chat/model/folderThreadHost';
import { useDownloadedModels } from '@/features/model/hooks/useDownloadedModels';
import { SourceCitations } from '@/features/reading/components/SourceCitations';
import { useChatReaderStore } from '@/features/reading/stores/chatReaderStore';
import { useCitationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import { useSettingsQuery } from '@/features/settings/hooks/useSettingsQuery';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import type { DisplayMessage, SourceWithMetadata } from '@/types/conversation';
import { normalizeAssistantMarkdown } from '@/utils/assistantMarkdown';
import { resolveActiveChatOption } from '@/utils/chatModelSelection';

interface MessageProps {
  message: DisplayMessage;
  isFresh?: boolean;
  /** True when this is the newest message in the thread. */
  isLastTurn?: boolean;
  /** Id of the message immediately before this one, for "Send as branch". */
  previousMessageId?: string;
}

export const Message = memo(({
  message,
  isFresh = false,
  isLastTurn = false,
  previousMessageId,
}: MessageProps) => {
  const navigate = useNavigate();
  const answerRef = useRef<HTMLDivElement>(null);
  const verificationPanelId = useId();
  const [isVerificationPanelExpanded, setIsVerificationPanelExpanded] = useState(false);

  const isUser = message.role === 'user';
  const isPending = message.status === 'pending';
  const isFailed = message.status === 'failed';
  const errorMessage = 'error' in message ? message.error : undefined;

  const activeConversationId = useConversationsStore((s) => s.activeConversationId);
  const messageBookmarkMap = useConversationsStore((s) => s.messageBookmarkMap);
  const messageVerificationMap = useConversationsStore((s) => s.messageVerification);
  const dismissFailedMessage = useConversationsStore((s) => s.dismissFailedMessage);
  const retryFailedMessage = useConversationsStore((s) => s.retryFailedMessage);
  const lastMessageSources = useConversationsStore((s) => s.lastMessageSources);
  const messageRetrieval = useConversationsStore((s) => s.messageRetrieval);
  const liveRetrieval = useConversationsStore((s) => s.liveRetrieval);
  const liveSteps = useConversationsStore((s) => s.liveSteps);
  const messageTurn = useConversationsStore((s) => s.messageTurn);
  const inFlightGenerations = useConversationsStore((s) => s.inFlightGenerations);
  const selectConversation = useConversationsStore((s) => s.selectConversation);

  // One reader serves the whole thread. A message hands it a citation to show
  // and reads back where the viewer found it.
  const resolvedLocations = useChatReaderStore((s) => s.resolvedLocations);
  const showCitations = useCitationDisplayStore((s) => s.visible);
  const { data: settings } = useSettingsQuery();
  const { downloadedModels = [] } = useDownloadedModels();

  const messageId = 'id' in message ? message.id : undefined;
  const messageBookmark = messageId ? messageBookmarkMap.get(messageId) : undefined;
  const verificationSummary = messageId ? messageVerificationMap.get(messageId) : undefined;
  const isMessageBookmarked = Boolean(messageBookmark);
  const canBookmarkMessage = Boolean(activeConversationId && messageId);
  const canDeleteMessage = Boolean(messageId);
  const domMessageId = messageId ? `message-${messageId}` : undefined;

  const sources: SourceWithMetadata[] = useMemo(() => {
    if ('sources' in message && message.sources && message.sources.length > 0) {
      return message.sources;
    }
    if (messageId) {
      return lastMessageSources.get(messageId) ?? [];
    }
    return [];
  }, [message, messageId, lastMessageSources]);

  const isAssistantWithSources = !isUser && sources.length > 0;

  const { attachmentNames, continuedFrom } = useMessageMetadata(message, isUser);

  const conversationId = 'conversationId' in message ? message.conversationId : undefined;
  const conversationTitle = useConversationsStore(
    (s) => s.conversations.find((c) => c.id === conversationId)?.title ?? null,
  );
  const conversationSpaceId = useConversationsStore(
    (s) => s.conversations.find((c) => c.id === conversationId)?.spaceId ?? null,
  );
  // An Explorer thread: its answers' line references open the folder's files.
  const explorerRoot = useConversationsStore(
    (s) => s.conversations.find((c) => c.id === conversationId)?.explorerRoot ?? null,
  );
  const showsCodeRefs = !isUser && Boolean(explorerRoot);
  const isBusy = Boolean(conversationId && inFlightGenerations.has(conversationId));

  // In flight, the live event stream is the trace; once persisted, the message
  // metadata is. Never both, and never a stale one.
  const retrievalTrace = useMemo(() => {
    if (isUser) return null;
    if (isPending && conversationId) return liveRetrieval.get(conversationId) ?? null;
    return messageId ? messageRetrieval.get(messageId) ?? null : null;
  }, [isUser, isPending, conversationId, liveRetrieval, messageId, messageRetrieval]);

  /**
   * When this turn began, for the clocks on its running steps.
   *
   * A step records only its offset from the start of the turn, and the pending
   * bubble was created at that start — so its timestamp is the origin the
   * offsets are measured from. Reading it from the message rather than from
   * the moment a row is drawn is what lets a clock survive the record being
   * folded and opened again.
   */
  const turnStartedAt = useMemo(() => {
    if (isUser || !isPending) return null;
    const started = Date.parse(message.createdAt);
    return Number.isNaN(started) ? null : started;
  }, [isUser, isPending, message.createdAt]);

  const verification = useMessageVerification({ verificationSummary, isUser });
  const {
    claimsEvaluated,
    verificationPending,
    canExpandVerification,
    claimVerdicts,
    verificationBadge,
  } = verification;

  const normalizedMarkdownContent = useMemo(
    () => (isUser ? message.content : normalizeAssistantMarkdown(message.content)),
    [isUser, message.content]
  );
  useRevealStreamingRef(normalizedMarkdownContent, showsCodeRefs && isPending);

  const {
    isEditing,
    setIsEditing,
    activeModel,
    handleCopy,
    handleBookmarkToggle,
    handleRegenerate,
    handleTryWithModel,
    handleBranch,
    handleEditSend,
    handleEditSendAsBranch,
    handleDeleteMessage,
  } = useMessageActions({
    message,
    messageId,
    conversationId,
    conversationSpaceId,
    previousMessageId,
    activeConversationId,
    isMessageBookmarked,
    normalizedMarkdownContent,
  });

  const activeChatOption = resolveActiveChatOption(
    settings?.llm,
    downloadedModels,
    activeModel?.model_id ?? null
  );

  const {
    citationMap, citationNumbers, openCitation, openSource, provenanceBySource,
    articleRef, citationHover, claimHover, claimActions, setClaimActions,
    handleBodyClick, handleBodyMouseOver, handleBodyMouseLeave,
    hoveredSource, hoveredVerdict, activeEvidence, hasEvidenceMargin, setLit,
  } = useMessageEvidence({
    sources, ownerKey: messageId ?? domMessageId ?? '', isAssistantWithSources,
    showCitations, claimVerdicts, showsCodeRefs, revealInExplorer: revealInFolder, openInExplorer: openInFolder,
  });
  useEffect(() => {
    if (!showCitations) setIsVerificationPanelExpanded(false);
  }, [showCitations]);

  const citationFootnotes = useMemo(() => {
    if (!showCitations || !isAssistantWithSources || citationMap.size === 0) return null;
    const entries = Array.from(citationMap.entries()).sort(([a], [b]) => a - b);
    return (
      <div className="mt-1 flex flex-wrap items-center gap-2 text-xs">
        <span className="text-xxs text-text-muted">Sources</span>
        {entries.map(([num, source], position) => (
          <CitationFootnote
            key={`cite-${num}`}
            number={num}
            source={source}
            provenanceLabel={provenanceBySource.get(source.chunkId)}
            resolvedLocation={resolvedLocations.get(source.chunkId)}
            onViewFile={() => openCitation(position)}
          />
        ))}
      </div>
    );
  }, [showCitations, isAssistantWithSources, citationMap, provenanceBySource, resolvedLocations, openCitation]);

  const timestamp = new Date(message.createdAt).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
  });

  return (
    <>
      <article
        ref={articleRef}
        id={domMessageId}
        data-role={message.role}
        className={`chat-turn group px-6 ${isUser ? 'pb-2 pt-7' : 'pb-5 pt-3'}${isFresh ? ' animate-in fade-in-0 slide-in-from-bottom-1 duration-base ease-out' : ''}`}
      >
        <div className="chat-turn-grid">
        <div className="chat-turn-main min-w-0">
        <MessageHeader
          message={message}
          isUser={isUser}
          isPending={isPending}
          isFailed={isFailed}
          errorMessage={errorMessage}
          isLastTurn={isLastTurn}
          isBusy={isBusy}
          conversationId={conversationId}
          continuedFrom={continuedFrom}
          selectConversation={selectConversation}
          retryFailedMessage={retryFailedMessage}
          dismissFailedMessage={dismissFailedMessage}
          handleRegenerate={handleRegenerate}
          showCitations={showCitations}
          verificationSummary={verificationSummary}
          verificationPending={verificationPending}
          verificationBadge={verificationBadge}
          canExpandVerification={canExpandVerification}
          isVerificationPanelExpanded={isVerificationPanelExpanded}
          setIsVerificationPanelExpanded={setIsVerificationPanelExpanded}
          verificationPanelId={verificationPanelId}
          isMessageBookmarked={isMessageBookmarked}
          timestamp={timestamp}
        />

        {/* Verification details panel */}
        {showCitations && !isUser && verificationSummary && verificationSummary.enabled && claimsEvaluated > 0 && isVerificationPanelExpanded && (
          <VerificationDetails verificationPanelId={verificationPanelId} verification={verification} />
        )}

        <TurnRecord
          trace={retrievalTrace}
          record={messageId ? messageTurn.get(messageId) ?? null : null}
          liveSteps={isPending && conversationId ? liveSteps.get(conversationId) ?? null : null}
          turnStartedAt={turnStartedAt}
          isPending={!isUser && isPending}
          isWriting={message.content.trim().length > 0}
          verification={verificationSummary ?? null}
        />

        {/* Body prose — tiptap.css styles inherit `font-family` from this wrapper. */}
        {isUser && isEditing ? (
          <MessageEditor
            initialValue={message.content}
            isBusy={isBusy}
            onCancel={() => setIsEditing(false)}
            onSend={handleEditSend}
            onSendAsBranch={handleEditSendAsBranch}
          />
        ) : (
        <div
          ref={answerRef}
          onClick={isUser ? undefined : handleBodyClick}
          onMouseOver={isUser || !showCitations ? undefined : handleBodyMouseOver}
          onMouseLeave={isUser || !showCitations ? undefined : handleBodyMouseLeave}
          className={`wrap-break-word wrap-anywhere ${
            isUser
              ? 'ml-auto w-fit max-w-[85%] rounded-2xl rounded-br-md bg-surface px-4 py-2.5 font-sans text-[15px] leading-[1.55] shadow-sheet'
              : 'chat-answer max-w-none font-serif text-[16.5px] leading-[1.7]'
          }`}
        >
          <TangentSelection conversationId={conversationId} messageId={messageId} enabled={!isUser && !isPending && !isFailed && message.status !== 'processing'}>
          <TiptapViewer
            content={normalizedMarkdownContent}
            citationNumbers={citationNumbers}
            claims={isUser ? undefined : claimVerdicts}
            showEvidence={showCitations}
            codeRefs={showsCodeRefs}
          />
          </TangentSelection>
          {/* Streaming cursor — spec §3.7 */}
          {!isUser && isPending && (
            <span
              aria-hidden="true"
              className="inline-block text-[hsl(var(--text-tertiary))] align-baseline"
              style={{ width: '0.5em', animation: 'chat-cursor-blink 1s steps(2, end) infinite' }}
            >
              ▍
            </span>
          )}
        </div>
        )}

        {/* The files this message brought into the conversation. */}
        {isUser && attachmentNames.length > 0 && (
          <div className="mt-1.5 ml-auto flex w-fit max-w-[85%] flex-wrap justify-end gap-1.5">
            {attachmentNames.map((name) => (
              <span
                key={name}
                className="inline-flex items-center gap-1 rounded-full bg-surface px-2 py-0.5 text-[11px] text-[hsl(var(--text-secondary))] shadow-sheet"
              >
                <Paperclip className="h-3 w-3" strokeWidth={1.6} />
                {name}
              </span>
            ))}
          </div>
        )}

        {/* The narrow form of the evidence; the margin replaces it where there is room. */}
        <div className="evidence-inline">
        {citationFootnotes}

        {/* Source citations (assistant-only) */}
        {showCitations && isAssistantWithSources && (
          <SourceCitations
            sources={sources}
            provenanceBySource={provenanceBySource}
            resolvedLocations={resolvedLocations}
            onViewSource={openSource}
          />
        )}
        </div>

        {/* Inline actions */}
        <MessageActions
          role={message.role}
          isLastTurn={isLastTurn}
          canBookmark={canBookmarkMessage}
          canDelete={canDeleteMessage}
          canBranch={Boolean(conversationId && messageId)}
          isBookmarked={isMessageBookmarked}
          isBusy={isBusy}
          activeModelId={activeChatOption?.id ?? activeModel?.model_id ?? null}
          onCopy={handleCopy}
          onBookmarkToggle={handleBookmarkToggle}
          onDelete={handleDeleteMessage}
          onRegenerate={handleRegenerate}
          onTryWithModel={handleTryWithModel}
          onEdit={() => setIsEditing(true)}
          onBranch={handleBranch}
          tangentSource={!isUser && !isPending && !isFailed && message.status !== 'processing' && conversationId && messageId ? {
            conversationId, messageId, getText: () => answerRef.current?.innerText || normalizedMarkdownContent,
          } : undefined}
          answerSources={isAssistantWithSources ? sources : undefined}
          answerMarkdown={isUser ? undefined : normalizedMarkdownContent}
          answerVerification={verificationSummary ?? null}
          conversationTitle={conversationTitle}
          answerTurn={messageId ? messageTurn.get(messageId) : undefined}
        />
        </div>
        {showCitations && isAssistantWithSources && (
          <EvidenceMargin
            citationMap={citationMap}
            activeNumbers={activeEvidence}
            claimVerdicts={claimVerdicts}
            provenanceBySource={provenanceBySource}
            resolvedLocations={resolvedLocations}
            onOpen={(number) => {
              const source = citationMap.get(number);
              if (source) openSource(source);
            }}
            onHoverNumber={(number) => setLit(number === null ? null : `[data-cite="${number}"]`)}
            onCompareDocuments={(ids) =>
              navigate(`/compare?${new URLSearchParams({ ids: ids.join(',') }).toString()}`)
            }
          />
        )}
        </div>
      </article>

      {/* Beside a margin the passage is already on screen; a card would repeat it. */}
      <CitationHoverCard
        hover={showCitations && citationHover && !hasEvidenceMargin() ? citationHover : null}
        source={hoveredSource}
        location={hoveredSource ? resolvedLocations.get(hoveredSource.chunkId) ?? null : null}
        provenanceLabel={hoveredSource ? provenanceBySource.get(hoveredSource.chunkId) : undefined}
      />
      <ClaimHoverCard hover={showCitations ? claimHover : null} verdict={hoveredVerdict} citationMap={citationMap} />
      {showCitations && claimActions && claimVerdicts[claimActions.index] && (
        <ClaimActionsPopover
          anchor={claimActions.rect}
          verdict={claimVerdicts[claimActions.index]!}
          citationMap={citationMap}
          conversationId={conversationId ?? null}
          maxRight={claimActions.maxRight}
          onClose={() => setClaimActions(null)}
        />
      )}
    </>
  );
});
