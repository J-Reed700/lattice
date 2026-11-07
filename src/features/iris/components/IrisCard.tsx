import { canOpenActivity, useOpenActivity } from '@/features/activity/hooks/useOpenActivity';
import {
  activitySummary,
  formatProgress,
  formatTimeLeft,
  progressPercent,
  STATE_WORDS,
  stateLine,
} from '@/features/activity/model/activityFormat';
import type { ActivityEntryDto } from '@/lib/bindings';
import { cn } from '@/lib/utils';

import { IrisOrb } from './IrisOrb';

import type { IrisDestination, IrisReading } from '../model/irisReading';

const SECTION_HEADING = 'text-xxs uppercase tracking-[0.08em] text-text-muted';
const LINK_BUTTON =
  'shrink-0 rounded-sm text-xs font-medium text-accent hover:underline focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring';

interface IrisCardProps {
  reading: IrisReading;
  onGo: (to: IrisDestination) => void;
  onHide: () => void;
  /** After opening an item's page the card closes. */
  onClose: () => void;
}

function WorkRow({ item, now, onOpen }: { item: ActivityEntryDto; now: number; onOpen: (item: ActivityEntryDto) => void }) {
  const measured = item.progress && { percent: progressPercent(item.progress), text: formatProgress(item.progress) };
  const facts = [
    measured?.text,
    item.secondsLeft !== null && item.state === 'running' ? formatTimeLeft(item.secondsLeft) : null,
    item.holding,
  ].filter((fact): fact is string => Boolean(fact));
  const body = (
    <span className="flex items-start gap-2">
      <span className="iris-dot" data-state={item.state} aria-hidden="true" />
      <span className="min-w-0 flex-1">
        <span className="flex items-baseline justify-between gap-2">
          <span className="truncate text-xs font-medium leading-5 text-text-primary">{item.title}</span>
          {measured && <span className="shrink-0 text-xxs tabular-nums text-text-muted">{measured.percent}%</span>}
        </span>
        <span className="block text-xxs font-normal text-text-muted">
          <span className="sr-only">{STATE_WORDS[item.state]}: </span>
          {stateLine(item, now)}
        </span>
        {measured && (
          <span
            role="progressbar"
            aria-label={`${item.title} progress`}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={measured.percent}
            aria-valuetext={measured.text}
            className="iris-bar mt-1.5"
          >
            <span style={{ width: `${measured.percent}%` }} />
          </span>
        )}
        {facts.length > 0 && (
          <span className="mt-1 flex flex-wrap gap-x-2 text-xxs font-normal text-text-muted">
            {facts.map((fact, index) => (
              <span key={fact}>
                {index > 0 && <span aria-hidden="true" className="mr-2">·</span>}
                {fact}
              </span>
            ))}
          </span>
        )}
      </span>
    </span>
  );
  const shape = '-mx-2 block w-[calc(100%+1rem)] rounded-md px-2 py-1.5 text-left';
  if (!canOpenActivity(item)) return <li className={shape}>{body}</li>;
  return (
    <li>
      <button
        type="button"
        onClick={() => onOpen(item)}
        className={cn(shape, 'transition-colors duration-fast hover:bg-surface-raised focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring')}
      >
        {body}
      </button>
    </li>
  );
}

/** What Iris says when you click her: one line, anything worth a look, and everything that is running. */
export function IrisCard({ reading, onGo, onHide, onClose }: IrisCardProps) {
  const openItem = useOpenActivity(onClose);
  const now = Date.now();
  const { work, activityFailed } = reading;

  return (
    <div className="text-text-primary">
      <header className="flex items-center gap-3.5 px-4 pb-3.5 pt-4">
        <IrisOrb mood={reading.mood} tone={reading.tone} progress={reading.progress} size={34} className="m-1.5" />
        <div className="min-w-0">
          <h2 className="font-serif text-[15px] font-medium tracking-[-0.01em]">Iris</h2>
          <p role="status" aria-live="polite" className="mt-0.5 text-xs text-text-secondary">
            {reading.headline}
          </p>
        </div>
      </header>

      {reading.notes.length > 0 && (
        <section aria-labelledby="iris-notes" className="border-t border-border-subtle px-4 py-3">
          <h3 id="iris-notes" className={SECTION_HEADING}>Worth a look</h3>
          <ul className="mt-2 space-y-2">
            {reading.notes.map((note) => (
              <li key={note.id} className="flex items-start gap-2.5">
                <span className="iris-dot" data-tone={note.tone} aria-hidden="true" />
                <span className="min-w-0 flex-1 text-xs leading-5 text-text-primary">{note.text}</span>
                {note.action && (
                  <button type="button" onClick={() => onGo(note.action!.to)} className={cn(LINK_BUTTON, 'leading-5')}>
                    {note.action.label}
                  </button>
                )}
              </li>
            ))}
          </ul>
        </section>
      )}

      {(work.length > 0 || !activityFailed) && (
        <section aria-labelledby="iris-work" className="border-t border-border-subtle px-4 py-3">
          <div className="flex items-baseline justify-between gap-3">
            <h3 id="iris-work" className={SECTION_HEADING}>Working on</h3>
            <p className="text-xxs font-normal text-text-muted">{activitySummary(work)}</p>
          </div>
          {work.length === 0 ? (
            <p className="mt-1.5 text-xxs font-normal text-text-muted">
              Lessons, indexes, model requests and downloads appear here while they run.
            </p>
          ) : (
            <div className="-mx-2.5 mt-1 max-h-72 overflow-y-auto overscroll-contain px-2.5 py-0.5">
              <ul aria-label="Running, paused and waiting work" className="space-y-0.5">
                {work.map((item) => (
                  <WorkRow key={item.id} item={item} now={now} onOpen={openItem} />
                ))}
              </ul>
            </div>
          )}
        </section>
      )}

      <footer className="flex items-center justify-between gap-3 border-t border-border-subtle px-4 py-2.5">
        <span className="text-xxs font-normal text-text-muted">Local only. Never calls a model.</span>
        <button type="button" onClick={onHide} className={cn(LINK_BUTTON, 'text-text-muted hover:text-text-primary')}>
          Hide
        </button>
      </footer>
    </div>
  );
}
