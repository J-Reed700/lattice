import { useEffect, useState } from 'react';

import { Check, LoaderCircle } from 'lucide-react';

import type { LearningOutlineProgressDto, OutlineStage } from '@/lib/bindings';

const stages: Record<OutlineStage, { step: number; title: string; detail: string }> = {
  reading_sources: { step: 0, title: 'Reading your references', detail: 'Capturing the selected pages and documents for this course.' },
  loading_model: { step: 0, title: 'Getting the model ready', detail: 'Connecting to your selected model before writing starts.' },
  gathering_references: { step: 0, title: 'Gathering references', detail: 'Planning the subtopics and saving reference pages before writing the outline.' },
  drafting: { step: 1, title: 'Writing your course outline', detail: 'Planning the modules, lesson objectives, prerequisites, and project milestones.' },
  checking_citations: { step: 2, title: 'Linking references', detail: 'Finding the saved passage that addresses each outline item. The model chooses among retrieved candidates or none; the quoted words are copied from the saved page.' },
  reviewing: { step: 2, title: 'Reviewing the teaching plan', detail: 'Checking the draft for gaps, prerequisite order, and realistic lesson workloads.' },
  researching: { step: 2, title: 'Researching an unresolved finding', detail: 'Searching for evidence on a specific finding and saving the pages before continuing review and repair.' },
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
  const modelStep = stage && ['drafting', 'checking_citations', 'reviewing', 'repairing', 'checking_repair'].includes(stage);
  const stopping = cancelling && stage !== 'saving' && stage !== 'completed';

  return <section aria-label="Course generation progress" className="mt-8 rounded-xl border border-border-default bg-surface p-5 shadow-xs">
    <div className="flex flex-col items-start justify-between gap-3 sm:flex-row">
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2 text-sm font-semibold text-text-primary" role="status">
          <LoaderCircle size={18} className="shrink-0 animate-spin text-accent" aria-hidden="true" />
          {stopping ? 'Cancelling generation…' : current.title}
        </div>
        <p className="mt-2 text-sm leading-relaxed text-text-secondary">
          {stopping ? 'Stopping this request. Your inputs and completed draft corrections stay saved.' : current.detail}
        </p>
        {!stopping && progress?.detail && (
          <p className="mt-1 text-xs text-text-muted" aria-label="Stage progress">{progress.detail}</p>
        )}
      </div>
      <span className="text-xs tabular-nums text-text-muted" aria-live="off">
        Elapsed <span aria-label="Elapsed time">{duration(Math.max(elapsed, progress?.elapsedSeconds ?? 0))}</span>
      </span>
    </div>
    <ol aria-label="Generation stages" className="mt-5 grid gap-3 text-xs sm:grid-cols-4">
      {['Read references', 'Write outline', 'Review and revise', 'Save outline'].map((label, index) => (
        <li
          key={label}
          aria-current={index === current.step ? 'step' : undefined}
          className={`flex items-center gap-2 ${
            index === current.step ? 'font-semibold text-accent' : index < current.step ? 'text-text-primary' : 'text-text-muted'
          }`}
        >
          <span className={`grid h-5 w-5 shrink-0 place-items-center rounded-full border ${
            index === current.step ? 'border-accent bg-accent-muted text-accent' : index < current.step ? 'border-accent text-accent' : 'border-border-default text-text-muted'
          }`}>
            {index < current.step ? <Check size={12} aria-label="Complete" /> : index + 1}
          </span>
          {label}
        </li>
      ))}
    </ol>
    {modelStep && (
      <div className="mt-4 rounded-lg border border-border-subtle bg-surface-sunken p-3.5 text-xs leading-relaxed text-text-secondary">
        <p>
          {progress!.responseCharacters > 0
            ? `Response so far · ${progress!.responseCharacters.toLocaleString()} characters received in this step`
            : 'Waiting for the model’s response. It may still be processing the request.'}
        </p>
        {progress?.modelName && (
          <p className="mt-1 wrap-break-word text-text-muted">Model: {progress.modelName}</p>
        )}
        {(progress?.stageSeconds ?? 0) >= 90 && (
          <p className="mt-2 text-warning-fg">
            This step is taking a while. You can keep waiting or cancel and try a faster model or a focused course.
          </p>
        )}
      </div>
    )}
    <p className="mt-4 text-xs leading-relaxed text-text-muted">
      {deepDive ? 'A deep dive plans 24–60 lessons, then reviews the whole syllabus. ' : 'Writing and reviewing use separate model calls. '}
      There is no overall time limit or per-step deadline. You can cancel; completed draft corrections stay saved.
    </p>
    {progress?.programId && (
      <p className="mt-3 text-sm text-text-secondary">
        A draft is saved in your programs.{' '}
        {onOpenDraft && (
          <button type="button" onClick={onOpenDraft} className="font-semibold text-accent underline">
            Inspect saved draft
          </button>
        )}
      </p>
    )}
    {onCancel && (
      <button
        type="button"
        onClick={onCancel}
        disabled={cancelling || stage === 'saving' || stage === 'completed'}
        className="mt-4 min-h-10 rounded-lg border border-border-default bg-surface px-4 text-sm font-medium text-text-primary shadow-xs transition hover:bg-surface-raised disabled:opacity-50"
      >
        {stopping ? 'Cancelling…' : 'Cancel generation'}
      </button>
    )}
    {cancelError && <p role="alert" className="mt-3 text-sm text-danger-fg">Could not cancel: {cancelError}</p>}
  </section>;
}
