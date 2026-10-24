import { useState } from 'react';

import { CriterionVerdict, RationaleCriteria } from '@/features/learning/quizzes/GradeRationale';
import { points, type Words } from '@/features/learning/quizzes/quizCopy';
import { VerdictChip } from '@/features/learning/quizzes/QuizVerdict';
import type { LearningGradeContestDto, LearningGradeRationaleDto, LearningSourceDto } from '@/lib/bindings';

const pill = 'inline-flex min-h-11 items-center justify-center gap-2 rounded-full px-4 text-sm font-semibold disabled:opacity-50';

/** "I think I was right": an optional note, then a blind regrade. */
export function ContestForm({ pending, error, onSubmit, onCancel }: { pending: boolean; error?: string; onSubmit: (note: string) => void; onCancel: () => void }) {
  const [note, setNote] = useState('');
  return <form className="rounded-xl border border-border bg-surface p-4" onSubmit={(event) => { event.preventDefault(); onSubmit(note.trim()); }}>
    <label htmlFor="contest-note" className="text-sm font-semibold text-text-primary">What did the grade miss? <span className="font-normal text-text-muted">(optional)</span></label>
    <textarea id="contest-note" rows={3} value={note} onChange={(event) => setNote(event.target.value)} autoFocus
      className="mt-2 w-full rounded-xl border border-border bg-background px-3 py-2 text-sm leading-6 text-text-primary outline-hidden focus:border-accent focus:ring-2 focus:ring-accent/30" />
    <p className="mt-1 text-xs text-text-muted">A fresh grade is made without seeing the first one, and it answers your note.</p>
    {error && <p role="alert" className="mt-2 text-sm text-rose-700 dark:text-rose-300">{error}</p>}
    <div className="mt-3 flex flex-wrap justify-end gap-2">
      <button type="button" onClick={onCancel} className={`${pill} border border-border font-medium text-text-secondary`}>Cancel</button>
      <button type="submit" disabled={pending} className={`${pill} bg-accent text-accent-fg`}>{pending ? 'Asking…' : 'Ask for a regrade'}</button>
    </div>
  </form>;
}

function contestWords(contest: LearningGradeContestDto): Words {
  const first = points(contest.firstScore);
  const regrade = contest.regradeScore === null ? null : points(contest.regradeScore);
  switch (contest.status) {
    case 'open': return { label: 'Regrade in progress', detail: 'A fresh grade, blind to the first one.', tone: 'pending' };
    case 'revised': return { label: 'Regrade raised your score', detail: `First grade ${first} · regrade ${regrade}`, tone: 'passed' };
    case 'upheld': return { label: 'Regrade upheld the grade', detail: `First grade ${first} · regrade ${regrade}`, tone: 'neutral' };
    case 'overridden': return { label: 'Overridden by you', detail: `First grade ${first} · regrade ${regrade} · counted as full credit`, tone: 'override' };
    case 'failed': return { label: "Couldn't regrade yet", detail: 'Your note is kept. Ask again when your model is free.', tone: 'warning' };
  }
}

/** Criteria whose verdict changed between the grades, said in words. */
export function changedCriteria(first: LearningGradeRationaleDto, regrade: LearningGradeRationaleDto) {
  return regrade.criteria.flatMap((after) => {
    const before = first.criteria.find((criterion) => criterion.criterionId === after.criterionId);
    return before && before.verdict !== after.verdict ? [{ title: after.title, before: before.verdict, after: after.verdict }] : [];
  });
}

function Side({ title, rationale, answer, sources }: { title: string; rationale: LearningGradeRationaleDto; answer: string; sources: LearningSourceDto[] }) {
  return <section aria-label={title} className="min-w-0">
    <h6 className="text-[10px] font-semibold uppercase tracking-[.15em] text-text-muted">{title}</h6>
    <p className="mt-1.5 text-sm leading-6 text-text-primary">{rationale.summary}</p>
    <div className="mt-2"><RationaleCriteria rationale={rationale} answer={answer} sources={sources} /></div>
  </section>;
}

/** Both grades side by side, what changed, the regrade's reply, and the override once upheld. */
export function ContestOutcome({ contest, answer, sources, overridePending, overrideError, onOverride }: {
  contest: LearningGradeContestDto; answer: string; sources: LearningSourceDto[]; overridePending: boolean; overrideError?: string; onOverride: () => void;
}) {
  const [confirming, setConfirming] = useState(false);
  const words = contestWords(contest);
  const changes = contest.regradeRationale ? changedCriteria(contest.firstRationale, contest.regradeRationale) : [];
  return <section aria-label="Your regrade request" className="rounded-xl border border-border bg-surface p-4">
    <div className="flex flex-wrap items-center gap-2"><VerdictChip words={words} /><span className="text-xs text-text-muted">{words.detail}</span></div>
    {contest.note && <p className="mt-2 text-xs text-text-secondary"><span className="font-semibold">Your note:</span> “{contest.note}”</p>}
    {contest.responseToNote && <p className="mt-2 text-sm leading-6 text-text-primary"><span className="font-semibold">The regrade&apos;s reply: </span>{contest.responseToNote}</p>}
    {changes.length > 0 && <ul className="mt-2 space-y-1 text-xs text-text-secondary">{changes.map((change) => <li key={change.title} className="flex flex-wrap items-center gap-1.5">
      Changed: <span className="font-semibold text-text-primary">{change.title}</span> <CriterionVerdict verdict={change.before} /> → <CriterionVerdict verdict={change.after} />
    </li>)}</ul>}
    {contest.regradeRationale && <div className="mt-3 grid gap-4 border-t border-border pt-3 lg:grid-cols-2">
      <Side title="First grade" rationale={contest.firstRationale} answer={answer} sources={sources} />
      <Side title="Regrade" rationale={contest.regradeRationale} answer={answer} sources={sources} />
    </div>}
    {contest.status === 'upheld' && !confirming && <button type="button" onClick={() => setConfirming(true)} className={`${pill} mt-3 border border-violet-500/40 font-medium text-violet-900 hover:bg-violet-500/5 dark:text-violet-200`}>Override: count it as right</button>}
    {contest.status === 'upheld' && confirming && <div role="group" aria-label="Confirm override" className="mt-3 rounded-xl bg-violet-500/8 p-3 text-sm leading-6 text-text-secondary">
      <p>Your score will count this answer as full credit and say it includes your override. It is labelled &ldquo;Overridden by you&rdquo; and never counts as mastery evidence.</p>
      {overrideError && <p role="alert" className="mt-1 text-rose-700 dark:text-rose-300">{overrideError}</p>}
      <div className="mt-2 flex flex-wrap gap-2">
        <button type="button" onClick={() => setConfirming(false)} className={`${pill} border border-border font-medium`}>Keep the grade</button>
        <button type="button" disabled={overridePending} onClick={onOverride} className={`${pill} bg-violet-700 text-white`}>{overridePending ? 'Saving…' : 'Override'}</button>
      </div>
    </div>}
  </section>;
}
