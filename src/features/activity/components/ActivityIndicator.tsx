import { useCallback, useState } from 'react';

import { Activity } from 'lucide-react';

import { cn } from '@/lib/utils';

import { useActivitySnapshot } from '../api';
import { ActivityDrawer } from './ActivityDrawer';
import { activityLabel } from '../model/activityFormat';


const EMPTY: never[] = [];

/**
 * Sidebar-rail affordance beside Downloads for everything running in the
 * background: a count of running items and a calm dot while anything runs.
 * Always shown, so "what is this waiting on?" has one place to look; it owns
 * the one snapshot subscription and the drawer it opens.
 */
export function ActivityIndicator() {
  const { snapshot, readAt, failed, refresh } = useActivitySnapshot();
  const [open, setOpen] = useState(false);
  const items = snapshot?.items ?? EMPTY;
  const running = snapshot?.running ?? 0;
  const label = activityLabel(running, items.length);
  const close = useCallback(() => setOpen(false), []);

  const toggle = () => {
    setOpen((value) => !value);
    // Read again on opening, for anything that moved without an event.
    if (!open) void refresh();
  };

  return (
    <>
      <button
        type="button"
        onClick={toggle}
        title={label}
        aria-label={label}
        aria-expanded={open}
        aria-haspopup="dialog"
        className={cn(
          'relative flex min-h-[44px] min-w-[44px] items-center justify-center rounded-md p-3 transition-colors duration-fast',
          'focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring',
          open ? 'bg-accent-muted text-accent' : 'text-text-muted hover:bg-surface-raised hover:text-text-primary',
        )}
      >
        <Activity className="h-[18px] w-[18px]" strokeWidth={1.75} aria-hidden="true" />
        {running > 0 && (
          <>
            <span
              aria-hidden="true"
              className="absolute right-1 top-1 flex h-4 min-w-4 items-center justify-center rounded-full bg-accent px-1 text-[10px] font-medium leading-none text-bg"
            >
              {running > 9 ? '9+' : running}
            </span>
            <span
              aria-hidden="true"
              data-testid="activity-busy-dot"
              className="absolute bottom-2 right-2 h-1.5 w-1.5 rounded-full bg-accent motion-safe:animate-pulse"
            />
          </>
        )}
      </button>
      {/* What the trigger says, for a screen reader that is elsewhere. */}
      <span role="status" aria-live="polite" className="sr-only">
        {label}
      </span>
      <ActivityDrawer open={open} onClose={close} items={items} readAt={readAt} failed={failed} />
    </>
  );
}
