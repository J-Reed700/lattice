import { Eye, EyeOff } from 'lucide-react';

import { useCitationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import { cn } from '@/lib/utils';

/**
 * One persisted reading preference shared by Chat, Explorer, tangents and
 * Learning Studio. It changes presentation only; evidence remains saved and
 * immediately reappears when the user turns it back on.
 */
export function CitationVisibilityToggle({ className, compact = false }: { className?: string; compact?: boolean }) {
  const visible = useCitationDisplayStore((state) => state.visible);
  const setVisible = useCitationDisplayStore((state) => state.setVisible);

  const nextLabel = visible ? 'Hide citations' : 'Show citations';
  return (
    <button
      type="button"
      aria-pressed={!visible}
      aria-label={nextLabel}
      title={visible ? 'Clean reading: hide citations and evidence highlights' : 'Show citations and evidence highlights'}
      onClick={() => setVisible(!visible)}
      className={cn(
        'inline-flex min-h-8 shrink-0 items-center gap-1.5 whitespace-nowrap rounded-full border border-border-subtle bg-surface px-2.5 text-xs font-medium text-text-secondary shadow-sm transition hover:border-border hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent',
        !visible && 'border-accent/25 bg-accent/5 text-accent',
        className,
      )}
    >
      {visible ? <EyeOff className="h-3.5 w-3.5" aria-hidden="true" /> : <Eye className="h-3.5 w-3.5" aria-hidden="true" />}
      <span className={compact ? 'sr-only' : undefined}>{nextLabel}</span>
    </button>
  );
}
