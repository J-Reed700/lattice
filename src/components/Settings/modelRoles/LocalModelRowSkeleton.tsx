/**
 * LocalModelRowSkeleton
 *
 * Flat placeholder matching the real {@link LocalModelRow} height so the
 * list doesn't jump when the data lands. No shimmer, no cascade.
 */

function Bar({ width, height = 'h-3' }: { width: string; height?: string }) {
  return <div className={`${height} ${width} rounded-sm bg-[hsl(var(--text-tertiary)/0.12)]`} />;
}

export function LocalModelRowSkeleton() {
  return (
    <div className="flex items-center gap-4 border-b border-border-subtle py-3 motion-safe:animate-pulse">
      <div className="min-w-0 flex-1 space-y-1.5">
        <Bar width="w-48" height="h-3.5" />
        <Bar width="w-64" height="h-3" />
      </div>
      <Bar width="w-40" height="h-3" />
      <div className="flex shrink-0 gap-1">
        <Bar width="w-12" height="h-7" />
        <Bar width="w-14" height="h-7" />
      </div>
    </div>
  );
}
