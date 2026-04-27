interface CorpusGroupHeaderProps {
  label: string;
  count: number;
}

const GROUP_HEADER_HEIGHT = 32;

export const GROUP_HEADER_HEIGHT_PX = GROUP_HEADER_HEIGHT;

/**
 * Sticky group heading — §6.3. Renders within a virtualized list via absolute
 * positioning; stickiness is handled by the virtualizer offset math.
 */
export function CorpusGroupHeader({ label, count }: CorpusGroupHeaderProps) {
  return (
    <div
      className="flex items-center justify-between border-b border-[hsl(var(--border-subtle))] bg-[hsl(var(--bg))] px-4"
      style={{ height: GROUP_HEADER_HEIGHT }}
    >
      <span className="text-xxs font-medium uppercase tracking-[0.08em] text-[hsl(var(--text-muted))]">
        {label}
      </span>
      <span className="text-xxs tabular-nums text-[hsl(var(--text-muted))]">
        {count.toLocaleString()} {count === 1 ? 'document' : 'documents'}
      </span>
    </div>
  );
}
