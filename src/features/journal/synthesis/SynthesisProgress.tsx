import { useEffect, useRef, useState } from 'react';

import { AlertCircle, ArrowUpRight, Check, CheckCircle2, ChevronDown, Combine, Loader2, RotateCcw, X } from 'lucide-react';
import { useNavigate } from 'react-router';

import { useSynthesisStore, type SynthesisJob, type SynthesisStage } from './synthesisStore';

const stages: Array<{ id: SynthesisStage; label: string }> = [
  { id: 'gathering', label: 'Gather sources' },
  { id: 'reading', label: 'Review material' },
  { id: 'writing', label: 'Write synthesis' },
  { id: 'saving', label: 'Save to Journal' },
];

function activityLabel(job: SynthesisJob): string {
  if (job.status === 'completed') return 'Synthesis ready';
  if (job.status === 'failed') return job.stage === 'saving' ? 'Synthesis needs saving' : 'Synthesis interrupted';
  if (job.stage === 'reading') return job.chunkCount && job.chunkCount > 1
    ? `Reviewing part ${job.chunkIndex ?? 1} of ${job.chunkCount}` : 'Reviewing the material';
  if (job.stage === 'writing') return 'Writing your synthesis';
  if (job.stage === 'saving') return 'Saving to Journal';
  return 'Gathering conversation context';
}

function elapsedTime(milliseconds: number): string {
  const seconds = Math.max(0, Math.floor(milliseconds / 1000));
  return seconds < 60 ? `${seconds}s` : `${Math.floor(seconds / 60)}m ${String(seconds % 60).padStart(2, '0')}s`;
}

/** Always available across routes, with an explicit compact mode for continued work. */
export function SynthesisProgress() {
  const { job, minimized, setMinimized, dismiss } = useSynthesisStore();
  const navigate = useNavigate();
  const [now, setNow] = useState(Date.now);
  const toggleButton = useRef<HTMLButtonElement>(null);
  const restoreToggleFocus = useRef(false);
  const running = job?.status === 'running';
  const showDetails = (show: boolean) => {
    restoreToggleFocus.current = true;
    setMinimized(!show);
  };
  useEffect(() => {
    if (!restoreToggleFocus.current) return;
    restoreToggleFocus.current = false;
    toggleButton.current?.focus();
  }, [minimized]);
  useEffect(() => {
    if (!running) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [running, job?.id]);

  if (!job) return null;
  const label = activityLabel(job);
  const elapsed = (job.finishedAt ?? now) - job.startedAt;
  const stageIndex = stages.findIndex(stage => stage.id === job.stage);
  const StatusIcon = running ? Loader2 : job.status === 'completed' ? CheckCircle2 : AlertCircle;
  const iconClass = `h-4 w-4 shrink-0 ${running ? 'animate-spin motion-reduce:animate-none' : ''} ${job.status === 'failed' ? 'text-danger-fg' : 'text-accent'}`;

  return (
    <aside aria-label="Conversation synthesis" className="fixed bottom-4 right-4 z-40 max-w-[calc(100vw-2rem)] text-text-primary">
      <p role="status" aria-live="polite" aria-atomic="true" className="sr-only">{label} · {job.title}</p>
      {minimized ? (
        <button ref={toggleButton} type="button" onClick={() => showDetails(true)} aria-label={`Show synthesis progress: ${label}`}
          className="flex max-w-full items-center gap-2 rounded-xl border border-border-default bg-surface-raised px-3 py-2.5 text-sm shadow-lg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
          <StatusIcon className={iconClass} aria-hidden="true" />
          <span className="truncate">{running ? 'Synthesizing…' : label}</span>
          <span className="text-xs tabular-nums text-text-muted" aria-hidden="true">{elapsedTime(elapsed)}</span>
        </button>
      ) : (
        <div className="max-h-[calc(100dvh-2rem)] w-[340px] max-w-full overflow-y-auto rounded-xl border border-border-default bg-surface-raised shadow-xl">
          <div className="flex items-start gap-2.5 px-4 pb-3 pt-4">
            <Combine className="mt-0.5 h-4 w-4 shrink-0 text-accent" aria-hidden="true" />
            <div className="min-w-0 flex-1">
              <h2 className="text-sm font-semibold">{running ? 'Synthesizing to Journal' : label}</h2>
              <p className="mt-0.5 truncate text-xs text-text-secondary" title={job.title}>{job.title}</p>
            </div>
            <button ref={toggleButton} type="button" onClick={() => running ? showDetails(false) : dismiss()}
              aria-label={running ? 'Minimize synthesis progress' : 'Dismiss synthesis progress'}
              className="-mr-1 -mt-1 rounded-md p-1.5 text-text-muted hover:bg-surface hover:text-text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
              {running ? <ChevronDown className="h-4 w-4" /> : <X className="h-4 w-4" />}
            </button>
          </div>
          <div className="border-t border-border-subtle px-4 py-3">
            {job.status === 'completed' ? (
              <p className="flex items-start gap-2 break-words text-sm"><StatusIcon className={iconClass} aria-hidden="true" /><span className="min-w-0">Saved to <strong className="font-medium">{job.noteTitle || 'your journal'}</strong>.</span></p>
            ) : (
              <ol aria-label="Synthesis stages" className="space-y-2">
                {stages.map((stage, index) => {
                  const completed = index < stageIndex;
                  const current = index === stageIndex;
                  return (
                    <li key={stage.id} aria-current={current ? 'step' : undefined} className={`flex items-center gap-2 text-xs ${current ? 'font-medium text-text-primary' : 'text-text-muted'}`}>
                      {completed ? <Check className="h-3.5 w-3.5 text-accent" aria-hidden="true" />
                        : current ? <StatusIcon className={`${iconClass} !h-3.5 !w-3.5`} aria-hidden="true" />
                          : <span className="flex h-3.5 w-3.5 items-center justify-center" aria-hidden="true"><span className="h-1.5 w-1.5 rounded-full bg-border-strong" /></span>}
                      <span>{current && running ? label : stage.label}</span>
                      {completed && <span className="sr-only">complete</span>}
                    </li>
                  );
                })}
              </ol>
            )}
            {job.status === 'failed' && (
              <div role="alert" className="mt-3 max-h-32 overflow-y-auto break-words text-xs text-danger-fg">
                <p>{job.error}</p>
                {job.stage === 'saving' && <p className="mt-1 text-text-secondary">Your synthesis is kept here while you retry saving.</p>}
              </div>
            )}
            {running && <p className="mt-3 text-xs leading-relaxed text-text-secondary">
              {job.stage === 'saving' ? 'Saving the finished synthesis and its sources.'
                : elapsed >= 60_000 ? 'Still working. Larger conversations and local models can take several minutes.'
                  : 'This can take a few minutes. You can keep using Lattice while it runs.'}
            </p>}
          </div>
          <div className="flex items-center justify-between gap-3 border-t border-border-subtle px-4 py-3">
            <span className="text-xs tabular-nums text-text-muted" aria-hidden="true">{elapsedTime(elapsed)} {running ? 'elapsed' : 'total'}{job.entryCount ? ` · ${job.entryCount} ${job.entryCount === 1 ? 'entry' : 'entries'}` : ''}</span>
            {running ? <button type="button" onClick={() => showDetails(false)} className="rounded-sm text-xs font-medium text-accent hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">Keep working</button>
              : job.status === 'completed' ? <button type="button" onClick={() => {
                navigate(`/journals?${new URLSearchParams({ noteId: job.noteId! }).toString()}`);
                dismiss();
              }} className="inline-flex items-center gap-1 rounded-md bg-accent px-2.5 py-1.5 text-xs font-medium text-accent-fg hover:bg-accent-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">Open journal page<ArrowUpRight className="h-3.5 w-3.5" aria-hidden="true" /></button>
                : <button type="button" onClick={() => void job.retry?.()} className="inline-flex items-center gap-1 rounded-md bg-accent px-2.5 py-1.5 text-xs font-medium text-accent-fg hover:bg-accent-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"><RotateCcw className="h-3.5 w-3.5" aria-hidden="true" />{job.stage === 'saving' ? 'Retry saving' : 'Try again'}</button>}
          </div>
        </div>
      )}
    </aside>
  );
}
