import { EmptyState } from '@/components/EmptyState/EmptyState';
import { useFileBrowserStore } from '@/features/files/stores/fileBrowserStore';

interface CorpusEmptyStateProps {
  onAddFolder: () => void;
}

/** Nothing indexed at all. One line, one verb. */
export function CorpusEmptyState({ onAddFolder }: CorpusEmptyStateProps) {
  return (
    <div className="rounded-xl border border-border-subtle bg-surface p-6 shadow-sm">
      <EmptyState
        title="Nothing indexed yet."
        action={{ label: 'Add a folder', onClick: onAddFolder }}
      />
    </div>
  );
}

/** Documents exist, but the current search/filter/scope matches none of them. */
export function FilterEmptyState() {
  const searchQuery = useFileBrowserStore((state) => state.searchQuery);
  const setSearchQuery = useFileBrowserStore((state) => state.setSearchQuery);
  return (
    <div className="rounded-xl border border-border-subtle bg-surface p-6 shadow-sm">
      <EmptyState
        title={searchQuery.trim() ? `No files match “${searchQuery.trim()}”.` : 'No files match.'}
        action={searchQuery.trim() ? { label: 'Clear search', onClick: () => setSearchQuery('') } : undefined}
      />
    </div>
  );
}

interface ErrorStateProps {
  message: string;
}

export function ErrorState({ message }: ErrorStateProps) {
  return (
    <div className="rounded-xl border border-border-subtle bg-surface px-6 py-16 text-center shadow-sm">
      <p className="text-sm font-medium text-text-secondary">Couldn&rsquo;t load your files.</p>
      <p className="mt-1 text-xs text-text-muted">{message}</p>
    </div>
  );
}
