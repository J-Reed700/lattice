import { useEffect, useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { LoaderCircle } from 'lucide-react';


import { useCancelLearningGenerationJob, useRetryLearningGenerationJob } from '@/features/learning/curriculum/useLearningPlan';
import { LEARNING_PROGRAMS_KEY, learningProgramKey } from '@/features/learning/workspace/useLearningStudio';
import type { LearningGenerationJob } from '@/lib/bindings';

export function LessonPreparationProgress({ job, pending, programId, revision, error }: {
  job?: LearningGenerationJob; pending: boolean; programId: string; revision: number; error?: string;
}) {
  const client = useQueryClient();
  const cancel = useCancelLearningGenerationJob();
  const retry = useRetryLearningGenerationJob();
  const [now, setNow] = useState(Date.now());
  const active = job ? ['pending', 'running'].includes(job.status) : pending;
  useEffect(() => {
    if (!active) return;
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [active]);
  useEffect(() => {
    if (job?.status !== 'completed') return;
    void client.invalidateQueries({ queryKey: learningProgramKey(programId) });
    void client.invalidateQueries({ queryKey: LEARNING_PROGRAMS_KEY });
  }, [client, job?.id, job?.status, programId]);
  if (!active && (!job || job.status === 'completed')) return null;
  const elapsed = Math.max(0, Math.floor((now - (job?.startedAt ?? job?.createdAt ?? now)) / 1000));
  const request = () => ({ operationId: crypto.randomUUID(), programId, jobId: job!.id, expectedRevision: revision });
  const actionError = cancel.error ?? retry.error;
  return <section aria-label="Lesson preparation progress" className="my-4 rounded-xl border border-accent/30 bg-accent/5 p-5">
    <div className="flex flex-wrap items-start justify-between gap-3">
      <div><h3 className="flex items-center gap-2 text-sm font-semibold text-text-primary" role="status">{active && <LoaderCircle size={18} className="animate-spin text-accent" aria-hidden="true" />}{active ? 'Preparing your lesson' : `Lesson preparation ${job?.status}`}</h3>
        <p className="mt-2 text-sm leading-6 text-text-secondary">{active ? job?.progressMessage || 'Starting lesson preparation…' : job?.error || error || 'Your course is saved. You can retry this lesson.'}</p>
      </div>
      {active && job && <span className="text-xs tabular-nums text-text-muted" aria-live="off">Elapsed {Math.floor(elapsed / 60)}:{String(elapsed % 60).padStart(2, '0')}</span>}
    </div>
    {active && <p className="mt-3 text-xs leading-5 text-text-muted">Preparation includes writing, reviewing the teaching and answer keys, and checking factual claims against your references. You can browse other modules while it runs.</p>}
    {job && <button type="button" disabled={cancel.isPending || retry.isPending} onClick={() => active ? cancel.mutate(request()) : retry.mutate(request())} className="mt-4 rounded-full border border-border bg-background px-4 py-2 text-sm font-medium text-text-primary disabled:opacity-50">{cancel.isPending ? 'Cancelling…' : retry.isPending ? 'Retrying…' : active ? 'Cancel lesson preparation' : 'Retry lesson preparation'}</button>}
    {actionError && <p role="alert" className="mt-3 text-sm text-rose-700">{actionError.message}</p>}
  </section>;
}
