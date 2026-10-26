import { canOpenActivity, useOpenActivity } from '@/features/activity/hooks/useOpenActivity';
import { progressPercent, stateLine } from '@/features/activity/model/activityFormat';
import type { ActivityEntryDto } from '@/lib/bindings';
import { cn } from '@/lib/utils';

import { IrisOrb } from './IrisOrb';

import type { IrisDestination, IrisReading } from '../model/irisReading';

/** Enough to see what is going on; the Activity drawer lists the rest. */
const WORK_SHOWN = 4;

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
  const percent = item.progress ? progressPercent(item.progress) : null;
  const body = (
    <>
      <span className="flex items-baseline justify-between gap-2">
        <span className="truncate text-xs font-medium text-text-primary">{item.title}</span>
        {percent !== null && <span className="shrink-0 text-xxs tabular-nums text-text-muted">{percent}%</span>}
      </span>
      <span className="block truncate text-xxs font-normal text-text-muted">{stateLine(item, now)}</span>
      {percent !== null && (
        <span className="iris-bar mt-1.5" aria-hidden="true">
          <span style={{ width: `${percent}%` }} />
        </span>
      )}
    </>
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

/** What Iris says when you click her: one line, anything worth a look, and what is running. */
export function IrisCard({ reading, onGo, onHide, onClose }: IrisCardProps) {
  const openItem = useOpenActivity(onClose);
  const now = Date.now();
  const shown = reading.work.slice(0, WORK_SHOWN);
  const more = reading.work.length - shown.length;

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

      {shown.length > 0 && (
        <section aria-labelledby="iris-work" className="border-t border-border-subtle px-4 py-3">
          <h3 id="iris-work" className={SECTION_HEADING}>Working on</h3>
          <ul className="mt-1.5 space-y-0.5">
            {shown.map((item) => (
              <WorkRow key={item.id} item={item} now={now} onOpen={openItem} />
            ))}
          </ul>
          {more > 0 && (
            <p className="mt-1.5 text-xxs font-normal text-text-muted">
              And {more} more in Activity.
            </p>
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
