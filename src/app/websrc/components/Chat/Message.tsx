import { useMemo, useState } from 'react';

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

import { CitationFootnote } from './CitationFootnote';
import { FilePreviewModal } from './FilePreviewModal';
import { MessageActions } from './MessageActions';
import { SourceCitations } from './SourceCitations';
import { useConversationsStore } from '../../stores/conversationsStore';
import { normalizeAssistantMarkdown } from '../../utils/assistantMarkdown';
import { createCitationMap } from '../../utils/citations';
import { TiptapViewer } from '../TiptapEditor';

import type { DisplayMessage, SourceWithMetadata } from '../../types/conversation';

interface MessageProps {
  message: DisplayMessage;
  isFresh?: boolean;
}

export function Message({ message, isFresh = false }: MessageProps) {
  const [previewSource, setPreviewSource] = useState<SourceWithMetadata | null>(null);
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

  const claimsEvaluated = verificationSummary?.claimsEvaluated ?? 0;
  const supportedCountRaw = verificationSummary?.supportedClaims ?? 0;
  const supportedClaims = verificationSummary?.supportedClaimNotes ?? [];
  const unsupportedClaims = verificationSummary?.unsupportedClaims ?? [];
  const unsupportedCount = unsupportedClaims.length;
  const supportedCount = Math.max(
    supportedCountRaw,
    Math.max(0, claimsEvaluated - unsupportedCount)
  );
  const groundedRatio =
    verificationSummary?.groundedRatio ??
    (claimsEvaluated > 0 ? supportedCount / claimsEvaluated : 1);
  const canExpandVerification =
    verificationSummary?.enabled === true && claimsEvaluated > 0;

  const visibleVerifiedClaims = showAllVerifiedClaims
    ? supportedClaims
    : supportedClaims.slice(0, 5);
  const visibleUnverifiedClaims = showAllUnverifiedClaims
    ? unsupportedClaims
    : unsupportedClaims.slice(0, 5);

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
    return {
      label: `Partially verified · ${unsupportedCount}`,
      icon: ShieldAlert,
      className:
        'border-[hsl(var(--warning-muted))] bg-[hsl(var(--warning-muted))] text-[hsl(var(--warning-fg))]',
    };
  }, [claimsEvaluated, isUser, verificationSummary, unsupportedCount]);

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
  };

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
      <div className="mt-2 flex flex-wrap gap-1">
        {entries.map(([num, source]) => (
          <CitationFootnote
            key={`cite-${num}`}
            number={num}
            source={source}
            onViewFile={() => setPreviewSource(source)}
          />
        ))}
      </div>
    );
  }, [isAssistantWithSources, citationMap]);

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
                className={`inline-flex items-center gap-1 rounded-sm border px-1.5 py-0.5 text-xxs uppercase tracking-[0.04em] ${verificationBadge.className} ${
                  canExpandVerification ? 'cursor-pointer' : 'cursor-default'
                }`}
                aria-expanded={canExpandVerification ? isVerificationPanelExpanded : undefined}
                title={
                  !verificationSummary?.enabled
                    ? 'Verification is off. Turn on in settings.'
                    : canExpandVerification
                      ? 'Show verification details'
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

        {/* Body prose — tiptap.css styles inherit `font-family` from this wrapper. */}
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

        {citationFootnotes}

        {/* Verification details panel */}
        {!isUser && verificationSummary && verificationSummary.enabled && claimsEvaluated > 0 && isVerificationPanelExpanded && (
          <div className="mt-4 rounded-md border border-subtle bg-surface p-4">
            <div className="mb-3 flex flex-wrap items-center gap-2 text-xs">
              <span className="inline-flex items-center gap-1 rounded-sm border border-[hsl(var(--success-muted))] bg-[hsl(var(--success-muted))] px-2 py-0.5 text-[hsl(var(--success-fg))]">
                <ShieldCheck className="h-3 w-3" />
                Verified: {supportedCount}
              </span>
              <span className="inline-flex items-center gap-1 rounded-sm border border-[hsl(var(--warning-muted))] bg-[hsl(var(--warning-muted))] px-2 py-0.5 text-[hsl(var(--warning-fg))]">
                <ShieldAlert className="h-3 w-3" />
                Unverified: {unsupportedCount}
              </span>
              <span className="text-[hsl(var(--text-muted))]">
                {(groundedRatio * 100).toFixed(0)}% coverage · {claimsEvaluated} claim{claimsEvaluated !== 1 ? 's' : ''}
              </span>
            </div>

            <div className="grid gap-3 lg:grid-cols-2">
              <div>
                <p className="mb-2 text-xxs uppercase tracking-[0.08em] text-[hsl(var(--success-fg))]">
                  Verified claims
                </p>
                {supportedClaims.length > 0 ? (
                  <>
                    <ul className="space-y-1.5">
                      {visibleVerifiedClaims.map((claim, idx) => (
                        <li
                          key={`verified-${idx}`}
                          className="rounded-sm border border-subtle bg-surface-raised px-3 py-1.5 text-sm text-[hsl(var(--text-secondary))]"
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
                  </>
                ) : (
                  <p className="text-xs text-[hsl(var(--text-muted))]">
                    No verified claim text available.
                  </p>
                )}
              </div>

              <div>
                <p className="mb-2 text-xxs uppercase tracking-[0.08em] text-[hsl(var(--warning-fg))]">
                  Unverified claims
                </p>
                {unsupportedClaims.length > 0 ? (
                  <>
                    <ul className="space-y-1.5">
                      {visibleUnverifiedClaims.map((claim, idx) => (
                        <li
                          key={`unsupported-${idx}`}
                          className="rounded-sm border border-subtle bg-surface-raised px-3 py-1.5 text-sm text-[hsl(var(--text-secondary))]"
                        >
                          {claim}
                        </li>
                      ))}
                    </ul>
                    {unsupportedClaims.length > 5 && (
                      <button
                        type="button"
                        onClick={() => setShowAllUnverifiedClaims((prev) => !prev)}
                        className="mt-2 text-xs text-[hsl(var(--accent))] underline-offset-2 hover:underline"
                      >
                        {showAllUnverifiedClaims
                          ? 'Show fewer'
                          : `Show all ${unsupportedClaims.length}`}
                      </button>
                    )}
                  </>
                ) : (
                  <p className="text-xs text-[hsl(var(--text-muted))]">
                    Every claim is grounded.
                  </p>
                )}
              </div>
            </div>
          </div>
        )}

        {/* Source citations (assistant-only) */}
        {isAssistantWithSources && (
          <SourceCitations
            sources={sources}
            onViewSource={(source) => setPreviewSource(source)}
          />
        )}

        {/* Inline actions */}
        <MessageActions
          canBookmark={canBookmarkMessage}
          canDelete={canDeleteMessage}
          isBookmarked={isMessageBookmarked}
          onCopy={handleCopy}
          onBookmarkToggle={handleBookmarkToggle}
          onDelete={handleDeleteMessage}
        />
      </article>

      <FilePreviewModal
        isOpen={previewSource !== null}
        onClose={() => setPreviewSource(null)}
        source={previewSource}
      />
    </>
  );
}
