import { Link } from 'react-router-dom';

import { useChatEmptyStateStats } from '@/hooks/queries/useChatEmptyStateStats';

/**
 * ChatEmptyStateIngestDelta
 *
 * Subtle one-line readout of lattice size + recent ingest activity,
 * shown under a Chat empty-state headline. Teaches the user that
 * the lattice is the substrate behind the empty prompt — the corpus
 * is already there, ready to be queried.
 *
 * Degrades silently on query failure so the empty state still looks
 * clean when the backend is unreachable.
 */
export function ChatEmptyStateIngestDelta() {
  const { data, isLoading, isError } = useChatEmptyStateStats();

  if (isLoading || isError || !data) {
    return null;
  }

  const { totalDocuments, documentsAddedToday, documentsAddedThisWeek } = data;

  if (totalDocuments === 0) {
    return (
      <p className="mt-3 text-xs text-[hsl(var(--text-muted))]">
        Your lattice is empty.{' '}
        <Link
          to="/ingest"
          className="text-[hsl(var(--accent))] underline-offset-2 hover:underline"
        >
          Add content
        </Link>{' '}
        to start asking questions.
      </p>
    );
  }

  const formatted = (value: number) => value.toLocaleString();

  let deltaFragment: React.ReactNode = null;
  if (documentsAddedToday > 0) {
    deltaFragment = (
      <>
        <span className="font-mono tabular-nums">{formatted(documentsAddedToday)}</span>
        {' '}added today · {' '}
      </>
    );
  } else if (documentsAddedThisWeek > 0) {
    deltaFragment = (
      <>
        <span className="font-mono tabular-nums">{formatted(documentsAddedThisWeek)}</span>
        {' '}added this week · {' '}
      </>
    );
  }

  return (
    <p className="mt-3 text-xs text-[hsl(var(--text-muted))]">
      {deltaFragment}
      <span className="font-mono tabular-nums">{formatted(totalDocuments)}</span>
      {' '}
      {totalDocuments === 1 ? 'document' : 'documents'} in your lattice ·{' '}
      <Link
        to="/files"
        className="text-[hsl(var(--accent))] underline-offset-2 hover:underline"
      >
        Browse
      </Link>
    </p>
  );
}
