import { useCallback, useId, useMemo, useState } from 'react';

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

import { usePassageReferenceIds, useSettingsQuery } from '@/hooks/queries';
import { useDownloadedModels } from '@/hooks/useDownloadedModels';
import { toast } from '@/stores/toastStore';

import { CitationFootnote } from './CitationFootnote';
import { FilePreviewModal } from './FilePreviewModal';
import { MessageActions } from './MessageActions';
import { MessageEditor } from './MessageEditor';
import { ActivityNote } from './ActivityNote';
import { RetrievalTrace } from './RetrievalTrace';
import { SourceCitations } from './SourceCitations';
import { provenanceLabel, sourceProvenance } from './sourceProvenance';
import { verificationSummaryLine } from './verificationSummary';
import { useConversationsStore } from '../../stores/conversationsStore';
import { normalizeAssistantMarkdown } from '../../utils/assistantMarkdown';
import { createCitationMap } from '../../utils/citations';
import { createDefaultConversationTitle } from '../../utils/conversationTitles';
import { locatorFromSource, rememberLocation } from '../Reading/passageLocator';
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
  const [previewIndex, setPreviewIndex] = useState<number | null>(null);
  const [resolvedLocations, setResolvedLocations] = useState<Map<string, string>>(
    () => new Map()
  );
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
  const liveActivity = useConversationsStore((s) => s.liveActivity);
  const inFlightGenerations = useConversationsStore((s) => s.inFlightGenerations);
  const regenerateResponse = useConversationsStore((s) => s.regenerateResponse);
  const truncateAfter = useConversationsStore((s) => s.truncateAfter);
  const forkConversation = useConversationsStore((s) => s.forkConversation);
  const sendMessage = useConversationsStore((s) => s.sendMessage);
  const createConversation = useConversationsStore((s) => s.createConversation);

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
      const branchId = previousMessageId
        ? await forkConversation(conversationId, previousMessageId)
        : await createConversation(createDefaultConversationTitle());
      if (!branchId) return;
      await sendMessage(value, branchId);
      toast.success('Branched', { message: "You're in the new conversation." });
    },
    [conversationId, previousMessageId, forkConversation, createConversation, sendMessage]
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
            onViewFile={() => setPreviewIndex(position)}
          />
        ))}
      </div>
    );
  }, [isAssistantWithSources, citationMap, provenanceBySource, resolvedLocations]);

  const previewSource =
    previewIndex !== null ? citationSources[previewIndex] ?? null : null;
  const previewLocator = useMemo(
    () => (previewSource ? locatorFromSource(previewSource, previewSource.chunkId) : null),
    [previewSource]
  );

  const handleLocationResolved = useCallback((chunkId: string, label: string) => {
    // Remembered globally so every citation row for this chunk shows the page,
    // and locally so this message re-renders with it now.
    rememberLocation(chunkId, label);
    setResolvedLocations((current) => new Map(current).set(chunkId, label));
  }, []);

  const timestamp = new Date(message.createdAt).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
  });

  return (
    <>
      <article
        id={domMessageId}
        className={`group border-t border-subtle px-6 py-8${isFresh ? ' animate-in fade-in-0 duration-fast ease-out' : ''}`}
      >
        {/* Header row: label + metadata */}
        <header className="mb-2 flex items-baseline justify-between gap-3">
          <div className="flex min-w-0 items-baseline gap-2">
            <span className="text-xxs uppercase tracking-[0.08em] text-[hsl(var(--text-tertiary))] font-medium">
              {isUser ? 'You' : 'Assistant'}
            </span>
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
          <time className="shrink-0 text-xs text-[hsl(var(--text-muted))]">
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

        <RetrievalTrace trace={retrievalTrace} />
        {!isUser && isPending && (
          <ActivityNote detail={conversationId ? liveActivity.get(conversationId) ?? null : null} />
        )}

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
          className={`max-w-none break-words text-base [overflow-wrap:anywhere] ${
            isUser ? 'font-sans leading-[1.5]' : 'font-serif leading-[1.65]'
          }`}
        >
          <TiptapViewer content={normalizedMarkdownContent} />
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

        {citationFootnotes}

        {/* Source citations (assistant-only) */}
        {isAssistantWithSources && (
          <SourceCitations
            sources={sources}
            provenanceBySource={provenanceBySource}
            resolvedLocations={resolvedLocations}
            onViewSource={(source) =>
              setPreviewIndex(
                Math.max(
                  0,
                  citationSources.findIndex((candidate) => candidate.chunkId === source.chunkId)
                )
              )
            }
          />
        )}

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
        />
      </article>

      <FilePreviewModal
        presentation="reading-pane"
        isOpen={previewIndex !== null && previewSource !== null}
        onClose={() => setPreviewIndex(null)}
        source={previewSource}
        initialLocator={previewLocator}
        citations={citationSources}
        citationIndex={previewIndex ?? undefined}
        onCitationIndexChange={setPreviewIndex}
        onLocationResolved={handleLocationResolved}
      />
    </>
  );
}
