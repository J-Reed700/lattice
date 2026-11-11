import { useState } from 'react';

import { Check, Lightbulb } from 'lucide-react';

import type * as Wire from '@/lib/bindings';

import type { KindCopy, KindView } from './kind';

export interface HintLadderProps {
  session: Wire.PracticeSessionDto;
  copy: Pick<KindCopy, 'rung'>;
  HintMark: KindView['HintMark'];
  asking?: boolean;
  onAsk: () => void;
}

export function HintLadder({ session, copy, HintMark, asking = false, onAsk }: HintLadderProps) {
  const { hints, nextRung } = session;
  const [read, setRead] = useState<ReadonlySet<number>>(new Set());
  const latest = hints[hints.length - 1] as Wire.PracticeHintDto | undefined;
  const latestRead = latest === undefined || read.has(latest.ordinal);

  return (
    <section aria-labelledby="practice-hints-heading" className="rounded-xl border border-border-default bg-surface p-4">
      <h2 id="practice-hints-heading" className="m-0 font-serif text-lg font-semibold text-text-primary">
        Stuck?
      </h2>
      <p className="m-0 mt-1 text-sm text-text-secondary">One hint at a time. Every hint says why it helps.</p>

      {hints.length > 0 ? (
        <ol className="m-0 mt-3 list-none space-y-3 p-0">
          {hints.map((hint) => (
            <li key={hint.ordinal} className="rounded-lg border border-border-subtle bg-surface-sunken p-3">
              <div className="flex items-center gap-2 text-xs font-semibold text-text-secondary">
                <Lightbulb className="h-3.5 w-3.5" aria-hidden="true" />
                <span>{copy.rung(hint.rung).name}</span>
              </div>
              <p className="m-0 mt-1.5 text-sm font-medium text-text-primary">{hint.text}</p>
              <p className="m-0 mt-1 text-sm text-text-secondary">Why: {hint.reason}</p>
              {HintMark ? <HintMark session={session} hint={hint} /> : null}
            </li>
          ))}
        </ol>
      ) : null}

      <div className="mt-3 flex flex-col items-start gap-1.5">
        {latestRead ? (
          <button
            type="button"
            disabled={asking}
            onClick={onAsk}
            className="min-h-[44px] cursor-pointer rounded-lg border border-border-default bg-surface px-4 text-sm font-semibold text-text-primary hover:bg-surface-raised disabled:cursor-not-allowed disabled:opacity-50"
          >
            {asking ? 'Getting a hint…' : copy.rung(nextRung).ask}
          </button>
        ) : (
          <>
            <button
              type="button"
              onClick={() => latest && setRead((current) => new Set(current).add(latest.ordinal))}
              className="inline-flex min-h-[44px] cursor-pointer items-center gap-2 rounded-lg border border-border-default bg-surface px-4 text-sm font-semibold text-text-primary hover:bg-surface-raised"
            >
              <Check className="h-4 w-4" aria-hidden="true" />
              <span>I&apos;ve read this</span>
            </button>
            <span className="text-sm text-text-secondary">The next hint opens once you have read this one.</span>
          </>
        )}
      </div>
    </section>
  );
}
