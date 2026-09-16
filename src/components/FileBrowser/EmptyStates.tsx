import { useFileBrowserStore } from '../../stores/fileBrowserStore';
import { EmptyState } from '../EmptyState/EmptyState';

interface CorpusEmptyStateProps {
  onAddFolder: () => void;
}

/** Nothing indexed at all. One line, one verb. */
export function CorpusEmptyState({ onAddFolder }: CorpusEmptyStateProps) {
  return (
    <EmptyState
      title="Nothing indexed yet."
      action={{ label: 'Add a folder', onClick: onAddFolder }}
    />
  );
}

/** Documents exist, but the current search/filter/scope matches none of them. */
export function FilterEmptyState() {
  const searchQuery = useFileBrowserStore((state) => state.searchQuery);
  const setSearchQuery = useFileBrowserStore((state) => state.setSearchQuery);
  return (
    <EmptyState
      title={searchQuery.trim() ? `No files match “${searchQuery.trim()}”.` : 'No files match.'}
      action={searchQuery.trim() ? { label: 'Clear search', onClick: () => setSearchQuery('') } : undefined}
    />
  );
}

interface ErrorStateProps {
  message: string;
}

export function ErrorState({ message }: ErrorStateProps) {
  return (
    <div className="px-6 py-16 text-center">
      <p className="text-sm text-text-secondary">Couldn&rsquo;t load your files.</p>
      <p className="mt-1 text-xs text-text-muted">{message}</p>
    </div>
  );
}
