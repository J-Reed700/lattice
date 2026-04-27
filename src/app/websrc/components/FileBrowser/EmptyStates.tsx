import Button from '../ui/Button/Button';

interface FilterEmptyStateProps {
  onClearFilters: () => void;
}

/**
 * Filter-empty state — §6.4. Shown inside the body when filters narrow to zero
 * rows. For a truly empty corpus (N=0), see CorpusIdentityBand's empty mode.
 */
export function FilterEmptyState({ onClearFilters }: FilterEmptyStateProps) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 px-6 py-16 text-center">
      <p className="text-sm text-[hsl(var(--text-tertiary))]">No documents match.</p>
      <p className="max-w-xs text-xs text-[hsl(var(--text-muted))]">
        Try clearing a filter or adjusting your search.
      </p>
      <Button
        variant="ghost"
        size="sm"
        onClick={onClearFilters}
        className="mt-2 text-[hsl(var(--text-secondary))]"
      >
        Clear filters
      </Button>
    </div>
  );
}

interface ErrorStateProps {
  message: string;
  onRetry: () => void;
}

export function ErrorState({ message, onRetry }: ErrorStateProps) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 px-6 py-16 text-center">
      <p className="text-sm font-medium text-[hsl(var(--danger-fg))]">Failed to load documents.</p>
      <p className="max-w-sm text-xs text-[hsl(var(--text-tertiary))]">{message}</p>
      <Button
        variant="ghost"
        size="sm"
        onClick={onRetry}
        className="mt-2 text-[hsl(var(--text-secondary))]"
      >
        Try again
      </Button>
    </div>
  );
}
