/**
 * LocalModelCardSkeleton
 *
 * Geometric placeholder mirroring the real {@link LocalModelCard} footprint
 * (header + 3 metadata rows + action footer). Reserves the exact slot so
 * real cards animate in without the surrounding grid jumping.
 *
 * Pulse animation is capped at ~40% opacity so loading feels calm rather
 * than nervous, matching the motion tokens.
 */

import Card from '../../ui/Card/Card';

function Bar({
  width,
  height = 'h-3',
}: {
  width: string;
  height?: string;
}) {
  return (
    <div
      className={`${height} ${width} rounded bg-[hsl(var(--text-tertiary)/0.12)]`}
    />
  );
}

export function LocalModelCardSkeleton() {
  return (
    <Card padding="md" className="h-full motion-safe:animate-pulse opacity-60">
      <div className="flex flex-col h-full gap-3">
        <div className="flex items-start justify-between gap-2">
          <div className="flex-1 min-w-0 space-y-2">
            <Bar width="w-12" height="h-3" />
            <Bar width="w-3/4" height="h-4" />
            <Bar width="w-1/2" height="h-3" />
          </div>
        </div>

        <div className="flex-1 space-y-2">
          <Bar width="w-2/3" height="h-3" />
          <Bar width="w-3/5" height="h-3" />
          <Bar width="w-1/2" height="h-3" />
        </div>

        <div className="pt-3 border-t border-[hsl(var(--border-subtle))] flex flex-wrap gap-1.5">
          <Bar width="w-20" height="h-7" />
          <Bar width="w-20" height="h-7" />
          <Bar width="w-20" height="h-7" />
        </div>
      </div>
    </Card>
  );
}
