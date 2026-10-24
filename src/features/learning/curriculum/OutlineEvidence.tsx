import { ExternalLink, Quote } from 'lucide-react';

import type { OutlineEvidenceQuery } from '@/features/learning/api/evidenceQueries';
import type {
  LearningOutlineCitationDto,
  LearningOutlineEvidenceDto,
} from '@/lib/bindings';

export function OutlineEvidenceStatus({ report }: { report: OutlineEvidenceQuery }) {
  if (report.isError) return (
    <div role="alert" className="my-3 rounded-lg border border-rose-500/20 bg-rose-500/5 p-3 text-sm text-rose-700 dark:text-rose-300">
      <p>Outline citations could not be loaded. {report.error.message}</p>
      <button type="button" disabled={report.isFetching} onClick={() => void report.refetch()} className="mt-2 underline disabled:opacity-50">
        {report.isFetching ? 'Retrying outline citations…' : 'Retry outline citations'}
      </button>
    </div>
  );
  if (report.isFetching && !report.data) return <p role="status" className="my-3 text-sm text-text-muted">Loading outline citations…</p>;
  return null;
}

export function OutlineCitation({
  citation,
  evidence,
  className = '',
}: {
  citation?: LearningOutlineCitationDto;
  evidence?: LearningOutlineEvidenceDto | null;
  className?: string;
}) {
  if (!citation || !evidence) return null;
  const number = evidence.citations.indexOf(citation) + 1;
  let href: string | undefined;
  try {
    const url = new URL(citation.sourceUrl ?? '');
    if (url.protocol === 'https:' || url.protocol === 'http:') href = url.href;
  } catch {
    // A saved local source has no external link.
  }

  return (
    <details className={`mt-2 wrap-anywhere rounded-lg border border-accent/15 bg-accent/[.035] px-3 py-2 ${className}`}>
      <summary className="flex cursor-pointer list-none items-center gap-2 text-[11px] font-medium text-accent">
        <span className="inline-grid h-5 min-w-5 place-items-center rounded bg-accent/10 px-1 text-[10px] font-semibold">{number}</span>
        <span className="min-w-0 flex-1 truncate">{citation.sourceTitle}</span>
        <span className="text-text-muted">View citation</span>
      </summary>
      <blockquote className="mt-2 whitespace-pre-wrap border-l-2 border-accent/40 pl-3 font-serif text-xs leading-5 text-text-primary">
        {citation.quote}
      </blockquote>
      <p className="mt-2 flex flex-wrap items-center gap-2 text-[10px] text-text-muted">
        <Quote size={11} /> Exact text from saved source {citation.sourceId}
        {href && <a href={href} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1 text-accent underline">Open original <ExternalLink size={10} /></a>}
      </p>
    </details>
  );
}
