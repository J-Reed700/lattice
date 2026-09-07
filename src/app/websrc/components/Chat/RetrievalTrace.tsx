import type { RetrievalTrace as RetrievalTraceData } from '@/types/conversation';

/**
 * One muted line saying what the assistant read before it wrote (BRIEF rank 17).
 *
 * The counts come from the backend's own scope set, so "Searched 1,247
 * documents" is the number of documents the hard space filter actually
 * allowed — never the size of the corpus. No trace renders nothing: a turn
 * with no retrieval did not search zero documents, it did not search.
 */

interface RetrievalTraceProps {
  trace: RetrievalTraceData | null | undefined;
}

export function RetrievalTrace({ trace }: RetrievalTraceProps) {
  if (!trace) return null;
  // The backend still sends a trace when it could not search at all — that is
  // how the composer learns *why*. But "Searched 0 documents" reads as a
  // search that came up empty, which is a different and untrue claim, so the
  // line stays off and `ChatModelNotice` says what actually happened.
  if (trace.searchedDocuments === 0 && trace.passages === 0) return null;

  return (
    <p className="mb-2 text-xs text-[hsl(var(--text-muted))]">
      Searched{' '}
      <span className="tabular-nums">{trace.searchedDocuments.toLocaleString()}</span>{' '}
      {trace.searchedDocuments === 1 ? 'document' : 'documents'}
      {trace.passages > 0 && (
        <>
          {' · '}
          <span className="tabular-nums">{trace.passages}</span>{' '}
          {trace.passages === 1 ? 'passage' : 'passages'} from{' '}
          <span className="tabular-nums">{trace.files}</span>{' '}
          {trace.files === 1 ? 'file' : 'files'}
        </>
      )}
      {trace.scope === 'linked' && ' · this conversation only'}
    </p>
  );
}
