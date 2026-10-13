import type { Dispatch, SetStateAction } from 'react';

import {
  AlertCircle,
  Bookmark,
  ChevronDown,
  ChevronRight,
  Loader2,
  MessageSquareShare,
  RefreshCw,
} from 'lucide-react';

import type { DisplayMessage, MessageVerificationSummary } from '@/types/conversation';

import type { useMessageVerification } from './useMessageVerification';

/** A message's header row: who spoke is carried by the layout; this holds state. */
export function MessageHeader({
  message,
  isUser,
  isPending,
  isFailed,
  errorMessage,
  isLastTurn,
  isBusy,
  conversationId,
  continuedFrom,
  selectConversation,
  retryFailedMessage,
  dismissFailedMessage,
  handleRegenerate,
  showCitations,
  verificationSummary,
  verificationPending,
  verificationBadge,
  canExpandVerification,
  isVerificationPanelExpanded,
  setIsVerificationPanelExpanded,
  verificationPanelId,
  isMessageBookmarked,
  timestamp,
}: {
  message: DisplayMessage;
  isUser: boolean;
  isPending: boolean;
  isFailed: boolean;
  errorMessage: string | undefined;
  isLastTurn: boolean;
  isBusy: boolean;
  conversationId: string | undefined;
  continuedFrom: { conversationId: string; title: string } | null;
  selectConversation: (conversationId: string) => unknown;
  retryFailedMessage: (tempId: string) => unknown;
  dismissFailedMessage: (tempId: string) => void;
  handleRegenerate: () => Promise<void>;
  showCitations: boolean;
  verificationSummary: MessageVerificationSummary | undefined;
  verificationPending: boolean;
  verificationBadge: ReturnType<typeof useMessageVerification>['verificationBadge'];
  canExpandVerification: boolean;
  isVerificationPanelExpanded: boolean;
  setIsVerificationPanelExpanded: Dispatch<SetStateAction<boolean>>;
  verificationPanelId: string;
  isMessageBookmarked: boolean;
  timestamp: string;
}) {
  return (
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
  );
}
