import { useEffect, useRef, useState } from 'react';

import { formatDistanceToNow } from 'date-fns';
import { useNavigate } from 'react-router';

import { cn } from '@/lib/utils';
import type { IndexingSnapshot } from '@/types/api/indexing';

import { FailedItemsList } from './FailedItemsList';
import { formatFailureLine, formatIndexingLine } from './formatIndexingStatus';
import {
  useIndexingActivitiesQuery,
  useIndexingControlMutation,
} from '../../hooks/queries/useIndexingStatusQuery';
import { GHOST_BUTTON_CLASS } from '../Settings/settingsStyles';




// `cn` (tailwind-merge), not template concatenation: GHOST_BUTTON_CLASS already
// carries h-8/px-3/text-sm, and plain string order does not win over stylesheet
// order — the compact size only takes effect once the conflicts are merged out.
const CONTROL_CLASS = cn(GHOST_BUTTON_CLASS, 'h-7 px-2 text-xs');

function basename(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

function relativeTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return '';
  return formatDistanceToNow(date, { addSuffix: true });
}

export interface IndexingStatusPopoverProps {
  snapshot: IndexingSnapshot;
  /** Only fetch activity while the popover is actually on screen. */
  open: boolean;
  onClose: () => void;
}

/**
 * What the index is doing, what failed, and the three verbs that steer it.
 * One level, hairlines only — no cards, no progress bar repeating the count.
 */
export function IndexingStatusPopover({ snapshot, open, onClose }: IndexingStatusPopoverProps) {
  const navigate = useNavigate();
  const control = useIndexingControlMutation();
  const activities = useIndexingActivitiesQuery(5, open);

  const [dismissed, setDismissed] = useState<ReadonlySet<string>>(() => new Set<string>());
  const previousStatus = useRef(snapshot.status);

  // A new run clears the backend failure log, so the local dismissals go too.
  useEffect(() => {
    if (previousStatus.current !== 'scanning' && snapshot.status === 'scanning') {
      setDismissed(new Set<string>());
    }
    previousStatus.current = snapshot.status;
  }, [snapshot.status]);

  const visibleFailures = snapshot.failures.filter((f) => !dismissed.has(f.path));
  const running = snapshot.status === 'scanning' || snapshot.status === 'processing';
  const steerable = running || snapshot.paused;

  const headline =
    formatIndexingLine(snapshot) ??
    (visibleFailures.length > 0 ? formatFailureLine(snapshot) : null) ??
    'Nothing indexing.';

  const failureLine = formatFailureLine(snapshot);

  const showAll = () => {
    // Navigate first: `Settings` installs the `settings:navigate-tab` listener
    // on mount, so dispatching before it exists sends the event nowhere.
    navigate('/settings');
    onClose();
    window.setTimeout(() => {
      window.dispatchEvent(
        new CustomEvent('settings:navigate-tab', { detail: { tab: 'indexing' } }),
      );
    }, 0);
  };

  return (
    <div>
      <div className="px-3 py-2.5">
        <div className="flex items-center justify-between gap-2">
          <span className="truncate text-sm text-text-primary">{headline}</span>
          {steerable ? (
            <span className="flex shrink-0 items-center gap-1">
              {snapshot.paused ? (
                <button
                  type="button"
                  className={CONTROL_CLASS}
                  onClick={() => control.mutate('resume')}
                  disabled={control.isPending}
                >
                  Resume
                </button>
              ) : (
                <button
                  type="button"
                  className={CONTROL_CLASS}
                  onClick={() => control.mutate('pause')}
                  disabled={control.isPending}
                >
                  Pause
                </button>
              )}
              <button
                type="button"
                className={CONTROL_CLASS}
                onClick={() => control.mutate('cancel')}
                disabled={control.isPending}
              >
                Stop
              </button>
            </span>
          ) : null}
        </div>
        {snapshot.currentFile ? (
          <p className="mt-0.5 truncate text-xs text-text-muted" title={snapshot.currentFile}>
            {basename(snapshot.currentFile)}
          </p>
        ) : null}
      </div>

      {visibleFailures.length > 0 && failureLine ? (
        <div className="border-t border-border-subtle px-3 py-2">
          <div className="flex items-center justify-between gap-2 pb-1">
            <span className="text-sm text-text-primary">{failureLine}</span>
            <button
              type="button"
              className="shrink-0 text-xs text-text-secondary transition-colors duration-fast hover:text-text-primary"
              onClick={() =>
                setDismissed(new Set(snapshot.failures.map((failure) => failure.path)))
              }
            >
              Clear all
            </button>
          </div>
          <FailedItemsList
            failures={snapshot.failures}
            dismissed={dismissed}
            onDismiss={(path) => setDismissed((prev) => new Set(prev).add(path))}
          />
        </div>
      ) : null}

      {activities.data && activities.data.length > 0 ? (
        <div className="border-t border-border-subtle px-3 py-2">
          <p className="pb-1 text-xs text-text-muted">Recent activity</p>
          {activities.data.map((activity) => (
            <div
              key={activity.id}
              className="flex items-center justify-between gap-3 border-b border-border-subtle py-1.5 last:border-b-0"
            >
              <span className="truncate text-sm text-text-primary" title={activity.filePath}>
                {basename(activity.filePath)}
              </span>
              <span className="shrink-0 text-xs tabular-nums text-text-muted">
                {relativeTime(activity.timestamp)}
              </span>
            </div>
          ))}
          <div className="flex justify-end pt-1.5">
            <button
              type="button"
              className="text-xs text-text-secondary transition-colors duration-fast hover:text-text-primary"
              onClick={showAll}
            >
              Show all →
            </button>
          </div>
        </div>
      ) : null}
    </div>
  );
}
