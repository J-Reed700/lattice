import { LoaderCircle } from 'lucide-react';

import { useAuditLessonNow, useLessonAudit, useProgramVerifierAudit } from '@/features/learning/lessons/review/useVerifierAudit';
import type { LearningLessonAuditDto, LearningLessonAuditResultDto } from '@/lib/bindings';

const tokens = (count: number) => count.toLocaleString();

/** "Auditing with <model> · batch 3 of 7 · 48,200 tokens sent" (§6.1, §14.4). */
export function auditLine(audit: LearningLessonAuditDto): string {
  const sent = `${tokens(audit.inputTokens)} tokens sent · ${tokens(audit.outputTokens)} received`;
  switch (audit.state) {
    case 'running': return `Auditing with ${audit.verifierModel} · ${audit.batchesPlanned ? `batch ${Math.min(audit.batchesDone + 1, audit.batchesPlanned)} of ${audit.batchesPlanned}` : 'planning batches'} · ${sent}`;
    case 'completed': return `Audit: ${audit.batchesDone} ${audit.batchesDone === 1 ? 'batch' : 'batches'} · ${sent} · ${audit.verifierModel}`;
    case 'stopped': return `Audit stopped after batch ${audit.batchesDone} of ${audit.batchesPlanned} · ${sent}`;
  }
}

/** The lesson's latest Verifier audit, if it has one. */
export function LessonAuditLine({ programId, lessonId }: { programId: string; lessonId: string }) {
  const audit = useLessonAudit(programId, lessonId).data;
  if (!audit) return null;
  return (
    <div className="mt-2 text-xs leading-5 text-text-muted">
      <p className="tabular-nums">{auditLine(audit)}</p>
      {audit.lastError && <p>Last error: {audit.lastError}</p>}
      {audit.webNote && <p>{audit.webNote}</p>}
    </div>
  );
}

function resultLine(result: LearningLessonAuditResultDto): string {
  if (result.disputes > 0) return `The Verifier disputes ${result.disputes} ${result.disputes === 1 ? 'claim' : 'claims'}; the lesson needs your review.`;
  if (result.outcome === 'deferred') return result.message ?? 'The audit is waiting to retry.';
  if (result.outcome === 'stopped') return [result.message, result.fix && `Fix: ${result.fix}`].filter(Boolean).join(' ') || 'The audit stopped.';
  return result.message ?? 'The Verifier agrees with every claim it checked.';
}

/** "Audit this lesson now" on a ready lesson of a course that opted in (§14.3). */
export function AuditLessonNow({ programId, lessonId }: { programId: string; lessonId: string }) {
  const optIn = useProgramVerifierAudit(programId).data;
  const run = useAuditLessonNow(programId, lessonId);
  if (!optIn?.on) return null;
  return (
    <div className="mb-5 flex flex-wrap items-center gap-3 rounded-xl border border-border bg-surface p-4 text-xs">
      <button type="button" disabled={run.isPending || Boolean(optIn.blocked)} onClick={() => run.mutate()}
        className="inline-flex min-h-9 items-center gap-2 rounded-full border border-accent/40 px-4 font-medium text-accent disabled:opacity-50">
        {run.isPending && <LoaderCircle size={13} className="animate-spin motion-reduce:animate-none" aria-hidden="true" />}
        {run.isPending ? 'Auditing…' : 'Audit this lesson now'}
      </button>
      <span className="text-text-muted">{optIn.blocked ?? `Sends this lesson to ${optIn.destination ?? 'your Verifier'}; a dispute sends it back to Needs your review.`}</span>
      {run.data && <p role="status" className="w-full text-text-secondary">{resultLine(run.data)}</p>}
      {run.error && <p role="alert" className="w-full text-rose-700">{run.error.message}</p>}
      <div className="w-full"><LessonAuditLine programId={programId} lessonId={lessonId} /></div>
    </div>
  );
}
