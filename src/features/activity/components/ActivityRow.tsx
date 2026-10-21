import type { ActivityEntryDto } from '@/lib/bindings';
import { cn } from '@/lib/utils';

import { canOpenActivity } from '../hooks/useOpenActivity';
import { formatProgress, formatTimeLeft, progressPercent, stateLine } from '../model/activityFormat';

const STATE_DOT: Record<ActivityEntryDto['state'], string> = {
  running: 'bg-accent',
  paused: 'bg-warning',
  waiting: 'bg-text-muted',
};

const STATE_NAME: Record<ActivityEntryDto['state'], string> = {
  running: 'Running',
  paused: 'Paused',
  waiting: 'Waiting',
};

interface ActivityRowProps {
  item: ActivityEntryDto;
  now: number;
  onOpen: (item: ActivityEntryDto) => void;
}

/**
 * One live item: its title, the line that says what it does or why it
 * waits, a bar where it has progress, and the time left when one was
 * measured. Opens where it lives when it has a place to open.
 */
export function ActivityRow({ item, now, onOpen }: ActivityRowProps) {
  const progress = item.progress;
  const facts = [
    progress ? formatProgress(progress) : null,
    item.secondsLeft !== null && item.state === 'running' ? formatTimeLeft(item.secondsLeft) : null,
    item.holding,
  ].filter((fact): fact is string => Boolean(fact));
  const body = (
    <>
      <span className="flex items-start gap-2">
        <span
          aria-hidden="true"
          className={cn('mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full', STATE_DOT[item.state])}
        />
        <span className="min-w-0 flex-1">
          <span className="block truncate text-sm text-text-primary">{item.title}</span>
          <span className="mt-0.5 block text-xs text-text-secondary">
            <span className="sr-only">{STATE_NAME[item.state]}: </span>
            {stateLine(item, now)}
          </span>
        </span>
      </span>
      {progress && (
        <span
          role="progressbar"
          aria-label={`${item.title} progress`}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={progressPercent(progress)}
          aria-valuetext={formatProgress(progress)}
          className="mt-2 ml-3.5 block h-1 overflow-hidden rounded-full bg-surface-raised"
        >
          <span
            className={cn(
              'block h-full rounded-full transition-[width] duration-300 ease-out',
              item.state === 'running' ? 'bg-accent' : 'bg-text-muted',
            )}
            style={{ width: `${progressPercent(progress)}%` }}
          />
        </span>
      )}
      {facts.length > 0 && (
        <span className="mt-1.5 ml-3.5 flex flex-wrap gap-x-2 text-xs text-text-muted">
          {facts.map((fact, index) => (
            <span key={fact}>
              {index > 0 && <span aria-hidden="true" className="mr-2">·</span>}
              {fact}
            </span>
          ))}
        </span>
      )}
    </>
  );

  if (!canOpenActivity(item)) {
    return <li className="rounded-lg border border-border-subtle bg-surface p-3">{body}</li>;
  }
  return (
    <li>
      <button
        type="button"
        onClick={() => onOpen(item)}
        className={cn(
          'block w-full rounded-lg border border-border-subtle bg-surface p-3 text-left transition-colors duration-fast',
          'hover:bg-surface-raised focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring',
        )}
      >
        {body}
      </button>
    </li>
  );
}
