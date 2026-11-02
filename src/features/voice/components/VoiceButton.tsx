import { Loader2, Mic } from 'lucide-react';

import { cn } from '@/lib/utils';

import type { VoicePhase } from '../model/voiceSession';

/** Where a voice conversation has got to, in the composer's corner. */
export const VOICE_STATUS: Record<Exclude<VoicePhase, 'off'>, string> = {
  starting: 'Starting…',
  listening: 'Listening…',
  hearing: 'Hearing you…',
  transcribing: 'Working out what you said…',
  thinking: 'Thinking…',
  speaking: 'Speaking · talk to interrupt',
};

/** The mic beside Send: one press starts a voice conversation, the next ends it. */
export function VoiceButton({ phase, onToggle }: { phase: VoicePhase; onToggle: () => void }) {
  const on = phase !== 'off';
  const label = on ? 'End the voice conversation' : 'Start a voice conversation';
  return (
    <button
      type="button"
      onClick={onToggle}
      aria-pressed={on}
      aria-label={label}
      title={label}
      className={cn(
        'pressable inline-flex h-8 w-8 items-center justify-center rounded-full transition-[background-color,color,scale] duration-fast',
        on
          ? 'bg-[hsl(var(--accent))] text-[hsl(var(--accent-fg))] hover:bg-[hsl(var(--accent-hover))]'
          : 'text-[hsl(var(--text-tertiary))] hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-[hsl(var(--text-primary))]',
      )}
    >
      {phase === 'starting' ? (
        <Loader2 className="h-[15px] w-[15px] animate-spin" aria-hidden="true" />
      ) : (
        <Mic
          className={cn('h-[15px] w-[15px]', (phase === 'listening' || phase === 'hearing') && 'motion-safe:animate-pulse')}
          strokeWidth={1.8}
          aria-hidden="true"
        />
      )}
    </button>
  );
}
