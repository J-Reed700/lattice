import { ArrowDownRight, ShieldCheck } from 'lucide-react';

import { helpWord } from '@/features/learning/practice/workbench/helpModes';
import type * as Wire from '@/lib/bindings';

type AssistanceEvent = Wire.LearningPracticeSessionDto['assistance'][number];

const KIND_LABELS: Record<string, string> = {
  source_opened: 'Opened a saved source',
  hint: 'Requested a tutor hint',
  solution_revealed: 'Revealed worked solution',
  mode_changed: 'Changed help',
};

export const dateLabel = (value: number) => {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? 'Time unavailable' : date.toLocaleString();
};

export function AssistanceLedger({ events }: { events: AssistanceEvent[] }) {
  return (
    <section className="rounded-2xl border border-border-default bg-surface p-5">
      <div className="text-xs font-semibold uppercase tracking-[.15em] text-text-muted">Assistance ledger</div>
      <h4 className="mt-1 font-serif text-lg text-text-primary">Help used in this attempt</h4>
      {events.length ? (
        <ol className="mt-3 space-y-3">
          {events.map((event) => (
            <li key={event.id} className="flex gap-2 border-t border-border-default pt-3 first:border-0 first:pt-0">
              <span className="mt-0.5 grid h-5 w-5 shrink-0 place-items-center rounded-full bg-accent-muted text-accent">
                <ArrowDownRight size={12} aria-hidden="true" />
              </span>
              <span className="min-w-0">
                <span className="block text-xs font-medium text-text-primary">{KIND_LABELS[event.kind] ?? event.kind}</span>
                <span className="mt-0.5 block text-xs leading-4 text-text-muted">
                  {helpWord(event.mode)} · {dateLabel(event.createdAt)}
                </span>
              </span>
            </li>
          ))}
        </ol>
      ) : (
        <p className="mt-3 text-xs leading-5 text-text-muted">
          No assistance recorded yet. Source openings, tutor turns, help changes, and solution reveals are listed here.
        </p>
      )}
      <div className="mt-4 rounded-lg bg-surface-sunken px-3 py-2 text-xs leading-4 text-text-muted">
        <ShieldCheck size={12} className="mr-1 inline text-accent" aria-hidden="true" /> Results are interpreted alongside the help used.
      </div>
    </section>
  );
}
