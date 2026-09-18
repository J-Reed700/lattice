import type { RetrievalTrace as RetrievalTraceData } from '@/types/conversation';

/**
 * A compact summary of the sources retrieved for a response.
 *
 * The counts come from the backend's own scope set, so "Searched 1,247
 * documents" is the number of documents the hard space filter actually
 * allowed — never the size of the corpus. No trace renders nothing: a turn
 * with no retrieval did not search zero documents, it did not search.
 */

interface RetrievalTraceProps {
  trace: RetrievalTraceData | null | undefined;
}

/**
 * The second line, when retrieval did something worth saying out loud.
 *
 * Only two things qualify: a corrective pass (the first ranking was judged
 * insufficient and the planner was asked again) and a skipped planner (a short
 * follow-up reused the previous turn's topic). A plain sufficient verdict is
 * the expected case and says nothing, so it draws nothing — a badge on every
 * ordinary answer is noise, not disclosure.
 */
function retrievalNote(trace: RetrievalTraceData): string | null {
  const retried = (trace.kbCorrectiveRetries ?? 0) > 0;
  const skipped = trace.kbPlannerSkipped === true;
  if (retried && skipped) {
    return 'Reused the previous topic, then searched again for more support';
  }
  if (retried) {
    return 'First results looked thin, so it searched again';
  }
  if (skipped) {
    return 'Reused the previous topic instead of planning a new search';
  }
  return null;
}

export function RetrievalTrace({ trace }: RetrievalTraceProps) {
  if (!trace) return null;
  // The backend still sends a trace when it could not search at all — that is
  // how the composer learns *why*. But "Searched 0 documents" reads as a
  // search that came up empty, which is a different and untrue claim, so the
  // line stays off and `ChatModelNotice` says what actually happened.
  if (
    trace.searchedDocuments === 0 &&
    trace.passages === 0 &&
    trace.files === 0 &&
    !trace.webPages
  ) {
    return null;
  }

  const note = retrievalNote(trace);

  return (
    <>
    <p className={`text-xs text-[hsl(var(--text-muted))] ${note ? 'mb-0.5' : 'mb-2'}`}>
      {trace.searchedDocuments > 0 ? <>
        Searched{' '}
        <span className="tabular-nums">{trace.searchedDocuments.toLocaleString()}</span>{' '}
        {trace.searchedDocuments === 1 ? 'document' : 'documents'}
      </> : 'Retrieved'}
      {trace.passages > 0 && (
        <>
          {trace.searchedDocuments > 0 ? ' · ' : ' '}
          <span className="tabular-nums">{trace.passages}</span>{' '}
          {trace.passages === 1 ? 'passage' : 'passages'} from{' '}
          <span className="tabular-nums">{trace.files}</span>{' '}
          {trace.files === 1 ? 'file' : 'files'}
        </>
      )}
      {trace.passages === 0 && trace.files > 0 && <>
        {trace.searchedDocuments > 0 ? ' · retrieved ' : ' '}
        <span className="tabular-nums">{trace.files}</span>{' '}
        {trace.files === 1 ? 'file' : 'files'}
      </>}
      {!!trace.webPages && (
        <>
          {trace.searchedDocuments > 0 || trace.passages > 0 || trace.files > 0 ? ' · ' : ' '}
          <span className="tabular-nums">{trace.webPages}</span>{' '}
          {trace.webPages === 1 ? 'web page' : 'web pages'}
        </>
      )}
      {trace.scope === 'linked' && ' · this space'}
    </p>
    {note && <p className="mb-2 text-xs text-[hsl(var(--text-tertiary))]">{note}</p>}
    </>
  );
}
