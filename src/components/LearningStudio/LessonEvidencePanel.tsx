import { useState } from 'react';

import { useQuery } from '@tanstack/react-query';
import { FileSearch, ExternalLink } from 'lucide-react';

import VaultAPI from '@/lib/api';

function safeUrl(value: string | null) {
  try { const url = new URL(value ?? ''); return ['https:', 'http:'].includes(url.protocol) ? url.href : undefined; }
  catch { return undefined; }
}

export function LessonEvidencePanel({ programId, lessonId }: { programId: string; lessonId: string }) {
  const [open, setOpen] = useState(false);
  const report = useQuery({
    queryKey: ['learning-lesson-evidence', programId, lessonId],
    queryFn: async () => {
      const result = await VaultAPI.getLearningLessonEvidence(programId, lessonId);
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
    enabled: open, retry: false, staleTime: 0,
  });
  const data = report.data;
  return <section className="mb-5 rounded-xl border border-border bg-surface p-4" aria-label="Lesson evidence">
    <button type="button" aria-expanded={open} onClick={() => setOpen(!open)} className="flex min-h-10 w-full items-center gap-2 text-left text-sm font-semibold text-accent"><FileSearch size={17} />{open ? 'Hide lesson evidence' : 'View lesson evidence'}</button>
    {open && <div className="mt-3 space-y-4">
      {report.isFetching && <p role="status" className="text-sm text-text-muted">Loading the saved verification report…</p>}
      {report.error && <div role="alert" className="text-sm text-rose-700"><p>{report.error.message}</p><button type="button" onClick={() => void report.refetch()} className="mt-2 underline">Retry evidence</button></div>}
      {report.isSuccess && !data && <p className="text-sm text-text-secondary">This lesson has no saved verification report. New lessons prepared with references record their evidence here.</p>}
      {data && <>
        <p className="text-sm leading-6 text-text-secondary">{data.claimCount} claims checked against saved evidence · {data.executedExamples} examples executed. These checks reduce errors; they do not guarantee that a lesson is correct or complete.</p>
        <p className="text-xs leading-5 text-text-muted">{data.retrievalMode === 'hybrid' ? 'Semantic and keyword retrieval' : data.retrievalMode === 'lexical_fallback' ? 'Keyword retrieval — embeddings were unavailable' : 'Earlier verification method'} · Checked {new Date(data.checkedAt).toLocaleString()} · {data.checkerModel}</p>
        {data.unexecutedLanguages.length > 0 && <p className="rounded-lg bg-amber-500/10 p-3 text-sm text-text-secondary">Examples in {data.unexecutedLanguages.join(', ')} were checked against references but were not compiled or executed.</p>}
        {!data.sourcesCurrent && <p role="status" className="rounded-lg bg-amber-500/10 p-3 text-sm text-text-secondary">The reference collection has changed since this check. The evidence below belongs to the saved lesson and its original source versions.</p>}
        <details><summary className="cursor-pointer text-xs text-text-muted">Verification details</summary><dl className="mt-2 space-y-1 break-all text-xs text-text-secondary"><div><dt className="inline font-medium">Policy: </dt><dd className="inline">{data.policy}</dd></div><div><dt className="inline font-medium">Lesson fingerprint: </dt><dd className="inline">{data.contentSha256}</dd></div>{data.embeddingModel && <div><dt className="inline font-medium">Embedding model: </dt><dd className="inline">{data.embeddingModel}</dd></div>}</dl></details>
        <div className="space-y-2">{data.teachingClaims.map((claim, index) => <details key={index} className="rounded-lg border border-border p-3"><summary className="cursor-pointer text-sm text-text-primary"><span className="mr-2 text-xs text-text-muted">Section {claim.sectionIndex + 1}</span>{claim.claim}</summary><p className="mt-3 text-xs leading-5 text-text-secondary">{claim.reason}</p>{claim.supportingQuote && <blockquote className="mt-2 whitespace-pre-wrap border-l-2 border-accent pl-3 text-sm text-text-primary">{claim.supportingQuote}</blockquote>}<div className="mt-3 space-y-2">{claim.passages.map((passage, passageIndex) => { const url = safeUrl(passage.url); return <details key={passageIndex} className="rounded-lg bg-background p-3"><summary className="cursor-pointer text-xs font-medium text-text-secondary">{passage.title}</summary><p className="mt-2 break-all text-[10px] text-text-muted">Saved version {passage.sourceVersionId} · {passage.retrievalKind.replace(/_/g, ' ')}</p><pre className="mt-2 whitespace-pre-wrap break-words font-sans text-xs leading-5 text-text-secondary">{passage.text}</pre>{url && <a href={url} target="_blank" rel="noreferrer" className="mt-2 inline-flex items-center gap-1 text-xs text-accent underline">Open original page <ExternalLink size={12} /></a>}</details>; })}</div></details>)}</div>
      </>}
    </div>}
  </section>;
}
