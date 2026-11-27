import { ExternalLink } from 'lucide-react';

import type { OutlineEvidenceQuery } from '@/features/learning/api/evidenceQueries';
import { CitationCard } from '@/features/reading/citations';
import type { LearningOutlineCitationDto } from '@/lib/bindings';

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
  onOpen,
  className = '',
}: {
  citation?: LearningOutlineCitationDto;
  onOpen?: (citation: LearningOutlineCitationDto) => void;
  className?: string;
}) {
  if (!citation) return null;
  let href: string | undefined;
  try {
    const url = new URL(citation.sourceUrl ?? '');
    if (url.protocol === 'https:' || url.protocol === 'http:') href = url.href;
  } catch {
    // A saved local source has no external link.
  }
  const anchorExcerpts = citation.support.anchors.map((anchor) => anchor.exact).filter(Boolean);
  const excerpts = anchorExcerpts.length > 0 ? anchorExcerpts : citation.quote.trim() ? [citation.quote] : [];

  return (
    <details className={`mt-2 wrap-anywhere rounded-lg border border-accent/15 bg-accent/[.035] px-3 py-2 ${className}`}>
      <summary className="flex cursor-pointer list-none items-center gap-2 text-[11px] font-medium text-accent">
        <span className="inline-grid h-5 min-w-5 place-items-center rounded bg-accent/10 px-1 text-[10px] font-semibold">{citation.number}</span>
        <span className="min-w-0 flex-1 truncate">{citation.sourceTitle}</span>
        <span className="text-text-muted">View citation</span>
      </summary>
      <div className="mt-2 border-t border-accent/15 pt-2">
        <CitationCard
          number={citation.number}
          title={citation.sourceTitle}
          relation={citation.support.relation}
          excerpts={excerpts}
          onOpen={onOpen ? () => onOpen(citation) : undefined}
          footer={href ? <a href={href} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1 text-xs text-accent underline">Open original <ExternalLink size={10} /></a> : undefined}
        />
      </div>
    </details>
  );
}
