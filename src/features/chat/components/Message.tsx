import { memo, useCallback, useEffect, useId, useMemo, useRef, useState } from 'react';

import {
  AlertCircle,
  Bookmark,
  ChevronDown,
  ChevronRight,
  Loader2,
  MessageSquareShare,
  Paperclip,
  RefreshCw,
  ShieldAlert,
  ShieldCheck,
  ShieldEllipsis,
  ShieldOff,
} from 'lucide-react';
import { useNavigate } from 'react-router';

import { TiptapViewer } from '@/components/TiptapEditor';
import { ClaimActionsPopover } from '@/features/chat/components/actions/ClaimActionsPopover';
import { CitationFootnote } from '@/features/chat/components/CitationFootnote';
import { CitationHoverCard } from '@/features/chat/components/CitationHoverCard';
import { ClaimHoverCard } from '@/features/chat/components/ClaimHoverCard';
import { EvidenceMargin } from '@/features/chat/components/EvidenceMargin';
import { MessageActions } from '@/features/chat/components/MessageActions';
import { MessageEditor } from '@/features/chat/components/MessageEditor';
import { TangentSelection } from '@/features/chat/components/tangents/TangentSelection';
import { TurnRecord } from '@/features/chat/components/turn/TurnRecord';
import { useMessageEvidence } from '@/features/chat/hooks/useMessageEvidence';
import { useRevealStreamingRef } from '@/features/chat/hooks/useRevealStreamingRef';
import { openInFolder, revealInFolder } from '@/features/chat/model/folderThreadHost';
import { verificationSummaryLine } from '@/features/chat/model/verificationSummary';
import { useDownloadedModels } from '@/features/model/hooks/useDownloadedModels';
import { SourceCitations } from '@/features/reading/components/SourceCitations';
import { useChatReaderStore } from '@/features/reading/stores/chatReaderStore';
import { useCitationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import type { GenerationOutcome } from '@/shared/conversations/conversationsStore.types';
import { toast } from '@/stores/toastStore';
import type { DisplayMessage, SourceWithMetadata } from '@/types/conversation';
import { normalizeAssistantMarkdown } from '@/utils/assistantMarkdown';
import { createDefaultConversationTitle } from '@/utils/conversationTitles';

/**
 * One entry per checked sentence, separated by a rule.
 *
 * Claims are printed as the model wrote them, citation markers included, so an
 * entry routinely ends in `[24]`. Spacing alone left that marker sitting
 * directly above the next claim's first word, where it read as that claim's
 * citation — and an uncited sentence looked cited.
 */
const CLAIM_LIST_CLASS = 'divide-y divide-border-subtle';
const CLAIM_ITEM_CLASS =
  'py-2 first:pt-0 last:pb-0 text-sm leading-relaxed text-[hsl(var(--text-secondary))]';

/**
 * What to say about a regenerate that failed.
 *
 * A cancelled turn is not a failure and never reaches here — only `'failed'`
 * hands the question back through the composer, so only `'failed'` may say so.
 */
function regenerateFailureMessage(
  outcome: Exclude<GenerationOutcome, 'answered' | 'cancelled'>
): string {
  if (outcome === 'busy') return 'This conversation is still answering.';
  return 'Your question is back in the composer.';
}

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
  const [isEditing, setIsEditing] = useState(false);
  const answerRef = useRef<HTMLDivElement>(null);
  const verificationPanelId = useId();
  const [isVerificationPanelExpanded, setIsVerificationPanelExpanded] = useState(false);
  const [showAllVerifiedClaims, setShowAllVerifiedClaims] = useState(false);
  const [showAllUnverifiedClaims, setShowAllUnverifiedClaims] = useState(false);

  const isUser = message.role === 'user';
  const isPending = message.status === 'pending';
  const isFailed = message.status === 'failed';
  const errorMessage = 'error' in message ? message.error : undefined;

  const activeConversationId = useConversationsStore((s) => s.activeConversationId);
  const messageBookmarkMap = useConversationsStore((s) => s.messageBookmarkMap);
  const messageVerificationMap = useConversationsStore((s) => s.messageVerification);
  const bookmarkMessage = useConversationsStore((s) => s.bookmarkMessage);
  const unbookmarkMessage = useConversationsStore((s) => s.unbookmarkMessage);
  const deleteMessage = useConversationsStore((s) => s.deleteMessage);
  const dismissFailedMessage = useConversationsStore((s) => s.dismissFailedMessage);
  const retryFailedMessage = useConversationsStore((s) => s.retryFailedMessage);
  const lastMessageSources = useConversationsStore((s) => s.lastMessageSources);
  const messageRetrieval = useConversationsStore((s) => s.messageRetrieval);
  const liveRetrieval = useConversationsStore((s) => s.liveRetrieval);
  const liveSteps = useConversationsStore((s) => s.liveSteps);
  const messageTurn = useConversationsStore((s) => s.messageTurn);
  const inFlightGenerations = useConversationsStore((s) => s.inFlightGenerations);
  const regenerateResponse = useConversationsStore((s) => s.regenerateResponse);
  const truncateAfter = useConversationsStore((s) => s.truncateAfter);
  const forkConversation = useConversationsStore((s) => s.forkConversation);
  const selectConversation = useConversationsStore((s) => s.selectConversation);
  const sendMessage = useConversationsStore((s) => s.sendMessage);
  const createConversation = useConversationsStore((s) => s.createConversation);

  // One reader serves the whole thread. A message hands it a citation to show
  // and reads back where the viewer found it.
  const resolvedLocations = useChatReaderStore((s) => s.resolvedLocations);
  const showCitations = useCitationDisplayStore((s) => s.visible);

  const { activeModel, setActiveChatModel } = useDownloadedModels();

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

  // The files this message brought into the conversation, stamped on it when
  // it was sent. Absent metadata or a parse failure is "no attachments",
  // never an error — older messages simply have no record.
  const attachmentNames = useMemo(() => {
    if (!isUser || !('metadata' in message) || !message.metadata) return [] as string[];
    try {
      const parsed: unknown = JSON.parse(message.metadata);
      const names = (parsed as { attachments?: unknown })?.attachments;
      if (!Array.isArray(names)) return [] as string[];
      return names.filter((name): name is string => typeof name === 'string');
    } catch {
      return [] as string[];
    }
  }, [isUser, message]);

  // A chat continued from another opens with a summary of it. That turn was
  // written from the old thread, not in answer to anything here, so it says
  // where it came from and links back.
  const continuedFrom = useMemo(() => {
    if (isUser || !('metadata' in message) || !message.metadata) return null;
    try {
      const parsed = (JSON.parse(message.metadata) as { continuedFrom?: { conversationId?: unknown; title?: unknown } })?.continuedFrom;
      if (typeof parsed?.conversationId !== 'string') return null;
      return { conversationId: parsed.conversationId, title: typeof parsed.title === 'string' ? parsed.title : 'the earlier chat' };
    } catch {
      return null;
    }
  }, [isUser, message]);

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

  const claimsEvaluated = verificationSummary?.claimsEvaluated ?? 0;
  const supportedClaims = verificationSummary?.supportedClaimNotes ?? [];
  // Memoized because the `?? []` fallback is a fresh array on every render,
  // which would re-run every hook that reads it.
  const unsupportedClaims = useMemo(
    () => verificationSummary?.unsupportedClaims ?? [],
    [verificationSummary]
  );
  const unsupportedCount = unsupportedClaims.length;
  // "Contradicted" is a stronger claim than "unsupported": a passage the model
  // actually read says otherwise. The backend reports contradictions inside
  // `unsupportedClaims` too, so they are subtracted here rather than counted
  // twice. Absent on messages verified before the judge existed, which is why
  // this reads as an empty list and never as "none were contradicted".
  const contradictedClaims = useMemo(
    () => verificationSummary?.contradictedClaims ?? [],
    [verificationSummary]
  );
  const contradictedCount = contradictedClaims.length;
  const ungroundedClaims = useMemo(() => {
    if (contradictedCount === 0) return unsupportedClaims;
    const contradicted = new Set(contradictedClaims);
    return unsupportedClaims.filter((claim) => !contradicted.has(claim));
  }, [contradictedClaims, contradictedCount, unsupportedClaims]);
  // Claims nothing checked: the judge ran out of time or the page had no
  // saved text. Kept apart from the ungrounded ones — "not looked at" is not
  // "not found".
  const uncheckedClaims = useMemo(
    () => (verificationSummary?.claimVerdicts ?? [])
      .filter((verdict) => verdict.verdict === 'unverified')
      .map((verdict) => verdict.sentence),
    [verificationSummary]
  );
  const uncheckedCount = verificationSummary?.verdictCounts?.unverified ?? uncheckedClaims.length;
  const verificationPending = verificationSummary?.enabled === true && verificationSummary.pending === true;
  /** The passage the judge read, keyed by the claim it ruled on. */
  const evidenceByClaim = useMemo(() => {
    const quotes = new Map<string, string>();
    for (const verdict of verificationSummary?.claimVerdicts ?? []) {
      if (verdict.evidenceQuote) quotes.set(verdict.sentence, verdict.evidenceQuote);
    }
    return quotes;
  }, [verificationSummary]);
  const reasonByClaim = useMemo(() => {
    const reasons = new Map<string, string>();
    for (const verdict of verificationSummary?.claimVerdicts ?? []) {
      if (verdict.reason) reasons.set(verdict.sentence, verdict.reason);
    }
    return reasons;
  }, [verificationSummary]);
  const canExpandVerification =
    verificationSummary?.enabled === true && !verificationPending && claimsEvaluated > 0;
  // Drawn on the sentences themselves, so a doubtful figure is doubtful where
  // it is read and not in a list under a badge.
  const claimVerdicts = useMemo(
    () => (verificationSummary?.enabled ? verificationSummary.claimVerdicts ?? [] : []),
    [verificationSummary]
  );

  const visibleVerifiedClaims = showAllVerifiedClaims
    ? supportedClaims
    : supportedClaims.slice(0, 5);
  const visibleUngroundedClaims = showAllUnverifiedClaims
    ? ungroundedClaims
    : ungroundedClaims.slice(0, 5);

  const verificationBadge = useMemo(() => {
    if (isUser || !verificationSummary) return null;
    if (!verificationSummary.enabled) {
      return {
        label: 'Verification off',
        icon: ShieldOff,
        className:
          'border-subtle bg-surface text-[hsl(var(--text-muted))]',
      };
    }
    // The answer is readable while its check runs; say so quietly rather
    // than showing a verdict the check has not reached yet.
    if (verificationPending) {
      return {
        label: 'Checking…',
        icon: ShieldEllipsis,
        className:
          'border-subtle bg-surface text-[hsl(var(--text-muted))]',
      };
    }
    // The check never reported: the app closed under it, or the wait ran
    // out. Say that, rather than "nothing to verify", which would be a lie.
    if (verificationSummary.interrupted) {
      return {
        label: 'Not checked',
        icon: ShieldOff,
        className:
          'border-subtle bg-surface text-[hsl(var(--text-muted))]',
      };
    }
    if (claimsEvaluated === 0) {
      return {
        label: 'Nothing to verify',
        icon: ShieldOff,
        className:
          'border-subtle bg-surface text-[hsl(var(--text-muted))]',
      };
    }
    if (unsupportedCount === 0 && uncheckedCount > 0) {
      return {
        label: `Partly checked · ${uncheckedCount} not checked`,
        icon: ShieldCheck,
        className:
          'border-subtle bg-surface text-[hsl(var(--text-secondary))]',
      };
    }
    if (unsupportedCount === 0) {
      return {
        label: 'Verified',
        icon: ShieldCheck,
        className:
          'border-[hsl(var(--success-muted))] bg-[hsl(var(--success-muted))] text-[hsl(var(--success-fg))]',
      };
    }
    // A contradiction outranks a merely ungrounded claim: the sources were read
    // and they disagree. Folding it into "Partially verified" would report the
    // worse finding as the milder one.
    if (contradictedCount > 0) {
      return {
        label: `Contradicted · ${contradictedCount}`,
        icon: ShieldAlert,
        className:
          'border-[hsl(var(--danger-muted))] bg-[hsl(var(--danger-muted))] text-[hsl(var(--danger-fg))]',
      };
    }
    return {
      label: `Partially verified · ${unsupportedCount}`,
      icon: ShieldAlert,
      className:
        'border-[hsl(var(--warning-muted))] bg-[hsl(var(--warning-muted))] text-[hsl(var(--warning-fg))]',
    };
  }, [
    claimsEvaluated,
    isUser,
    verificationSummary,
    verificationPending,
    unsupportedCount,
    contradictedCount,
    uncheckedCount,
  ]);

  const normalizedMarkdownContent = useMemo(
    () => (isUser ? message.content : normalizeAssistantMarkdown(message.content)),
    [isUser, message.content]
  );
  useRevealStreamingRef(normalizedMarkdownContent, showsCodeRefs && isPending);

  const handleCopy = async () => {
    await navigator.clipboard.writeText(normalizedMarkdownContent);
  };

  const handleBookmarkToggle = async () => {
    if (!activeConversationId || !messageId) return;

    if (isMessageBookmarked) {
      await unbookmarkMessage(activeConversationId, messageId);
      return;
    }

    const compact = message.content.replace(/\s+/g, ' ').trim();
    const title = compact.length > 80 ? `${compact.slice(0, 80)}...` : compact;
    if (!(await bookmarkMessage(activeConversationId, messageId, title || null, null))) return;
    // Say where it went and what it is for; a silent save teaches nothing.
    toast.success('Saved', { message: 'Lattice will use this in future answers.' });
  };

  const handleRegenerate = useCallback(async () => {
    if (!conversationId) return;
    const outcome = await regenerateResponse(conversationId);
    if (outcome === 'answered' || outcome === 'cancelled') return;
    // Only a real failure puts the question back in the composer; saying so
    // when it did not happen would send the reader looking for it.
    toast.error("Couldn't regenerate", { message: regenerateFailureMessage(outcome) });
  }, [conversationId, regenerateResponse]);

  const handleTryWithModel = useCallback(
    async (modelId: string, modelLabel: string) => {
      if (!conversationId) return;
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
      // The switch is app-wide, so every outcome offers the way back.
      const switchBack = previousModelId
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
        : {};

      const outcome = await regenerateResponse(conversationId);
      if (outcome === 'cancelled') {
        // You stopped it. That is not an error, and it is not an answer.
        toast.info(`Switched to ${modelLabel}`, {
          message: 'You stopped the answer.',
          ...switchBack,
        });
        return;
      }
      if (outcome !== 'answered') {
        // The model did switch, but no answer came back. Saying "Answered
        // with X" here would be a claim about output that does not exist.
        toast.error("Couldn't regenerate", {
          message: regenerateFailureMessage(outcome),
          ...switchBack,
        });
        return;
      }
      toast.success(`Answered with ${modelLabel}`, switchBack);
    },
    [conversationId, activeModel, setActiveChatModel, regenerateResponse]
  );

  const handleBranch = useCallback(async () => {
    if (!conversationId || !messageId) return;
    const newId = await forkConversation(conversationId, messageId);
    if (newId) {
      toast.success('Branched', { message: "You're in the new conversation." });
    }
  }, [conversationId, messageId, forkConversation]);

  const handleEditSend = useCallback(
    async (value: string) => {
      if (!conversationId || !messageId) return;
      setIsEditing(false);
      const truncated = await truncateAfter(conversationId, messageId, true);
      if (!truncated) return;
      await sendMessage(value, conversationId);
    },
    [conversationId, messageId, truncateAfter, sendMessage]
  );

  const handleEditSendAsBranch = useCallback(
    async (value: string) => {
      if (!conversationId) return;
      setIsEditing(false);
      // No previous message means this is the first turn: a fork of nothing is
      // a new conversation, so make one rather than copying the whole thread.
      // Either way the branch stays in this conversation's space. A fork copies
      // it; a new conversation would otherwise take whatever the sidebar has
      // selected, and the branch would then search a different library.
      const branchId = previousMessageId
        ? await forkConversation(conversationId, previousMessageId)
        : await createConversation(createDefaultConversationTitle(), conversationSpaceId);
      if (!branchId) return;
      await sendMessage(value, branchId);
      toast.success('Branched', { message: "You're in the new conversation." });
    },
    [conversationId, conversationSpaceId, previousMessageId, forkConversation, createConversation, sendMessage]
  );

  const handleDeleteMessage = async () => {
    if (!messageId || !('conversationId' in message)) return;

    const confirmed = window.confirm(
      "Delete this message? This can't be undone."
    );
    if (!confirmed) return;

    await deleteMessage(message.conversationId, messageId);
  };

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
        {/* Header row: who spoke is carried by the layout; this holds state. */}
        <header className={`mb-1.5 flex items-center gap-3 ${isUser ? 'justify-end' : 'justify-between'}`}>
          <div className="flex min-w-0 items-center gap-2">
            <span className="sr-only">{isUser ? 'You' : 'Assistant'}</span>
            {continuedFrom && (
              <button
                type="button"
                onClick={() => { void selectConversation(continuedFrom.conversationId); }}
                title={`Open "${continuedFrom.title}"`}
                className="inline-flex min-w-0 items-center gap-1 rounded-sm text-xs text-[hsl(var(--text-muted))] transition-colors duration-fast hover:text-accent focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
              >
                <MessageSquareShare className="h-3 w-3 shrink-0" strokeWidth={1.6} aria-hidden="true" />
                <span className="truncate">Summary of {continuedFrom.title}</span>
              </button>
            )}
            {isPending && (
              <span className="inline-flex items-center gap-1 text-xs text-[hsl(var(--text-muted))]">
                <Loader2 className="h-3 w-3 animate-spin" />
                Sending
              </span>
            )}
            {isFailed && (
              <span className="inline-flex items-center gap-1 text-xs text-[hsl(var(--danger-fg))]">
                <AlertCircle className="h-3 w-3" />
                {errorMessage || "Message didn't send"}
              </span>
            )}
            {/* Only the last question: regenerate re-asks whatever came last. */}
            {isFailed && isUser && (isLastTurn || 'tempId' in message) && conversationId && (
              <button
                type="button"
                onClick={() => {
                  if ('tempId' in message) {
                    void retryFailedMessage(message.tempId);
                  }
                  else void handleRegenerate();
                }}
                disabled={isBusy}
                className="inline-flex items-center gap-1 rounded-sm px-1 text-xs text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))] focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-[hsl(var(--accent))] disabled:opacity-50"
              >
                <RefreshCw className="h-3 w-3" />
                Try again
              </button>
            )}
            {isFailed && 'tempId' in message && (
              <button
                type="button"
                onClick={() => dismissFailedMessage(message.tempId)}
                className="rounded-sm px-1 text-xs text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))] focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
              >
                Dismiss
              </button>
            )}
            {showCitations && verificationBadge && (
              <button
                type="button"
                onClick={() => {
                  if (canExpandVerification) {
                    setIsVerificationPanelExpanded((prev) => !prev);
                  }
                }}
                className={`inline-flex items-center gap-1 rounded-sm border px-1.5 py-0.5 text-xxs ${verificationBadge.className} ${
                  canExpandVerification ? 'cursor-pointer' : 'cursor-default'
                }`}
                aria-expanded={canExpandVerification ? isVerificationPanelExpanded : undefined}
                aria-controls={canExpandVerification ? verificationPanelId : undefined}
                title={
                  !verificationSummary?.enabled
                    ? 'Verification is off. Turn on in settings.'
                    : verificationPending
                      ? 'Checking the answer against its sources'
                      : canExpandVerification
                      ? (isVerificationPanelExpanded ? 'Hide verification details' : 'Show verification details')
                      : 'Nothing in this message to verify.'
                }
              >
                <verificationBadge.icon className="h-3 w-3" />
                {verificationBadge.label}
                {canExpandVerification && (
                  isVerificationPanelExpanded
                    ? <ChevronDown className="h-3 w-3" />
                    : <ChevronRight className="h-3 w-3" />
                )}
              </button>
            )}
            {isMessageBookmarked && (
              <Bookmark
                className="h-3 w-3 fill-current text-[hsl(var(--accent))]"
                aria-label="Referenced"
              />
            )}
          </div>
          <time className="shrink-0 text-[11px] tabular-nums text-[hsl(var(--text-muted))] opacity-0 transition-opacity duration-fast group-focus-within:opacity-100 group-hover:opacity-100">
            {timestamp}
          </time>
        </header>

        {/* Verification details panel */}
        {showCitations && !isUser && verificationSummary && verificationSummary.enabled && claimsEvaluated > 0 && isVerificationPanelExpanded && (
          <div id={verificationPanelId} role="region" aria-label="Verification details" className="mb-4 rounded-sm border border-subtle bg-surface p-4">
            <p className="mb-1 text-sm font-medium text-text-primary">Verification details</p>
            <p className="text-xs text-[hsl(var(--text-muted))]">
              {verificationSummaryLine(claimsEvaluated, unsupportedCount, uncheckedCount)}
            </p>

            <div className="mt-3 grid gap-4 lg:grid-cols-2">
              {contradictedClaims.length > 0 && (
                <div>
                  <p className="mb-2 text-xs font-medium text-[hsl(var(--danger-fg))]">
                    Contradicted by your sources
                  </p>
                  {/* Ruled, not just spaced. A claim ends in its own citation
                      marker, so with nothing but a gap between entries the
                      `[24]` closing one sentence reads as though it belongs to
                      the sentence printed underneath it. */}
                  <ul className={CLAIM_LIST_CLASS}>
                    {contradictedClaims.map((claim, idx) => (
                      <li key={`contradicted-${idx}`} className={CLAIM_ITEM_CLASS}>
                        {claim}
                        {reasonByClaim.get(claim) && (
                          <span className="mt-1 block text-xs leading-relaxed text-[hsl(var(--text-secondary))]">
                            {reasonByClaim.get(claim)}
                          </span>
                        )}
                        {/* The passage the judge read. Without it the verdict is
                            an assertion; with it the reader can check. */}
                        {evidenceByClaim.get(claim) && (
                          <span className="mt-1 block border-l-2 border-[hsl(var(--danger-muted))] pl-2 text-xs italic text-[hsl(var(--text-muted))]">
                            <span className="not-italic font-medium">What the source says: </span>
                            &ldquo;{evidenceByClaim.get(claim)}&rdquo;
                          </span>
                        )}
                      </li>
                    ))}
                  </ul>
                </div>
              )}

              {supportedClaims.length > 0 && (
                <div>
                  <p className="mb-2 text-xs font-medium text-[hsl(var(--text-secondary))]">
                    Verified claims
                  </p>
                  <ul className={CLAIM_LIST_CLASS}>
                    {visibleVerifiedClaims.map((claim, idx) => (
                      <li key={`verified-${idx}`} className={CLAIM_ITEM_CLASS}>
                        {claim}
                      </li>
                    ))}
                  </ul>
                  {supportedClaims.length > 5 && (
                    <button
                      type="button"
                      onClick={() => setShowAllVerifiedClaims((prev) => !prev)}
                      className="mt-2 text-xs text-[hsl(var(--accent))] underline-offset-2 hover:underline"
                    >
                      {showAllVerifiedClaims
                        ? 'Show fewer'
                        : `Show all ${supportedClaims.length}`}
                    </button>
                  )}
                </div>
              )}

              {ungroundedClaims.length > 0 && (
                <div>
                  <p className="mb-2 text-xs font-medium text-[hsl(var(--text-secondary))]">
                    Not found in your sources
                  </p>
                  <ul className={CLAIM_LIST_CLASS}>
                    {visibleUngroundedClaims.map((claim, idx) => (
                      <li key={`unsupported-${idx}`} className={CLAIM_ITEM_CLASS}>
                        {claim}
                      </li>
                    ))}
                  </ul>
                  {ungroundedClaims.length > 5 && (
                    <button
                      type="button"
                      onClick={() => setShowAllUnverifiedClaims((prev) => !prev)}
                      className="mt-2 text-xs text-[hsl(var(--accent))] underline-offset-2 hover:underline"
                    >
                      {showAllUnverifiedClaims
                        ? 'Show fewer'
                        : `Show all ${ungroundedClaims.length}`}
                    </button>
                  )}
                </div>
              )}

              {uncheckedClaims.length > 0 && (
                <div>
                  <p className="mb-2 text-xs font-medium text-[hsl(var(--text-muted))]">
                    Not checked
                  </p>
                  <ul className={CLAIM_LIST_CLASS}>
                      {uncheckedClaims.map((claim, idx) => (
                        <li key={`unchecked-${idx}`} className={CLAIM_ITEM_CLASS}>
                          {claim}
                          {reasonByClaim.get(claim) && (
                            <span className="mt-1 block text-xs leading-relaxed text-[hsl(var(--text-secondary))]">
                              {reasonByClaim.get(claim)}
                            </span>
                          )}
                        </li>
                      ))}
                  </ul>
                </div>
              )}
            </div>
          </div>
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
          activeModelId={activeModel?.model_id ?? null}
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
