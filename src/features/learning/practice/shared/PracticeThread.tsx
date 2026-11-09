import React, { useCallback, useEffect, useMemo, useRef } from 'react';
import { ArrowUp } from 'lucide-react';

import { VOICE_STATUS, VoiceButton } from '@/features/voice/components/VoiceButton';
import { useVoiceConversation } from '@/features/voice/hooks/useVoiceConversation';
import { cn } from '@/lib/utils';
import { useDraft } from '@/shared/drafts/drafts';

import { StatusLine, type VoicePhase } from './StatusLine';

export type ThreadSpeaker = 'learner' | 'counterpart' | 'coach';

export interface ThreadMessage {
  id: string;
  speaker: ThreadSpeaker;
  /** The speaker's name as shown: "Interviewer", "Coach", "You". */
  name: string;
  /** Markdown. */
  content: string;
  /** Anything the host adds under a message: citations, a hint's rung. */
  footer?: React.ReactNode;
}

export interface PracticeThreadTurn {
  id: string;
  speaker: ThreadSpeaker;
  speakerLabel?: string;
  name?: string;
  content: string;
  createdAt?: string | number | null;
  badge?: string | null;
  extra?: React.ReactNode;
  footer?: React.ReactNode;
}

export interface PracticeThreadProps {
  heading?: string;
  conversationId?: string | null;
  turns?: PracticeThreadTurn[];
  messages?: ThreadMessage[];
  draftKey: string | null;
  placeholder?: string;
  sendLabel?: string;
  busy?: string | null;
  busyLabel?: string | null;
  error?: string | null;
  onRetry?: () => void;
  disabled?: boolean;
  disabledReason?: string | null;
  closedReason?: string | null;
  onSend: (content: string, spoken: boolean) => Promise<void> | void;
  clearOnSend?: boolean;
  emptyText?: string;
  header?: React.ReactNode;
  footerActions?: React.ReactNode;
  hint?: React.ReactNode;
  voice?: { phase: VoicePhase; onToggle: () => void };
  onPhaseChange?: (phase: VoicePhase) => void;
  logRef?: React.Ref<HTMLDivElement>;
  logLabel?: string;
  label?: string;
  inputLabel?: string;
  sendAriaLabel?: string;
  retryAriaLabel?: string;
  maxLength?: number;
  renderTurnContent?: (turn: PracticeThreadTurn) => React.ReactNode;
  renderTurnExtra?: (turn: PracticeThreadTurn) => React.ReactNode;
  composerTop?: React.ReactNode;
  className?: string;
}

/**
 * Unified conversation thread for practice, role-play simulations, and coaching.
 *
 * Provides transcript history, accessible busy line, mic input via VoiceButton,
 * persistent unsent draft, and keyboard submission (Ctrl+Enter).
 */
export function PracticeThread({
  heading,
  conversationId,
  turns,
  messages,
  draftKey,
  placeholder = 'Type your response, reasoning, or question…',
  sendLabel = 'Send',
  busy = null,
  busyLabel = null,
  error = null,
  onRetry,
  disabled = false,
  disabledReason = null,
  closedReason = null,
  onSend,
  clearOnSend = true,
  emptyText = 'No messages yet.',
  header,
  footerActions,
  hint,
  voice: externalVoice,
  logRef,
  logLabel,
  label,
  inputLabel = 'Your message',
  sendAriaLabel,
  retryAriaLabel,
  maxLength,
  renderTurnContent,
  renderTurnExtra,
  composerTop,
  className,
  onPhaseChange,
}: PracticeThreadProps) {
  const [draft, setDraft] = useDraft(draftKey);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const lastTurnRef = useRef<HTMLElement>(null);
  const effectiveBusy = busy ?? busyLabel;
  const isBusy = Boolean(effectiveBusy);
  const effectiveClosedReason = closedReason ?? disabledReason;
  const isClosed = disabled || Boolean(closedReason);
  const effectiveInputLabel = label ?? inputLabel;
  const effectiveLogLabel = logLabel ?? heading ?? 'Practice conversation turns';

  const normalizedTurns: PracticeThreadTurn[] = useMemo(() => {
    if (turns) return turns;
    if (messages) {
      return messages.map((m) => ({
        id: m.id,
        speaker: m.speaker,
        speakerLabel: m.name,
        name: m.name,
        content: m.content,
        extra: m.footer,
        footer: m.footer,
      }));
    }
    return [];
  }, [turns, messages]);

  const internalVoice = useVoiceConversation({
    conversationId: conversationId ?? null,
    send: async (text: string) => {
      await onSend(text, true);
      return undefined;
    },
    fill: (text: string) => {
      setDraft(text);
    },
  });

  const voicePhase = externalVoice ? externalVoice.phase : internalVoice.phase;
  const onVoiceToggle = externalVoice ? externalVoice.onToggle : internalVoice.toggle;

  useEffect(() => {
    onPhaseChange?.(voicePhase);
  }, [onPhaseChange, voicePhase]);

  // P2-9: Restore focus to textarea when a turn / reply completes
  const wasBusyRef = useRef(isBusy);
  useEffect(() => {
    if (wasBusyRef.current && !isBusy) {
      textareaRef.current?.focus();
    }
    wasBusyRef.current = isBusy;
  }, [isBusy]);

  // Scroll to bottom when turns change or reply lands
  useEffect(() => {
    lastTurnRef.current?.scrollIntoView?.({ behavior: 'smooth' });
  }, [normalizedTurns.length, effectiveBusy]);

  const handleSend = useCallback(async (spoken = false) => {
    const trimmed = draft.trim();
    if (!trimmed || isBusy || isClosed) return;
    if (clearOnSend) {
      setDraft('');
    }
    await onSend(trimmed, spoken);
  }, [draft, isBusy, isClosed, clearOnSend, onSend, setDraft]);

  const handleKeyDown = (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') {
      event.preventDefault();
      void handleSend(false);
    }
  };

  return (
    <div className={cn('flex h-full flex-col overflow-hidden rounded-2xl border border-border-default bg-surface', className)}>
      {(header || heading) && (
        <div className="border-b border-border-subtle p-3 sm:px-4">
          {header ?? <h3 className="font-serif text-base text-text-primary">{heading}</h3>}
        </div>
      )}

      {/* Transcript list */}
      <div
        ref={logRef}
        role="log"
        aria-label={effectiveLogLabel}
        aria-live="polite"
        className="flex-1 space-y-3 overflow-y-auto p-3 sm:p-4"
      >
        {normalizedTurns.length === 0 ? (
          <div className="grid min-h-[140px] place-items-center text-center text-xs text-text-muted">
            <p>{emptyText}</p>
          </div>
        ) : (
          normalizedTurns.map((turn, index) => {
            const isLearner = turn.speaker === 'learner';
            const isCoach = turn.speaker === 'coach';
            const isLast = index === normalizedTurns.length - 1;

            return (
              <article
                key={turn.id}
                ref={isLast ? lastTurnRef : undefined}
                className={cn(
                  'max-w-[88%] rounded-2xl p-3 sm:p-3.5',
                  isLearner
                    ? 'ml-auto bg-surface-raised text-text-primary'
                    : isCoach
                      ? 'border-l-2 border-accent bg-accent-muted text-text-primary'
                      : 'bg-surface-sunken text-text-primary',
                )}
              >
                <div className="flex items-center justify-between gap-2">
                  <span className="text-xs font-semibold uppercase tracking-wider text-text-secondary">
                    {turn.speakerLabel ?? turn.name}
                  </span>
                  {turn.badge && (
                    <span className="rounded-full bg-surface px-2 py-0.5 text-xs text-text-tertiary">
                      {turn.badge}
                    </span>
                  )}
                </div>
                {renderTurnContent ? (
                  renderTurnContent(turn)
                ) : (
                  <p className="mt-1.5 whitespace-pre-wrap text-sm leading-6 text-text-primary">
                    {turn.content}
                  </p>
                )}
                {turn.extra ?? turn.footer}
                {renderTurnExtra && renderTurnExtra(turn)}
              </article>
            );
          })
        )}
      </div>

      {/* StatusLine in aria-live container (P1-1) */}
      <div className="px-3 sm:px-4">
        <StatusLine
          busy={effectiveBusy}
          error={error}
          onRetry={onRetry}
          retryAriaLabel={retryAriaLabel}
        />
      </div>

      {/* Message box composer */}
      <div className="border-t border-border-subtle bg-surface-sunken/40 p-3 sm:p-4">
        {composerTop && (
          <div className="mb-2">
            {composerTop}
          </div>
        )}
        <label htmlFor="practice-thread-input" className="sr-only">
          {effectiveInputLabel}
        </label>
        <textarea
          ref={textareaRef}
          id="practice-thread-input"
          rows={3}
          value={draft}
          disabled={isClosed}
          readOnly={isBusy}
          aria-busy={isBusy ? 'true' : 'false'}
          maxLength={maxLength}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={placeholder}
          className={cn(
            'w-full resize-y rounded-xl border border-border-default bg-surface px-3 py-2 text-sm leading-6 text-text-primary placeholder:text-text-muted outline-hidden transition-colors',
            'focus:border-accent focus:ring-1 focus:ring-ring',
            isBusy && 'bg-surface-sunken cursor-not-allowed text-text-secondary',
          )}
        />

        {hint && (
          <div className="mt-1.5 text-xs text-text-muted">
            {hint}
          </div>
        )}

        <div className="mt-2.5 flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            {footerActions}
            {effectiveClosedReason && (
              <span className="text-xs text-text-muted">
                {effectiveClosedReason}
              </span>
            )}
          </div>

          <div className="flex items-center gap-2">
            {/* Shortcut hint or voice status */}
            <span
              role={voicePhase === 'off' ? undefined : 'status'}
              className="text-xs text-text-muted"
            >
              {voicePhase !== 'off'
                ? VOICE_STATUS[voicePhase]
                : 'Ctrl+Enter'}
            </span>

            {/* Voice microphone button */}
            <VoiceButton phase={voicePhase} onToggle={onVoiceToggle} />

            {/* Send button */}
            <button
              type="button"
              disabled={isClosed || isBusy || !draft.trim()}
              onClick={() => void handleSend(false)}
              aria-label={sendAriaLabel ?? sendLabel}
              title={isClosed && effectiveClosedReason ? effectiveClosedReason : `${sendLabel} · Ctrl+Enter`}
              className="pressable inline-flex h-8 items-center gap-1.5 rounded-full bg-accent px-3.5 text-xs font-semibold text-accent-fg transition-opacity duration-fast disabled:cursor-not-allowed disabled:opacity-40"
            >
              <span>{sendLabel}</span>
              <ArrowUp className="h-3.5 w-3.5" strokeWidth={2.2} aria-hidden="true" />
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
