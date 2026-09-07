import { Link } from 'react-router';

import { useChatEmptyStateStats } from '@/hooks/queries/useChatEmptyStateStats';

/**
 * ChatEmptyStateIngestDelta
 *
 * One quiet line of library size and recent ingest activity, shown under a
 * Chat empty state. Degrades to nothing when the backend is unreachable.
 */
export function ChatEmptyStateIngestDelta() {
  const { data, isLoading, isError } = useChatEmptyStateStats();

  if (isLoading || isError || !data) {
    return null;
  }

  const { totalDocuments, documentsAddedToday, documentsAddedThisWeek } = data;

  if (totalDocuments === 0) {
    return (
      <p className="mt-3 text-xs text-text-muted">
        Nothing indexed yet.{' '}
        <Link to="/ingest" className="text-accent underline-offset-2 hover:underline">
          Add a folder
        </Link>{' '}
        to start.
      </p>
    );
  }

  const formatted = (value: number) => value.toLocaleString();

  let deltaFragment: React.ReactNode = null;
  if (documentsAddedToday > 0) {
    deltaFragment = (
      <>
        <span className="tabular-nums">{formatted(documentsAddedToday)}</span>
        {' added today · '}
      </>
    );
  } else if (documentsAddedThisWeek > 0) {
    deltaFragment = (
      <>
        <span className="tabular-nums">{formatted(documentsAddedThisWeek)}</span>
        {' added this week · '}
      </>
    );
  }

  return (
    <p className="mt-3 text-xs text-text-muted">
      {deltaFragment}
      <span className="tabular-nums">{formatted(totalDocuments)}</span>{' '}
      {totalDocuments === 1 ? 'document' : 'documents'} ·{' '}
      <Link to="/files" className="text-accent underline-offset-2 hover:underline">
        Library
      </Link>
    </p>
  );
}
