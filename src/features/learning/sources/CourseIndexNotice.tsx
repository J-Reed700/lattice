import { Link } from 'react-router';

import type { CourseIndexStatusDto } from '@/lib/bindings';

import { useCourseIndexStatus, useCourseIndexStatusEvents } from './useCourseIndexStatus';

/** Where the embedding model is chosen. */
export const EMBEDDING_SETTINGS = '/settings?tab=downloaded-models';

const count = (value: number) => value.toLocaleString();

/** The notice's words for a status; `null` when the course is ready. Exported for tests. */
export function describeCourseIndex(status: CourseIndexStatusDto | null): string | null {
  switch (status?.state) {
    case 'indexing': {
      const pace = status.passagesPerSecond ? ` · ${Math.round(status.passagesPerSecond).toLocaleString()} passages/s` : '';
      if (status.total > 0) return `Indexing references · ${count(status.embedded)} of ${count(status.total)} passages${pace}`;
      if (status.referencesNew > 0) return `Indexing ${count(status.referencesNew)} new ${status.referencesNew === 1 ? 'reference' : 'references'}`;
      return 'Indexing references';
    }
    case 'noEmbeddingModel':
      return 'References are searched by words only. Choose an embedding model in Settings to search by meaning';
    case 'unavailable':
      return `References are searched by words only. ${status.reason}`;
    default:
      return null;
  }
}

/**
 * One quiet line in the course workspace while the course's references are
 * not searched by meaning: while they index (evidence actions wait for it,
 * and say so), without an embedding model, or when the index cannot be read.
 * A ready course shows nothing.
 */
export function CourseIndexNotice({ programId }: { programId: string }) {
  useCourseIndexStatusEvents(programId);
  const status = useCourseIndexStatus(programId);
  const words = describeCourseIndex(status);
  if (!status || !words) return null;
  const indexing = status.state === 'indexing';
  return (
    <p role="status" className="mt-2 flex items-center gap-2 rounded-xl border border-border bg-surface px-3 py-2 text-xs tabular-nums text-text-muted">
      {indexing && <span aria-hidden="true" className="h-1.5 w-1.5 shrink-0 animate-pulse rounded-full bg-[hsl(var(--accent)/0.7)]" />}
      <span className="min-w-0">
        <span className="text-text-secondary">{words}</span>
        {indexing && <> — lesson preparation, evidence checks and source search wait until it finishes</>}
        {status.state === 'noEmbeddingModel' && <> · <Link to={EMBEDDING_SETTINGS} className="text-accent underline underline-offset-2">Open model settings</Link></>}
      </span>
    </p>
  );
}
