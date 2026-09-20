import { useCallback, useEffect, useId, useMemo, useRef, useState, type MouseEvent as ReactMouseEvent } from 'react';

import {
  AlertCircle,
  Bookmark,
  ChevronDown,
  ChevronRight,
  Loader2,
  ShieldAlert,
  ShieldCheck,
  ShieldOff,
} from 'lucide-react';
import { useNavigate } from 'react-router';

import { usePassageReferenceIds, useSettingsQuery } from '@/hooks/queries';
import { useDownloadedModels } from '@/hooks/useDownloadedModels';
import { toast } from '@/stores/toastStore';

import { ClaimActionsPopover } from './actions/ClaimActionsPopover';
import { CitationFootnote } from './CitationFootnote';
import { CitationHoverCard, type CitationHover } from './CitationHoverCard';
import { ClaimHoverCard, type ClaimHover } from './ClaimHoverCard';
import { EvidenceMargin } from './EvidenceMargin';
import { MessageActions } from './MessageActions';
import { MessageEditor } from './MessageEditor';
import { SourceCitations } from './SourceCitations';
import { provenanceLabel, sourceProvenance } from './sourceProvenance';
import { TurnRecord } from './turn/TurnRecord';
import { verificationSummaryLine } from './verificationSummary';
import { useChatReaderStore } from '../../stores/chatReaderStore';
import { useConversationsStore } from '../../stores/conversationsStore';
import { normalizeAssistantMarkdown } from '../../utils/assistantMarkdown';
import { createCitationMap } from '../../utils/citations';
import { createDefaultConversationTitle } from '../../utils/conversationTitles';
import { TiptapViewer } from '../TiptapEditor';

import type { GenerationOutcome } from '../../stores/conversationsStore.types';
import type { DisplayMessage, SourceWithMetadata } from '../../types/conversation';

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

export function Message({
  message,
  isFresh = false,
  isLastTurn = false,
  previousMessageId,
}: MessageProps) {
  const navigate = useNavigate();
  const [isEditing, setIsEditing] = useState(false);
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
  const lastMessageSources = useConversationsStore((s) => s.lastMessageSources);
  const messageRetrieval = useConversationsStore((s) => s.messageRetrieval);
  const liveRetrieval = useConversationsStore((s) => s.liveRetrieval);
  const liveSteps = useConversationsStore((s) => s.liveSteps);
  const messageTurn = useConversationsStore((s) => s.messageTurn);
  const inFlightGenerations = useConversationsStore((s) => s.inFlightGenerations);
  const regenerateResponse = useConversationsStore((s) => s.regenerateResponse);
  const truncateAfter = useConversationsStore((s) => s.truncateAfter);
  const forkConversation = useConversationsStore((s) => s.forkConversation);
  const sendMessage = useConversationsStore((s) => s.sendMessage);
  const createConversation = useConversationsStore((s) => s.createConversation);

  // One reader serves the whole thread. A message hands it a citation to show
  // and reads back where the viewer found it.
  const openReader = useChatReaderStore((s) => s.open);
  const readerSession = useChatReaderStore((s) => s.session);
  const resolvedLocations = useChatReaderStore((s) => s.resolvedLocations);

  const { activeModel, setActiveChatModel } = useDownloadedModels();
  const settings = useSettingsQuery().data;
  const referenceKeys = usePassageReferenceIds();

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

  const citationMap = useMemo(() => createCitationMap(sources), [sources]);
  const isAssistantWithSources = !isUser && sources.length > 0;

  // The ordered list behind the citation numbers — what `[` / `]` travel over.
  const citationSources = useMemo(
    () =>
      Array.from(citationMap.entries())
        .sort(([a], [b]) => a - b)
        .map(([, source]) => source),
    [citationMap]
  );

  /** What the reader calls this message when it holds its citations. */
  const ownerKey = messageId ?? domMessageId ?? '';

  const openCitation = useCallback(
    (index: number) => {
      if (index < 0 || !ownerKey) return;
      openReader(ownerKey, citationSources, index);
    },
    [ownerKey, openReader, citationSources]
  );

  /** Open the reader on a source by identity, whatever its position. */
  const openSource = useCallback(
    (source: SourceWithMetadata) => {
      const index = citationSources.findIndex((candidate) => candidate.chunkId === source.chunkId);
      // A passage the answer never numbered — two chunks that share a citation
      // id keep one of them out of the map — still opens, on its own.
      if (index < 0) {
        if (ownerKey) openReader(ownerKey, [source], 0);
        return;
      }
      openCitation(index);
    },
    [citationSources, openCitation, openReader, ownerKey]
  );

  const vaultPath = settings?.vault?.vaultPath ?? '';
  const provenanceBySource = useMemo(() => {
    const map = new Map<string, string>();
    for (const source of sources) {
      const label = provenanceLabel(sourceProvenance(source, vaultPath, referenceKeys));
      if (label) map.set(source.chunkId, label);
    }
    return map;
  }, [sources, vaultPath, referenceKeys]);

  const conversationId = 'conversationId' in message ? message.conversationId : undefined;
  const conversationTitle = useConversationsStore(
    (s) => s.conversations.find((c) => c.id === conversationId)?.title ?? null,
  );
  const conversationSpaceId = useConversationsStore(
    (s) => s.conversations.find((c) => c.id === conversationId)?.spaceId ?? null,
  );
  const isBusy = Boolean(conversationId && inFlightGenerations.has(conversationId));

  // In flight, the live event stream is the trace; once persisted, the message
  // metadata is. Never both, and never a stale one.
  const retrievalTrace = useMemo(() => {
    if (isUser) return null;
    if (isPending && conversationId) return liveRetrieval.get(conversationId) ?? null;
    return messageId ? messageRetrieval.get(messageId) ?? null : null;
  }, [isUser, isPending, conversationId, liveRetrieval, messageId, messageRetrieval]);

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
  const unverifiedClaims = useMemo(() => {
    if (contradictedCount === 0) return unsupportedClaims;
    const contradicted = new Set(contradictedClaims);
    return unsupportedClaims.filter((claim) => !contradicted.has(claim));
  }, [contradictedClaims, contradictedCount, unsupportedClaims]);
  /** The passage the judge read, keyed by the claim it ruled on. */
  const evidenceByClaim = useMemo(() => {
    const quotes = new Map<string, string>();
    for (const verdict of verificationSummary?.claimVerdicts ?? []) {
      if (verdict.evidenceQuote) quotes.set(verdict.sentence, verdict.evidenceQuote);
    }
    return quotes;
  }, [verificationSummary]);
  const canExpandVerification =
    verificationSummary?.enabled === true && claimsEvaluated > 0;
  // Drawn on the sentences themselves, so a doubtful figure is doubtful where
  // it is read and not in a list under a badge.
  const claimVerdicts = useMemo(
    () => (verificationSummary?.enabled ? verificationSummary.claimVerdicts ?? [] : []),
    [verificationSummary]
  );

  const visibleVerifiedClaims = showAllVerifiedClaims
    ? supportedClaims
    : supportedClaims.slice(0, 5);
  const visibleUnverifiedClaims = showAllUnverifiedClaims
    ? unverifiedClaims
    : unverifiedClaims.slice(0, 5);

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
    if (claimsEvaluated === 0) {
      return {
        label: 'Nothing to verify',
        icon: ShieldOff,
        className:
          'border-subtle bg-surface text-[hsl(var(--text-muted))]',
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
  }, [claimsEvaluated, isUser, verificationSummary, unsupportedCount, contradictedCount]);

  const normalizedMarkdownContent = useMemo(
    () => (isUser ? message.content : normalizeAssistantMarkdown(message.content)),
    [isUser, message.content]
  );

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
    await bookmarkMessage(activeConversationId, messageId, title || null, null);
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

  const citationFootnotes = useMemo(() => {
    if (!isAssistantWithSources || citationMap.size === 0) return null;
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
  }, [isAssistantWithSources, citationMap, provenanceBySource, resolvedLocations, openCitation]);

  // Inline `[n]` chips live inside the rendered answer, so they are handled by
  // delegation: one listener on the body, the chip found by its data attribute.
  const [citationHover, setCitationHover] = useState<CitationHover | null>(null);
  const citationNumbers = useMemo(
    () => (isAssistantWithSources ? Array.from(citationMap.keys()) : []),
    [isAssistantWithSources, citationMap]
  );

  const chipFromEvent = (event: ReactMouseEvent<HTMLElement>): HTMLElement | null =>
    event.target instanceof Element ? event.target.closest<HTMLElement>('[data-cite]') : null;

  const articleRef = useRef<HTMLElement>(null);
  const [claimHover, setClaimHover] = useState<ClaimHover | null>(null);
  const claimTimerRef = useRef<number | null>(null);
  const cancelPendingClaim = () => {
    if (claimTimerRef.current !== null) window.clearTimeout(claimTimerRef.current);
    claimTimerRef.current = null;
  };
  useEffect(() => cancelPendingClaim, []);

  /** True where the thread is wide enough that the evidence sits beside the answer. */
  const hasEvidenceMargin = () =>
    Boolean(articleRef.current?.querySelector<HTMLElement>('.evidence-margin')?.offsetParent);

  /**
   * The citation the reader is showing, if it is showing one of ours.
   *
   * Only the answer the reader was opened from lights up: the same number in
   * the answer above means a different passage.
   */
  const readerCitationNumber = useMemo(() => {
    if (!readerSession || !ownerKey || readerSession.ownerKey !== ownerKey) return null;
    const shown = readerSession.citations[readerSession.index];
    if (!shown) return null;
    for (const [number, source] of citationMap) {
      if (source.chunkId === shown.chunkId) return number;
    }
    return null;
  }, [readerSession, ownerKey, citationMap]);

  /** What stays lit when nothing is under the pointer. */
  const restingLitRef = useRef<string | null>(null);

  /** Light every piece of one citation or one sentence; decorations split at each mark. */
  const setLit = useCallback((selector: string | null) => {
    const article = articleRef.current;
    if (!article) return;
    // Nothing hovered falls back to what the reader is showing, so moving the
    // pointer away does not put the open citation out.
    const next = selector ?? restingLitRef.current;
    article.querySelectorAll('.is-lit').forEach((element) => element.classList.remove('is-lit'));
    if (next) article.querySelectorAll(next).forEach((element) => element.classList.add('is-lit'));
  }, []);

  // The open citation is lit in the text as long as the reader shows it.
  useEffect(() => {
    restingLitRef.current =
      readerCitationNumber === null ? null : `[data-cite="${readerCitationNumber}"]`;
    setLit(null);
  }, [readerCitationNumber, setLit]);

  // Resting on a sentence says why it is trusted; clicking it is how the
  // sentence leaves the chat.
  const [claimActions, setClaimActions] = useState<{ index: number; rect: DOMRect; maxRight: number } | null>(null);

  const handleBodyClick = (event: ReactMouseEvent<HTMLElement>) => {
    const chip = chipFromEvent(event);
    if (chip) {
      const source = citationMap.get(Number(chip.dataset.cite));
      if (!source) return;
      event.preventDefault();
      setCitationHover(null);
      openSource(source);
      return;
    }

    // A click that ends a text selection is a selection, not a request.
    if (window.getSelection()?.toString()) return;
    const claim =
      event.target instanceof Element ? event.target.closest<HTMLElement>('[data-claim]') : null;
    if (!claim) return;
    const index = Number(claim.dataset.claim);
    if (!claimVerdicts[index]) return;
    cancelPendingClaim();
    setClaimHover(null);
    const lines = Array.from(claim.getClientRects());
    const rect =
      lines.find((line) => event.clientY >= line.top && event.clientY <= line.bottom) ??
      claim.getBoundingClientRect();
    setClaimActions({ index, rect, maxRight: event.currentTarget.getBoundingClientRect().right });
  };

  const handleBodyMouseOver = (event: ReactMouseEvent<HTMLElement>) => {
    const chip = chipFromEvent(event);
    if (chip) {
      cancelPendingClaim();
      if (claimHover) setClaimHover(null);
      setLit(null);
      const number = Number(chip.dataset.cite);
      if (citationHover?.number === number) return;
      setCitationHover({ number, rect: chip.getBoundingClientRect() });
      return;
    }
    if (citationHover) setCitationHover(null);

    const claim =
      event.target instanceof Element ? event.target.closest<HTMLElement>('[data-claim]') : null;
    if (!claim) {
      cancelPendingClaim();
      if (claimHover) setClaimHover(null);
      setLit(null);
      return;
    }
    const index = Number(claim.dataset.claim);
    if (claimHover?.index === index) return;
    setLit(`[data-claim="${index}"]`);
    // The line under the pointer, not the box around a sentence that wraps.
    const lines = Array.from(claim.getClientRects());
    const rect =
      lines.find((line) => event.clientY >= line.top && event.clientY <= line.bottom) ??
      claim.getBoundingClientRect();
    const next: ClaimHover = { index, rect, maxRight: event.currentTarget.getBoundingClientRect().right };
    cancelPendingClaim();
    // Sweeping the pointer across a paragraph should not flash a card per
    // sentence; once one is open, the next opens at once.
    if (claimHover) {
      setClaimHover(next);
      return;
    }
    claimTimerRef.current = window.setTimeout(() => setClaimHover(next), 280);
  };

  const handleBodyMouseLeave = () => {
    cancelPendingClaim();
    setCitationHover(null);
    setClaimHover(null);
    setLit(null);
  };

  const hoveredSource = citationHover ? citationMap.get(citationHover.number) ?? null : null;
  const hoveredVerdict = claimHover ? claimVerdicts[claimHover.index] ?? null : null;
  // What the margin lights: the citation under the pointer, or every citation
  // of the sentence under it.
  const activeEvidence = useMemo(
    () => (citationHover ? [citationHover.number] : hoveredVerdict?.citationIds ?? []),
    [citationHover, hoveredVerdict]
  );

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
            {verificationBadge && (
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
        {!isUser && verificationSummary && verificationSummary.enabled && claimsEvaluated > 0 && isVerificationPanelExpanded && (
          <div id={verificationPanelId} role="region" aria-label="Verification details" className="mb-4 rounded-sm border border-subtle bg-surface p-4">
            <p className="mb-1 text-sm font-medium text-text-primary">Verification details</p>
            <p className="text-xs text-[hsl(var(--text-muted))]">
              {verificationSummaryLine(claimsEvaluated, unsupportedCount)}
            </p>

            <div className="mt-3 grid gap-4 lg:grid-cols-2">
              {contradictedClaims.length > 0 && (
                <div>
                  <p className="mb-2 text-xs font-medium text-[hsl(var(--danger-fg))]">
                    Contradicted by your sources
                  </p>
                  <ul className="space-y-2">
                    {contradictedClaims.map((claim, idx) => (
                      <li
                        key={`contradicted-${idx}`}
                        className="text-sm leading-relaxed text-[hsl(var(--text-secondary))]"
                      >
                        {claim}
                        {/* The passage the judge read. Without it the verdict is
                            an assertion; with it the reader can check. */}
                        {evidenceByClaim.get(claim) && (
                          <span className="mt-1 block border-l-2 border-[hsl(var(--danger-muted))] pl-2 text-xs italic text-[hsl(var(--text-muted))]">
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
                  <ul className="space-y-2">
                    {visibleVerifiedClaims.map((claim, idx) => (
                      <li
                        key={`verified-${idx}`}
                        className="text-sm leading-relaxed text-[hsl(var(--text-secondary))]"
                      >
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

              {unverifiedClaims.length > 0 && (
                <div>
                  <p className="mb-2 text-xs font-medium text-[hsl(var(--text-secondary))]">
                    Unverified claims
                  </p>
                  <ul className="space-y-2">
                    {visibleUnverifiedClaims.map((claim, idx) => (
                      <li
                        key={`unsupported-${idx}`}
                        className="text-sm leading-relaxed text-[hsl(var(--text-secondary))]"
                      >
                        {claim}
                      </li>
                    ))}
                  </ul>
                  {unverifiedClaims.length > 5 && (
                    <button
                      type="button"
                      onClick={() => setShowAllUnverifiedClaims((prev) => !prev)}
                      className="mt-2 text-xs text-[hsl(var(--accent))] underline-offset-2 hover:underline"
                    >
                      {showAllUnverifiedClaims
                        ? 'Show fewer'
                        : `Show all ${unverifiedClaims.length}`}
                    </button>
                  )}
                </div>
              )}
            </div>
          </div>
        )}

        <TurnRecord
          trace={retrievalTrace}
          record={messageId ? messageTurn.get(messageId) ?? null : null}
          liveSteps={isPending && conversationId ? liveSteps.get(conversationId) ?? null : null}
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
          onClick={isUser ? undefined : handleBodyClick}
          onMouseOver={isUser ? undefined : handleBodyMouseOver}
          onMouseLeave={isUser ? undefined : handleBodyMouseLeave}
          className={`break-words [overflow-wrap:anywhere] ${
            isUser
              ? 'ml-auto w-fit max-w-[85%] rounded-2xl rounded-br-md bg-surface px-4 py-2.5 font-sans text-[15px] leading-[1.55] shadow-sheet'
              : 'chat-answer max-w-none font-serif text-[16.5px] leading-[1.7]'
          }`}
        >
          <TiptapViewer
            content={normalizedMarkdownContent}
            citationNumbers={citationNumbers}
            claims={isUser ? undefined : claimVerdicts}
          />
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

        {/* The narrow form of the evidence; the margin replaces it where there is room. */}
        <div className="evidence-inline">
        {citationFootnotes}

        {/* Source citations (assistant-only) */}
        {isAssistantWithSources && (
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
          answerSources={isAssistantWithSources ? sources : undefined}
          answerMarkdown={isUser ? undefined : normalizedMarkdownContent}
          answerVerification={verificationSummary ?? null}
          conversationTitle={conversationTitle}
          answerTurn={messageId ? messageTurn.get(messageId) : undefined}
        />
        </div>
        {isAssistantWithSources && (
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
        hover={citationHover && !hasEvidenceMargin() ? citationHover : null}
        source={hoveredSource}
        location={hoveredSource ? resolvedLocations.get(hoveredSource.chunkId) ?? null : null}
        provenanceLabel={hoveredSource ? provenanceBySource.get(hoveredSource.chunkId) : undefined}
      />
      <ClaimHoverCard hover={claimHover} verdict={hoveredVerdict} citationMap={citationMap} />
      {claimActions && claimVerdicts[claimActions.index] && (
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
}
