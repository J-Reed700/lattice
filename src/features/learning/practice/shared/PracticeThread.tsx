import { Send } from 'lucide-react';
import type React from 'react';
import { useEffect, useRef } from 'react';

import { useSpeaker } from '@/features/speech/hooks/useSpeech';
import { speechChunks } from '@/features/speech/model/speechText';
import { VOICE_STATUS, VoiceButton } from '@/features/voice/components/VoiceButton';
import { useVoiceConversation } from '@/features/voice/hooks/useVoiceConversation';
import { cn } from '@/lib/utils';
import { setDraft, useDraft } from '@/shared/drafts/drafts';

import { StatusLine } from './StatusLine';

export interface ThreadMessage {
  id: string;
  speaker: 'learner' | 'counterpart' | 'coach';
  content: string;
  /** Lines of the learner's code the message is about. */
  lines?: number[];
  /** Overrides the default speaker name. */
  name?: string;
}

export interface PracticeThreadProps {
  label: string;
  messages: ThreadMessage[];
  onSend: (content: string, spoken: boolean) => Promise<void>;
  busy?: string | null;
  error?: { message: string; retry?: () => void } | null;
  placeholder?: string;
  draftKey?: string;
  voice?: boolean;
  suggestion?: string | null;
  disabled?: { reason: string } | null;
  renderMessage?: (message: ThreadMessage) => React.ReactNode;
  /** Shown above the box: quick prompts, a clock. */
  composerTop?: React.ReactNode;
  /** The longest reply the backend accepts. */
  maxLength?: number;
}

const SPEAKER_NAME: Record<ThreadMessage['speaker'], string> = {
  learner: 'You',
  counterpart: 'Counterpart',
  coach: 'Coach',
};

const SPEAKER_STYLE: Record<ThreadMessage['speaker'], string> = {
  learner: 'ml-auto bg-accent-muted text-text-primary',
  counterpart: 'bg-surface-raised text-text-primary',
  coach: 'bg-surface-sunken text-text-primary',
};

function lineRange(lines: number[]): string {
  const first = Math.min(...lines);
  const last = Math.max(...lines);
  return first === last ? `line ${first}` : `lines ${first}–${last}`;
}

function Message({ message, render }: { message: ThreadMessage; render?: PracticeThreadProps['renderMessage'] }) {
  return (
    <article className={cn('max-w-[85%] rounded-2xl px-4 py-3 text-sm', SPEAKER_STYLE[message.speaker])}>
      <p className="mb-1 text-xs font-semibold text-text-secondary">{message.name ?? SPEAKER_NAME[message.speaker]}</p>
      {message.lines && message.lines.length > 0 && (
        <p className="mb-1 inline-block rounded-full bg-surface px-2 py-0.5 text-xs text-text-secondary">
          About {lineRange(message.lines)}
        </p>
      )}
      {render ? render(message) : <p className="whitespace-pre-wrap">{message.content}</p>}
    </article>
  );
}

interface FooterProps {
  messages: ThreadMessage[];
  onSend: PracticeThreadProps['onSend'];
  setText: (value: string) => void;
  spokenSentAfter: React.MutableRefObject<string | null>;
  lastId: string | null;
  disabled: boolean;
}

function Hint({ children }: { children: React.ReactNode }) {
  return <span className="text-xs text-text-muted">{children}</span>;
}

function VoiceControls({ messages, onSend, setText, spokenSentAfter, lastId, disabled }: FooterProps) {
  const speak = useSpeaker();
  const { phase, toggle } = useVoiceConversation({
    conversationId: null,
    send: async (text) => {
      spokenSentAfter.current = lastId ?? '';
      try {
        await onSend(text, true);
      } catch (cause) {
        spokenSentAfter.current = null;
        setText(text);
        throw cause;
      }
      return 'answered';
    },
    fill: setText,
  });

  const latest = messages[messages.length - 1];
  useEffect(() => {
    if (spokenSentAfter.current === null || !latest || latest.speaker === 'learner') return;
    if (latest.id === spokenSentAfter.current) return;
    spokenSentAfter.current = null;
    void speak(`practice-thread:${latest.id}`, speechChunks(latest.content));
  }, [latest, speak, spokenSentAfter]);

  return (
    <>
      {phase === 'off' ? <Hint>Ctrl+Enter to send</Hint> : <Hint>{VOICE_STATUS[phase]}</Hint>}
      {!disabled && <VoiceButton phase={phase} onToggle={toggle} />}
    </>
  );
}

/** A conversation transcript with a composer, an optional mic, and its busy and error lines. */
export function PracticeThread({
  label,
  messages,
  onSend,
  busy = null,
  error = null,
  placeholder,
  draftKey,
  voice = true,
  suggestion = null,
  disabled = null,
  renderMessage,
  composerTop,
  maxLength,
}: PracticeThreadProps) {
  const [text, setText] = useDraft(draftKey ?? null);
  const box = useRef<HTMLTextAreaElement>(null);
  const log = useRef<HTMLDivElement>(null);
  const spokenSentAfter = useRef<string | null>(null);
  const wasBusy = useRef(Boolean(busy));
  const lastId = messages[messages.length - 1]?.id ?? null;

  useEffect(() => {
    if (wasBusy.current && !busy) box.current?.focus();
    wasBusy.current = Boolean(busy);
  }, [busy]);

  useEffect(() => {
    const container = log.current;
    const newest = container?.querySelector('article:last-of-type');
    if (!container || !newest) return;
    container.scrollTop += newest.getBoundingClientRect().top - container.getBoundingClientRect().top - 12;
  }, [lastId]);

  const send = async () => {
    const content = text.trim();
    if (!content || busy || disabled) return;
    try {
      await onSend(content, false);
      if (draftKey) setDraft(draftKey, '');
      else setText('');
    } catch {
      return;
    } finally {
      box.current?.focus();
    }
  };

  const onKeyDown = (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      void send();
    }
  };

  const status = busy
    ? ({ kind: 'busy', label: busy } as const)
    : error
      ? ({ kind: 'error', message: error.message, retry: error.retry } as const)
      : ({ kind: 'idle' } as const);

  return (
    <div className="flex min-h-0 flex-col gap-3">
      <div
        ref={log}
        role="log"
        aria-label={label}
        className="flex max-h-[28rem] min-h-24 flex-col gap-3 overflow-y-auto rounded-2xl border border-border-subtle bg-surface p-3"
      >
        {messages.map((message) => (
          <Message key={message.id} message={message} render={renderMessage} />
        ))}
      </div>
      <StatusLine state={status} />
      {suggestion && <p className="text-xs text-text-secondary">{suggestion}</p>}
      {composerTop}
      <textarea
        ref={box}
        aria-label="Your response"
        value={text}
        onChange={(event) => setText(event.target.value)}
        onKeyDown={onKeyDown}
        placeholder={placeholder}
        readOnly={Boolean(busy)}
        aria-busy={busy ? true : undefined}
        disabled={Boolean(disabled)}
        rows={3}
        maxLength={maxLength}
        className="w-full resize-y rounded-xl border border-border-default bg-surface-sunken px-3 py-2 text-sm text-text-primary focus:outline-hidden focus:ring-2 focus:ring-ring disabled:opacity-60"
      />
      <div className="flex flex-wrap items-center justify-end gap-3">
        {voice ? (
          <VoiceControls
            messages={messages}
            onSend={onSend}
            setText={setText}
            spokenSentAfter={spokenSentAfter}
            lastId={lastId}
            disabled={Boolean(disabled)}
          />
        ) : (
          <Hint>Ctrl+Enter to send</Hint>
        )}
        <button
          type="button"
          onClick={() => void send()}
          disabled={Boolean(disabled) || Boolean(busy) || text.trim() === ''}
          className="inline-flex items-center gap-1.5 rounded-lg bg-accent px-3 py-1.5 text-xs font-medium text-accent-fg focus:outline-hidden focus:ring-2 focus:ring-ring disabled:opacity-60"
        >
          <Send size={14} aria-hidden="true" />
          Send
        </button>
      </div>
      {disabled && <p className="text-xs text-text-secondary">{disabled.reason}</p>}
    </div>
  );
}
