import { HelpCircle } from 'lucide-react';

import type * as Wire from '@/lib/bindings';

import type { Failure } from './kind';
import { PracticeThread, type ThreadMessage } from '../shared/PracticeThread';

export interface TalkPanelProps {
  session: Wire.PracticeSessionDto;
  counterpart: string;
  /** Shown while the counterpart reads the work; null when no reply is pending. */
  busy: string | null;
  error: Failure | null;
  asking: boolean;
  onSend: (content: string, spoken: boolean) => Promise<void>;
  onAsk: () => void;
}

/** The conversation with the coach or interviewer, and "I'm stuck" in Watched. */
export function TalkPanel({ session, counterpart, busy, error, asking, onSend, onAsk }: TalkPanelProps) {
  const { setting } = session;
  const messages: ThreadMessage[] = session.turns.map((turn) => ({
    id: turn.id,
    speaker: turn.speaker,
    content: turn.content,
  }));

  return (
    <section
      aria-labelledby="practice-thread-heading"
      className="flex min-h-0 flex-col gap-3 rounded-xl border border-border-default bg-surface p-4"
    >
      <h2 id="practice-thread-heading" className="m-0 font-serif text-lg font-semibold text-text-primary">
        {counterpart}
      </h2>
      <PracticeThread
        label={`Message the ${counterpart.toLowerCase()}`}
        messages={messages}
        onSend={onSend}
        busy={busy}
        error={error}
        placeholder={setting === 'watched' ? 'Think out loud…' : 'Ask, or explain your thinking…'}
        draftKey={`coding-thread-${session.sessionId}`}
        suggestion={setting === 'watched' ? 'Talk through your thinking: type or use the mic' : null}
        disabled={session.finished ? { reason: 'This session is finished.' } : null}
      />
      {setting === 'watched' ? (
        <div className="flex flex-col items-start gap-1">
          <button
            type="button"
            onClick={onAsk}
            disabled={asking}
            className="inline-flex min-h-[44px] cursor-pointer items-center gap-2 rounded-lg border border-border-default bg-surface px-4 text-sm font-semibold text-text-primary hover:bg-surface-raised disabled:cursor-not-allowed disabled:opacity-50"
          >
            <HelpCircle className="h-4 w-4" aria-hidden="true" />
            <span>I&apos;m stuck</span>
          </button>
          <span className="text-sm text-text-secondary">
            The interviewer answers in the conversation. Hints are noted in your debrief.
          </span>
        </div>
      ) : null}
    </section>
  );
}
