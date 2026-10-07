import { useEffect, useState } from 'react';

import { Check, LoaderCircle } from 'lucide-react';

import type { LearningOutlineProgressDto, OutlineStage } from '@/lib/bindings';

const stages: Record<OutlineStage, { step: number; title: string; detail: string }> = {
  reading_sources: { step: 0, title: 'Reading your references', detail: 'Capturing the selected pages and documents for this course.' },
  loading_model: { step: 0, title: 'Getting the model ready', detail: 'Connecting to your selected model before writing starts.' },
  drafting: { step: 1, title: 'Writing your course outline', detail: 'Planning the modules, lesson objectives, prerequisites, and project milestones.' },
  reviewing: { step: 2, title: 'Reviewing the teaching plan', detail: 'Checking the draft for gaps, prerequisite order, and realistic lesson workloads.' },
  researching: { step: 2, title: 'Finding supporting references', detail: 'Searching for missing evidence and saving reference pages before continuing review and repair.' },
  repairing: { step: 2, title: 'Revising the course outline', detail: 'Correcting affected modules and saving each edit before checking it again.' },
  checking_repair: { step: 2, title: 'Checking the revised outline', detail: 'Checking that the requested corrections were made. The outline is not ready yet.' },
  saving: { step: 3, title: 'Saving your outline', detail: 'Saving the syllabus, references, and review findings.' },
  completed: { step: 4, title: 'Your draft is saved', detail: 'Opening the outline and its review findings.' },
  cancelled: { step: 0, title: 'Generation cancelled', detail: 'Your inputs are kept so you can adjust them and try again.' },
  failed: { step: 0, title: 'Generation stopped', detail: 'Your inputs are kept. No unfinished outline will be presented as ready.' },
};

function duration(seconds: number) {
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
}

export function OutlineGenerationProgress({ progress, cancelling, onCancel, onOpenDraft, cancelError, deepDive }: {
  progress?: LearningOutlineProgressDto | null; cancelling?: boolean; onCancel?: () => void; onOpenDraft?: () => void; cancelError?: string; deepDive: boolean;
}) {
  const [startedAt] = useState(() => Date.now());
  const [elapsed, setElapsed] = useState(0);
  useEffect(() => {
    const timer = window.setInterval(() => setElapsed(Math.floor((Date.now() - startedAt) / 1000)), 1000);
    return () => window.clearInterval(timer);
  }, [startedAt]);
  const stage = progress?.stage;
  const current = stage ? stages[stage] : { step: 0, title: 'Starting your course', detail: 'Waiting for the app to begin reading your references.' };
  const modelStep = stage && ['drafting', 'reviewing', 'repairing', 'checking_repair'].includes(stage);
  const stopping = cancelling && stage !== 'saving' && stage !== 'completed';

  return <section aria-label="Course generation progress" className="mt-8 rounded-xl border border-accent/30 bg-accent/5 p-4 sm:p-5">
    <div className="flex flex-col items-start justify-between gap-3 sm:flex-row">
      <div className="min-w-0 flex-1"><div className="flex items-center gap-2 text-sm font-semibold text-text-primary" role="status"><LoaderCircle size={18} className="shrink-0 animate-spin text-accent" aria-hidden="true" />{stopping ? 'Cancelling generation…' : current.title}</div><p className="mt-2 text-sm leading-6 text-text-secondary">{stopping ? 'Stopping this request. Your inputs and completed draft corrections stay saved.' : current.detail}</p></div>
      <span className="text-xs tabular-nums text-text-muted" aria-live="off">Elapsed <span aria-label="Elapsed time">{duration(Math.max(elapsed, progress?.elapsedSeconds ?? 0))}</span></span>
    </div>
    <ol aria-label="Generation stages" className="mt-5 grid gap-3 text-xs sm:grid-cols-4">{['Read references', 'Write outline', 'Review and revise', 'Save outline'].map((label, index) => <li key={label} aria-current={index === current.step ? 'step' : undefined} className={`flex items-center gap-2 ${index === current.step ? 'font-semibold text-accent' : 'text-text-muted'}`}><span className="grid h-5 w-5 shrink-0 place-items-center rounded-full border border-current">{index < current.step ? <Check size={12} aria-label="Complete" /> : index + 1}</span>{label}</li>)}</ol>
    {modelStep && <div className="mt-4 rounded-lg bg-background/70 p-3 text-xs leading-5 text-text-secondary"><p>{progress!.responseCharacters > 0 ? `Response so far · ${progress!.responseCharacters.toLocaleString()} characters received in this step` : 'Waiting for the model’s response. It may still be processing the request.'}</p>{progress?.modelName && <p className="mt-1 wrap-break-word text-text-muted">Model: {progress.modelName}</p>}{(progress?.stageSeconds ?? 0) >= 90 && <p className="mt-2">This step is taking a while. You can keep waiting or cancel and try a faster model or a focused course.</p>}</div>}
    <p className="mt-4 text-xs leading-5 text-text-muted">{deepDive ? 'A deep dive plans 24–60 lessons, then reviews the whole syllabus. ' : 'Writing and reviewing use separate model calls. '}There is no overall time limit or per-step deadline. You can cancel; completed draft corrections stay saved.</p>
    {progress?.programId && <p className="mt-3 text-sm text-text-secondary">A draft is saved in your programs. {onOpenDraft && <button type="button" onClick={onOpenDraft} className="font-semibold text-accent underline">Inspect saved draft</button>}</p>}
    {onCancel && <button type="button" onClick={onCancel} disabled={cancelling || stage === 'saving' || stage === 'completed'} className="mt-4 min-h-10 rounded-lg border border-border bg-background px-4 text-sm font-medium text-text-primary hover:bg-surface disabled:opacity-50">{stopping ? 'Cancelling…' : 'Cancel generation'}</button>}
    {cancelError && <p role="alert" className="mt-3 text-sm text-rose-700">Could not cancel: {cancelError}</p>}
  </section>;
}
