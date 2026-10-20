import { Link } from 'react-router';

import type { CourseIndexStatusDto } from '@/lib/bindings';

import { describeIndexCounts, describeTimeLeft, indexPercent, type IndexingStatus } from './courseIndexProgress';
import { useCourseIndexStatus, useCourseIndexStatusEvents } from './useCourseIndexStatus';

/** Where the embedding model is chosen. */
export const EMBEDDING_SETTINGS = '/settings?tab=downloaded-models';

/** The notice's first line for a status; `null` when the course is ready. Exported for tests. */
export function describeCourseIndex(status: CourseIndexStatusDto | null): string | null {
  switch (status?.state) {
    case 'indexing':
      return status.waitingTurn ? 'Waiting to index this course’s references' : 'Indexing this course’s references';
    case 'noEmbeddingModel':
      return 'References are searched by words only. Choose an embedding model in Settings to search by meaning';
    case 'unavailable':
      return `References are searched by words only. ${status.reason}`;
    default:
      return null;
  }
}

/** What indexing means for the course meanwhile, after its counts. Exported for tests. */
export function describeIndexingMeanwhile(status: IndexingStatus): string {
  const meanwhile = 'Search covers the whole course by its words and sharpens as passages embed; lesson preparation and verification wait until it finishes.';
  return status.waitingTurn ? `Another course’s references are indexing first, and this course starts when they finish. ${meanwhile}` : meanwhile;
}

/** How far the course's passages are embedded: filled while counted, pulsing before. */
function IndexBar({ status }: { status: IndexingStatus }) {
  if (status.total <= 0) {
    if (status.waitingTurn) return null;
    return <div aria-hidden="true" className="mt-2.5 h-1.5 animate-pulse rounded-full bg-accent/30 motion-reduce:animate-none" />;
  }
  const percent = indexPercent(status.embedded, status.total);
  return (
    <div role="progressbar" aria-label="Passages embedded" aria-valuemin={0} aria-valuemax={status.total} aria-valuenow={Math.min(status.embedded, status.total)} aria-valuetext={describeIndexCounts(status) ?? undefined}
      className="mt-2.5 h-1.5 overflow-hidden rounded-full bg-border">
      <div className="h-full rounded-full bg-accent transition-[width] duration-300 motion-reduce:transition-none" style={{ width: `${percent}%` }} />
    </div>
  );
}

/**
 * The course's index while its references are not searched by meaning, at
 * the top of the course workspace: while they index, a bar of the passages
 * embedded, their counts and the time left at the build's measured pace
 * (search reads their words at once; lesson preparation waits, and says so
 * beside each held action); without an embedding model; or when the index
 * cannot be read. A ready course shows nothing. Each embedded batch moves
 * the build's job, which reads the status again.
 */
export function CourseIndexNotice({ programId }: { programId: string }) {
  useCourseIndexStatusEvents(programId);
  const status = useCourseIndexStatus(programId);
  const words = describeCourseIndex(status);
  if (!status || !words) return null;
  const frame = 'mt-3 rounded-xl border border-accent/30 bg-accent/5 px-4 py-3';
  if (status.state !== 'indexing') {
    return (
      <p role="status" className={`${frame} text-sm leading-6 text-text-primary`}>
        {words}
        {status.state === 'noEmbeddingModel' && <> · <Link to={EMBEDDING_SETTINGS} className="text-accent underline underline-offset-2">Open model settings</Link></>}
      </p>
    );
  }
  const counts = describeIndexCounts(status);
  const left = describeTimeLeft(status);
  return (
    <section role="status" aria-label="Course index" className={frame}>
      <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
        <p className="flex min-w-0 items-center gap-2 text-sm font-medium text-text-primary">
          {!status.waitingTurn && <span aria-hidden="true" className="h-1.5 w-1.5 shrink-0 animate-pulse rounded-full bg-accent motion-reduce:animate-none" />}
          {words}
        </p>
        {/* The counts move with every batch; only a change of state is announced. */}
        {left && <p aria-live="off" className="text-sm tabular-nums text-text-secondary">{left}</p>}
      </div>
      <IndexBar status={status} />
      <p className="mt-2 text-xs leading-5 text-text-secondary">
        {counts && <><span aria-live="off" className="font-medium tabular-nums text-text-primary">{counts}</span> · </>}
        {describeIndexingMeanwhile(status)}
      </p>
    </section>
  );
}
