import { useState } from 'react';

import { primaryPill, quietPill } from '@/features/learning/quizzes/RunnerParts';
import { useOperationIds, useUpdateQuizSettings } from '@/features/learning/quizzes/useQuizzes';
import type { LearningQuizSettingsDto } from '@/lib/bindings';

type Draft = { quizPass: string; testPass: string; examPass: string; writtenAnswers: boolean };

const toDraft = (settings: LearningQuizSettingsDto): Draft => ({
  quizPass: String(Math.round(settings.quizPass * 100)),
  testPass: String(Math.round(settings.testPass * 100)),
  examPass: String(Math.round(settings.examPass * 100)),
  writtenAnswers: settings.writtenAnswers,
});

const fields: { key: 'quizPass' | 'testPass' | 'examPass'; label: string }[] = [
  { key: 'quizPass', label: 'Lesson quiz' },
  { key: 'testPass', label: 'Module test' },
  { key: 'examPass', label: 'Course exam' },
];

const valid = (value: string) => /^\d{1,3}$/.test(value) && Number(value) >= 1 && Number(value) <= 100;

/** Three pass marks and whether written answers are included: the course's only quiz settings. */
export function QuizSettingsCard({ programId, settings, onClose }: { programId: string; settings: LearningQuizSettingsDto; onClose: () => void }) {
  const [draft, setDraft] = useState(() => toDraft(settings));
  const update = useUpdateQuizSettings();
  const operations = useOperationIds();
  const ok = fields.every(({ key }) => valid(draft[key]));
  const save = async () => {
    const next: LearningQuizSettingsDto = { quizPass: Number(draft.quizPass) / 100, testPass: Number(draft.testPass) / 100, examPass: Number(draft.examPass) / 100, writtenAnswers: draft.writtenAnswers };
    const key = `settings:${JSON.stringify(next)}`;
    try { await update.mutateAsync({ operationId: operations.take(key), programId, settings: next }); operations.done(key); onClose(); } catch { /* shown below */ }
  };
  return <section aria-labelledby="quiz-settings-heading" className="rounded-2xl border border-border bg-surface p-5 shadow-sm">
    <h3 id="quiz-settings-heading" className="font-serif text-xl text-text-primary">Quiz settings for this course</h3>
    <p className="mt-1 text-sm text-text-secondary">Pass marks change the verdict words only. Nothing is ever locked.</p>
    <div className="mt-4 grid gap-3 sm:grid-cols-3">{fields.map(({ key, label }) => <label key={key} className="block rounded-xl border border-border bg-background/40 px-4 py-3">
      <span className="text-xs font-semibold text-text-secondary">{label} pass mark</span>
      <span className="mt-1 flex items-baseline gap-1">
        <input inputMode="numeric" value={draft[key]} aria-invalid={!valid(draft[key])} onChange={(event) => setDraft((current) => ({ ...current, [key]: event.target.value.trim() }))}
          className="min-h-11 w-20 rounded-lg border border-border bg-surface px-3 font-serif text-xl tabular-nums text-text-primary outline-hidden focus:border-accent focus:ring-2 focus:ring-accent/30" />
        <span className="text-sm text-text-muted">%</span>
      </span>
    </label>)}</div>
    {!ok && <p role="alert" className="mt-2 text-sm text-rose-700 dark:text-rose-300">Each pass mark is a whole number from 1 to 100.</p>}
    <label className="mt-4 flex min-h-11 cursor-pointer items-start gap-3 rounded-xl border border-border px-4 py-3">
      <input type="checkbox" checked={draft.writtenAnswers} onChange={(event) => setDraft((current) => ({ ...current, writtenAnswers: event.target.checked }))} className="mt-1 h-4 w-4 accent-accent" />
      <span><span className="text-sm font-medium text-text-primary">Include written answers (graded by your model)</span>
        <span className="mt-0.5 block text-xs leading-5 text-text-muted">Off leaves out written explanations and the model check of near-miss short answers. Every grade still says why.</span></span>
    </label>
    {update.error && <p role="alert" className="mt-3 text-sm text-rose-700 dark:text-rose-300">{update.error.message}</p>}
    <div className="mt-4 flex justify-end gap-2">
      <button type="button" onClick={onClose} className={quietPill}>Cancel</button>
      <button type="button" disabled={!ok || update.isPending} onClick={() => void save()} className={primaryPill}>{update.isPending ? 'Saving…' : 'Save settings'}</button>
    </div>
  </section>;
}
