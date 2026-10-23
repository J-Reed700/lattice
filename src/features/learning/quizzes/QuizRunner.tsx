import { useId, useState } from 'react';

import { ArrowLeft, ArrowRight, Eye, RotateCcw, Send } from 'lucide-react';

import { TiptapViewer } from '@/components/TiptapEditor';
import { itemRegistry } from '@/features/learning/quizzes/items/itemRegistry';
import { isAnswered, type FormItem, type ItemDraft } from '@/features/learning/quizzes/items/itemResponse';
import { coverageWords, formScopeKind, points, scopeHeadline, scopeKindLabel } from '@/features/learning/quizzes/quizCopy';
import { QuizSourceLine } from '@/features/learning/quizzes/QuizSourceLine';
import { BackLink, ConfirmDialog, eyebrow, parchmentBand, primaryPill, QuestionRail, quietPill, SaveStatus } from '@/features/learning/quizzes/RunnerParts';
import { useQuizAutosave } from '@/features/learning/quizzes/useQuizAutosave';
import { useOperationIds, useSubmitQuiz } from '@/features/learning/quizzes/useQuizzes';
import type { LearningAssessmentFormDto, LearningQuizCoverageDto, LearningSourceDto } from '@/lib/bindings';

function errorText(error: unknown) {
  return error instanceof Error ? error.message : 'Something went wrong. Your answers are still here.';
}

function RunnerItem({ item, index, draft, practice, disabled, sources, onChange, onCheck }: {
  item: FormItem; index: number; draft: ItemDraft; practice: boolean; disabled: boolean; sources: LearningSourceDto[];
  onChange: (patch: Partial<ItemDraft>) => void; onCheck: () => void;
}) {
  const promptId = useId();
  const renderer = itemRegistry[item.format];
  const { Editor, Review } = renderer;
  return <article aria-labelledby={promptId}>
    <div className="flex flex-wrap items-center gap-2">
      <span className="text-[11px] font-semibold uppercase tracking-[.15em] text-text-muted">Question {index + 1} · {renderer.label}</span>
      {item.previouslyExposed && <span className="rounded-full bg-background px-2.5 py-0.5 text-[11px] font-medium text-text-secondary">Seen before · a repeat from an earlier attempt</span>}
      {item.format === 'explanation' && <span className="text-[11px] text-text-muted">· worth {points(item.points)} {item.points === 1 ? 'point' : 'points'}</span>}
    </div>
    {!renderer.ownsPrompt && <div id={promptId} className="mt-3 max-w-3xl font-serif text-[1.4rem] leading-snug text-text-primary"><TiptapViewer content={item.prompt} /></div>}
    <div className={renderer.ownsPrompt ? 'mt-3' : 'mt-6'}><Editor item={item} draft={draft} disabled={disabled} promptId={promptId} onChange={onChange} /></div>
    {practice && item.keyReveal && <section aria-label="Feedback on this answer" className="mt-6 rounded-2xl border border-accent/25 bg-accent/5 p-4 sm:p-5">
      <div className={eyebrow}>Feedback · practice</div>
      <div className="mt-3"><Review item={item} draft={draft} reveal={item.keyReveal} result={null} /></div>
    </section>}
    {practice && !item.keyReveal && isAnswered(item, draft) && item.format !== 'explanation' && <button type="button" onClick={onCheck} className={`${quietPill} mt-5`}><Eye size={15} aria-hidden="true" /> Check my answer</button>}
    {practice && item.format === 'explanation' && <p className="mt-4 text-xs text-text-muted">Written answers get their feedback when you finish the round.</p>}
    <QuizSourceLine ids={item.sourceVersionIds} sources={sources} />
  </article>;
}

/**
 * Taking a quiz: header band, question rail, one question at a time with
 * autosave, then submit or pause through the alert dialogs.
 */
export function QuizRunner({ form, sources, coverage, onExit, onSubmitted, reload }: {
  form: LearningAssessmentFormDto; sources: LearningSourceDto[]; coverage?: LearningQuizCoverageDto | null;
  onExit: () => void; onSubmitted: (form: LearningAssessmentFormDto) => void; reload: () => Promise<LearningAssessmentFormDto | undefined>;
}) {
  const autosave = useQuizAutosave(form, form.programId, reload);
  const submit = useSubmitQuiz();
  const operations = useOperationIds();
  const [index, setIndex] = useState(() => Math.max(0, form.items.findIndex((item) => !isAnswered(item, autosave.drafts[item.id]))));
  const [confirm, setConfirm] = useState<'submit' | 'pause' | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const kind = formScopeKind(form);
  const practice = form.mode === 'practice';
  const total = form.items.length;
  const answered = form.items.filter((item) => isAnswered(item, autosave.drafts[item.id])).length;
  const item = form.items[Math.min(index, total - 1)];
  const busy = submit.isPending;

  const leave = async () => { if (await autosave.flush()) onExit(); };
  const doSubmit = async () => {
    setActionError(null);
    if (!(await autosave.flush())) { setConfirm(null); return; }
    const key = `submit:${form.id}:${autosave.revision()}`;
    try {
      const submitted = await submit.mutateAsync({ operationId: operations.take(key), programId: form.programId, formId: form.id, expectedRevision: autosave.revision() });
      operations.done(key);
      setConfirm(null);
      onSubmitted(submitted);
    } catch (error) { setActionError(errorText(error)); setConfirm(null); }
  };
  const go = (next: number) => setIndex(Math.max(0, Math.min(total - 1, next)));

  return <div className="space-y-3">
    <BackLink label="Quizzes" onClick={() => void leave()} />
    <section className="overflow-hidden rounded-2xl border border-border bg-surface shadow-sm" aria-label={`${scopeKindLabel[kind]}: ${form.title}`}>
      <header className={`${parchmentBand} px-5 py-5 sm:px-8`}>
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div className="min-w-0">
            <div className={eyebrow}>{practice ? 'Practice · ' : ''}{scopeKindLabel[kind]} · {form.title}</div>
            <h2 className="mt-1 font-serif text-[1.75rem] leading-tight text-text-primary">{practice ? 'Practice round' : scopeHeadline[kind]}</h2>
            <p className="mt-1.5 max-w-2xl text-sm leading-6 text-text-secondary">{practice
              ? 'Ungraded. Check each answer as you go; nothing here counts toward your result.'
              : 'Keys are shown after you submit. You can pause and resume any time.'}</p>
            {coverage && kind !== 'lesson' && <p className="mt-1 text-xs font-medium text-text-secondary">{coverageWords(coverage, kind)}.</p>}
          </div>
          <div className="shrink-0 text-right">
            <p className="text-sm font-semibold tabular-nums text-text-primary">{answered} of {total} answered</p>
            <p className="mt-0.5 text-xs text-text-muted">About {form.expectedMinutes} min</p>
            <div className="mt-1"><SaveStatus state={autosave.state} /></div>
          </div>
        </div>
      </header>
      <div className="px-5 py-6 sm:px-8 sm:py-8">
        <QuestionRail items={form.items} drafts={autosave.drafts} current={index} onSelect={go} />
        <p className="sr-only" aria-live="polite">Question {index + 1} of {total}</p>
        <div className="mt-7">{item && <RunnerItem key={item.id} item={item} index={index} draft={autosave.drafts[item.id]} practice={practice} disabled={busy}
          sources={sources} onChange={(patch) => autosave.update(item.id, patch)} onCheck={() => void autosave.flush()} />}</div>
      </div>
      <footer className="border-t border-border px-5 py-4 sm:px-8">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <button type="button" disabled={index === 0} onClick={() => go(index - 1)} className={quietPill}><ArrowLeft size={15} aria-hidden="true" /> Previous</button>
          <div className="flex flex-wrap gap-2">
            {index < total - 1 && <button type="button" onClick={() => go(index + 1)} className={quietPill}>Next <ArrowRight size={15} aria-hidden="true" /></button>}
            <button type="button" disabled={busy || answered !== total} onClick={() => setConfirm('submit')} className={primaryPill}><Send size={15} aria-hidden="true" /> {practice ? 'Finish practice' : 'Submit answers'}</button>
          </div>
        </div>
        <p className="mt-2 flex flex-wrap items-center justify-end gap-x-2 text-xs text-text-muted">
          {answered !== total && <span>Answer all {total} questions before submitting ·</span>}
          <button type="button" onClick={() => setConfirm('pause')} className="min-h-11 font-medium text-text-secondary underline-offset-2 hover:text-text-primary hover:underline">Pause</button>
        </p>
        {(autosave.saveError || actionError) && <div role="alert" className="mt-3 flex flex-wrap items-center justify-between gap-3 rounded-xl border border-rose-500/20 bg-rose-500/5 px-4 py-3 text-sm text-rose-800 dark:text-rose-200">
          <span>{actionError ?? autosave.saveError}</span>
          {autosave.conflict
            ? <button type="button" onClick={() => void autosave.reloadKeepingAnswers()} className={quietPill}>Reload · keep my answers</button>
            : <button type="button" onClick={() => void (actionError ? doSubmit() : autosave.retry())} className={quietPill}><RotateCcw size={14} aria-hidden="true" /> Try again</button>}
        </div>}
      </footer>
    </section>
    <ConfirmDialog open={confirm === 'submit'} pending={busy} onCancel={() => setConfirm(null)} onConfirm={() => void doSubmit()}
      title={practice ? 'Finish this practice round?' : 'Submit your answers?'} confirmLabel={practice ? 'Finish' : 'Submit now'}
      body={practice ? 'You will see every answer with its key. Nothing here counts toward your result.' : 'Your responses will be frozen and graded. You can start a clearly labelled retake later.'} />
    <ConfirmDialog open={confirm === 'pause'} pending={false} onCancel={() => setConfirm(null)} onConfirm={() => { setConfirm(null); void leave(); }}
      title="Pause this quiz?" confirmLabel="Pause" body="Your answers are saved. Resume any time from the lesson or the Quizzes tab; nothing is graded until you submit." />
  </div>;
}
