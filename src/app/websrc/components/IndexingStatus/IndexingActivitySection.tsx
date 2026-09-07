import { formatDistanceToNow } from 'date-fns';

import {
  useIndexingActivitiesQuery,
  useIndexingControlMutation,
  useIndexingStatusQuery,
} from '../../hooks/queries/useIndexingStatusQuery';
import { SECONDARY_BUTTON_CLASS } from '../Settings/settingsStyles';
import { SettingsSection } from '../ui';
import { FailedItemsList } from './FailedItemsList';
import { formatFailureLine, formatIndexingLine } from './formatIndexingStatus';

function relativeTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return '';
  return formatDistanceToNow(date, { addSuffix: true });
}

function fileName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

/**
 * Settings → Indexing → Activity. The same state the rail popover shows, with
 * room for the full log instead of five rows.
 */
export function IndexingActivitySection() {
  const { data: snapshot } = useIndexingStatusQuery();
  const control = useIndexingControlMutation();
  const { data: activities } = useIndexingActivitiesQuery(25);

  const running = snapshot?.status === 'scanning' || snapshot?.status === 'processing';
  const paused = snapshot?.paused ?? false;
  const steerable = running || paused;

  const statusLine = formatIndexingLine(snapshot) ?? 'Nothing indexing.';
  const failureLine = formatFailureLine(snapshot);
  const failures = snapshot?.failures ?? [];

  return (
    <SettingsSection
      title="Activity"
      actions={
        steerable ? (
          <>
            {paused ? (
              <button
                type="button"
                className={SECONDARY_BUTTON_CLASS}
                onClick={() => control.mutate('resume')}
                disabled={control.isPending}
              >
                Resume
              </button>
            ) : (
              <button
                type="button"
                className={SECONDARY_BUTTON_CLASS}
                onClick={() => control.mutate('pause')}
                disabled={control.isPending}
              >
                Pause
              </button>
            )}
            <button
              type="button"
              className={SECONDARY_BUTTON_CLASS}
              onClick={() => control.mutate('cancel')}
              disabled={control.isPending}
            >
              Stop
            </button>
          </>
        ) : null
      }
    >
      <div className="border-b border-border-subtle py-2.5 text-sm text-text-primary">
        {statusLine}
      </div>

      {failures.length > 0 && failureLine ? (
        <div className="border-b border-border-subtle py-2.5">
          <p className="pb-1 text-sm text-text-primary">{failureLine}</p>
          <FailedItemsList failures={failures} limit={10} />
        </div>
      ) : null}

      {!activities || activities.length === 0 ? (
        <div className="border-b border-border-subtle py-3 text-sm text-text-muted">
          Nothing indexed yet. Add a folder to start.
        </div>
      ) : (
        activities.map((activity) => (
          <div
            key={activity.id}
            className="flex items-center justify-between gap-4 border-b border-border-subtle py-2.5"
          >
            <span className="truncate text-sm text-text-primary" title={activity.file_path}>
              {fileName(activity.file_path)}
            </span>
            <span className="shrink-0 text-xs tabular-nums text-text-muted">
              {relativeTime(activity.timestamp)}
            </span>
          </div>
        ))
      )}
    </SettingsSection>
  );
}
