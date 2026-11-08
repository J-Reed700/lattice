import { LoaderCircle } from 'lucide-react';

import { Button } from '@/components/ui/button';

import type { Building } from './drillState';

/**
 * A drill is being asked for or written. A build takes a minute or two, and the bank keeps the next drill of the
 * kind ready for the course after the first, so only the first start of one waits.
 */
export function DrillBuilding({ building, onStop }: { building: Building | null; onStop: () => void }) {
  if (!building) {
    return (
      <p role="status" className="flex items-center gap-3 text-sm text-text-secondary">
        <LoaderCircle size={16} className="animate-spin motion-reduce:animate-none" aria-hidden="true" />
        Starting your drill…
      </p>
    );
  }
  const share = building.total > 0 ? Math.min(100, Math.round((building.current / building.total) * 100)) : null;
  return (
    <section aria-labelledby="drill-building-title" className="rounded-2xl border border-border bg-surface p-6">
      <div role="status" aria-live="polite">
        <div className="flex items-center gap-3">
          <LoaderCircle size={18} className="shrink-0 animate-spin text-accent motion-reduce:animate-none" aria-hidden="true" />
          <h2 id="drill-building-title" className="font-serif text-xl text-text-primary">Writing a drill for you, usually a minute or two</h2>
        </div>
        {building.message && <p className="mt-3 text-sm text-text-secondary">{building.message}</p>}
        {building.error && (
          <>
            <p className="mt-2 text-sm text-warning-fg">The last try failed, so it is trying again.</p>
            <p className="mt-1 text-xs text-text-secondary">{building.error}</p>
          </>
        )}
      </div>
      {share !== null && (
        <div
          role="progressbar"
          aria-label="Drill build progress"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={share}
          className="mt-4 h-1.5 overflow-hidden rounded-full bg-surface-raised"
        >
          <div className="h-full rounded-full bg-accent transition-[width]" style={{ width: `${share}%` }} />
        </div>
      )}
      <p className="mt-4 text-xs leading-5 text-text-muted">After the first, the app keeps the next drill of this kind ready for the course, so later starts are usually instant.</p>
      <Button type="button" variant="ghost" className="-ml-3 mt-4" onClick={onStop}>Stop waiting</Button>
    </section>
  );
}
