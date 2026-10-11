import { useEffect, useRef, useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { AlertCircle, ArrowUpRight, Check, CheckCircle2, ChevronDown, Combine, Loader2, RotateCcw, X } from 'lucide-react';
import { useNavigate } from 'react-router';

import type { SynthesisStage } from '@/lib/bindings';

import { dismissSynthesis, isSynthesisLive, rememberSynthesis, retrySynthesis, synthesisKeys, useJournalSyntheses, type JournalSynthesisDto } from './api';
import { applySynthesis } from './runSynthesis';
import { setSaving, useSynthesisPanel, type SavedSynthesis } from './synthesisPanel';

type PanelStage = SynthesisStage | 'saving';

const stages: Array<{ id: PanelStage; label: string }> = [
  { id: 'gathering', label: 'Gather sources' },
  { id: 'reading', label: 'Review material' },
  { id: 'writing', label: 'Write synthesis' },
  { id: 'saving', label: 'Save to Journal' },
];

/** What the panel shows: one synthesis, or the last one it saved. */
type PanelView =
  | { kind: 'live'; synthesis: JournalSynthesisDto }
  | { kind: 'saving'; synthesis: JournalSynthesisDto; error?: string }
  | { kind: 'ready'; synthesis: JournalSynthesisDto }
  | { kind: 'failed'; synthesis: JournalSynthesisDto }
  | { kind: 'saved'; saved: SavedSynthesis };

/** Saving first, then what is still being written, then what waits to be saved, then what failed. */
export function panelView(
  syntheses: readonly JournalSynthesisDto[],
  saving: Readonly<Record<string, { error?: string }>>,
  saved: SavedSynthesis | null,
): PanelView | null {
  const beingSaved = syntheses.find(synthesis => saving[synthesis.job.id]);
  if (beingSaved) return { kind: 'saving', synthesis: beingSaved, error: saving[beingSaved.job.id]?.error };
  const live = syntheses.find(isSynthesisLive);
  if (live) return { kind: 'live', synthesis: live };
  const ready = syntheses.find(synthesis => synthesis.job.status === 'completed');
  if (ready) return { kind: 'ready', synthesis: ready };
  const failed = syntheses.find(synthesis => synthesis.job.status !== 'completed');
  if (failed) return { kind: 'failed', synthesis: failed };
  return saved ? { kind: 'saved', saved } : null;
}

function stageOf(view: PanelView): PanelStage {
  if (view.kind === 'saved' || view.kind === 'saving' || view.kind === 'ready') return 'saving';
  return view.synthesis.activity?.stage ?? 'gathering';
}

function activityLabel(view: PanelView): string {
  if (view.kind === 'saved' || view.kind === 'ready') return 'Synthesis ready';
  if (view.kind === 'saving') return view.error ? 'Synthesis needs saving' : 'Saving to Journal';
  if (view.kind === 'failed') return 'Synthesis interrupted';
  const { job, activity } = view.synthesis;
  if (job.status === 'pending') return job.progressMessage;
  if (activity?.stage === 'reading') return activity.chunkCount && activity.chunkCount > 1
    ? `Reviewing part ${activity.chunkIndex ?? 1} of ${activity.chunkCount}` : 'Reviewing the material';
  if (activity?.stage === 'writing') return 'Writing your synthesis';
  return 'Gathering conversation context';
}

function elapsedTime(milliseconds: number): string {
  const seconds = Math.max(0, Math.floor(milliseconds / 1000));
  return seconds < 60 ? `${seconds}s` : `${Math.floor(seconds / 60)}m ${String(seconds % 60).padStart(2, '0')}s`;
}

function timesOf(view: PanelView): { startedAt: number; finishedAt?: number; entryCount?: number } {
  if (view.kind === 'saved') return view.saved;
  const { job, activity } = view.synthesis;
  return { startedAt: job.startedAt ?? job.createdAt, finishedAt: job.finishedAt ?? undefined, entryCount: activity?.entryCount ?? undefined };
}

/**
 * Always available across routes, with an explicit compact mode for continued
 * work. Reads the syntheses' jobs, and saves each one this window started as
 * soon as it finishes.
 */
export function SynthesisProgress() {
  const client = useQueryClient();
  const { data: syntheses = [] } = useJournalSyntheses();
  const { minimized, setMinimized, saving, saved, autoSave } = useSynthesisPanel();
  const navigate = useNavigate();
  const [now, setNow] = useState(Date.now);
  const toggleButton = useRef<HTMLButtonElement>(null);
  const restoreToggleFocus = useRef(false);
  const view = panelView(syntheses, saving, saved);
  const running = view?.kind === 'live' || (view?.kind === 'saving' && !view.error);
  const failedSave = view?.kind === 'saving' ? view.error : undefined;
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
  }, [running]);
  // A synthesis started here is saved the moment it finishes.
  useEffect(() => {
    for (const synthesis of syntheses) {
      if (synthesis.job.status === 'completed' && autoSave.includes(synthesis.job.id) && !saving[synthesis.job.id]) {
        void applySynthesis(synthesis, client);
      }
    }
  }, [autoSave, client, saving, syntheses]);

  if (!view) return null;
  const label = activityLabel(view);
  const { startedAt, finishedAt, entryCount } = timesOf(view);
  const elapsed = (running ? now : finishedAt ?? now) - startedAt;
  const stageIndex = stages.findIndex(stage => stage.id === stageOf(view));
  const failed = view.kind === 'failed' || Boolean(failedSave);
  const StatusIcon = running ? Loader2 : failed ? AlertCircle : CheckCircle2;
  const iconClass = `h-4 w-4 shrink-0 ${running ? 'animate-spin motion-reduce:animate-none' : ''} ${failed ? 'text-danger-fg' : 'text-accent'}`;
  const title = view.kind === 'saved' ? view.saved.title : view.synthesis.title;
  const dismiss = () => {
    if (view.kind === 'saved') {
      useSynthesisPanel.setState({ saved: null, minimized: false });
      return;
    }
    if (view.kind === 'failed') {
      const jobId = view.synthesis.job.id;
      void dismissSynthesis(jobId).then(() =>
        client.setQueryData<JournalSynthesisDto[]>(synthesisKeys.list, list => list?.filter(item => item.job.id !== jobId)));
    }
  };
  const retry = () => {
    if (view.kind === 'saving') {
      setSaving(view.synthesis.job.id, null);
      void applySynthesis(view.synthesis, client);
    } else if (view.kind === 'failed') {
      void retrySynthesis(view.synthesis.job.id).then((attempt) => {
        useSynthesisPanel.setState(panel => ({ autoSave: [...panel.autoSave, attempt.job.id] }));
        client.setQueryData<JournalSynthesisDto[]>(synthesisKeys.list, list => rememberSynthesis(list, attempt));
      });
    }
  };
  const canDismiss = view.kind === 'saved' || view.kind === 'failed';

  return (
    <aside aria-label="Conversation synthesis" className="fixed bottom-4 right-4 z-40 max-w-[calc(100vw-2rem)] text-text-primary">
      <p role="status" aria-live="polite" aria-atomic="true" className="sr-only">{label} · {title}</p>
      {minimized ? (
        <button ref={toggleButton} type="button" onClick={() => showDetails(true)} aria-label={`Show synthesis progress: ${label}`}
          className="flex max-w-full items-center gap-2 rounded-xl border border-border-default bg-surface-raised px-3 py-2.5 text-sm shadow-lg focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring">
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
              <p className="mt-0.5 truncate text-xs text-text-secondary" title={title}>{title}</p>
            </div>
            {(running || canDismiss || view.kind === 'ready' || failedSave) && (
              <button ref={toggleButton} type="button" onClick={() => canDismiss ? dismiss() : showDetails(false)}
                aria-label={canDismiss ? 'Dismiss synthesis progress' : 'Minimize synthesis progress'}
                className="-mr-1 -mt-1 rounded-md p-1.5 text-text-muted hover:bg-surface hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring">
                {canDismiss ? <X className="h-4 w-4" /> : <ChevronDown className="h-4 w-4" />}
              </button>
            )}
          </div>
          <div className="border-t border-border-subtle px-4 py-3">
            {view.kind === 'saved' ? (
              <p className="flex items-start gap-2 wrap-break-word text-sm"><StatusIcon className={iconClass} aria-hidden="true" /><span className="min-w-0">Saved to <strong className="font-medium">{view.saved.noteTitle || 'your journal'}</strong>.</span></p>
            ) : view.kind === 'ready' ? (
              <p className="text-sm">Finished while you were away. Save it to the page it was started for.</p>
            ) : (
              <ol aria-label="Synthesis stages" className="space-y-2">
                {stages.map((stage, index) => {
                  const completed = index < stageIndex;
                  const current = index === stageIndex;
                  return (
                    <li key={stage.id} aria-current={current ? 'step' : undefined} className={`flex items-center gap-2 text-xs ${current ? 'font-medium text-text-primary' : 'text-text-muted'}`}>
                      {completed ? <Check className="h-3.5 w-3.5 text-accent" aria-hidden="true" />
                        : current ? <StatusIcon className={`${iconClass} h-3.5! w-3.5!`} aria-hidden="true" />
                          : <span className="flex h-3.5 w-3.5 items-center justify-center" aria-hidden="true"><span className="h-1.5 w-1.5 rounded-full bg-border-strong" /></span>}
                      <span>{current && running ? label : stage.label}</span>
                      {completed && <span className="sr-only">complete</span>}
                    </li>
                  );
                })}
              </ol>
            )}
            {failed && (
              <div role="alert" className="mt-3 max-h-32 overflow-y-auto wrap-break-word text-xs text-danger-fg">
                <p>{failedSave ?? (view.kind === 'failed' ? view.synthesis.job.error ?? 'The synthesis stopped before it finished.' : '')}</p>
                {failedSave && <p className="mt-1 text-text-secondary">Your synthesis is kept here while you retry saving.</p>}
              </div>
            )}
            {running && <p className="mt-3 text-xs leading-relaxed text-text-secondary">
              {view.kind === 'saving' ? 'Saving the finished synthesis and its sources.'
                : elapsed >= 60_000 ? 'Still working. Larger conversations and local models can take several minutes.'
                  : 'This can take a few minutes. You can keep using Lattice, or close it: the synthesis carries on when it opens again.'}
            </p>}
          </div>
          <div className="flex items-center justify-between gap-3 border-t border-border-subtle px-4 py-3">
            <span className="text-xs tabular-nums text-text-muted" aria-hidden="true">{elapsedTime(elapsed)} {running ? 'elapsed' : 'total'}{entryCount ? ` · ${entryCount} ${entryCount === 1 ? 'entry' : 'entries'}` : ''}</span>
            {running ? <button type="button" onClick={() => showDetails(false)} className="rounded-sm text-xs font-medium text-accent hover:underline focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring">Keep working</button>
              : view.kind === 'saved' ? <button type="button" onClick={() => {
                navigate(`/journals?${new URLSearchParams({ noteId: view.saved.noteId }).toString()}`);
                dismiss();
              }} className="inline-flex items-center gap-1 rounded-md bg-accent px-2.5 py-1.5 text-xs font-medium text-accent-fg hover:bg-accent-hover focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring">Open journal page<ArrowUpRight className="h-3.5 w-3.5" aria-hidden="true" /></button>
                : view.kind === 'ready' ? <button type="button" onClick={() => void applySynthesis(view.synthesis, client)} className="inline-flex items-center gap-1 rounded-md bg-accent px-2.5 py-1.5 text-xs font-medium text-accent-fg hover:bg-accent-hover focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring">Save to Journal</button>
                  : <button type="button" onClick={retry} className="inline-flex items-center gap-1 rounded-md bg-accent px-2.5 py-1.5 text-xs font-medium text-accent-fg hover:bg-accent-hover focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"><RotateCcw className="h-3.5 w-3.5" aria-hidden="true" />{failedSave ? 'Retry saving' : 'Try again'}</button>}
          </div>
        </div>
      )}
    </aside>
  );
}
