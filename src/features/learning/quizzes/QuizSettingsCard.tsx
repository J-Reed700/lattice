import { useState } from 'react';

import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/Checkbox';
import { NumberStepper } from '@/components/ui/number-stepper';
import { useOperationIds, useUpdateQuizSettings } from '@/features/learning/quizzes/useQuizzes';
import type { LearningQuizSettingsDto } from '@/lib/bindings';

type Draft = { quizPass: number; testPass: number; examPass: number; writtenAnswers: boolean };

const toDraft = (settings: LearningQuizSettingsDto): Draft => ({
  quizPass: Math.round(settings.quizPass * 100),
  testPass: Math.round(settings.testPass * 100),
  examPass: Math.round(settings.examPass * 100),
  writtenAnswers: settings.writtenAnswers,
});

const fields: { key: 'quizPass' | 'testPass' | 'examPass'; label: string }[] = [
  { key: 'quizPass', label: 'Lesson quiz' },
  { key: 'testPass', label: 'Module test' },
  { key: 'examPass', label: 'Course exam' },
];

const valid = (value: number) => Number.isInteger(value) && value >= 1 && value <= 100;

/** Three pass marks and whether written answers are included: the course's only quiz settings. */
export function QuizSettingsCard({ programId, settings, onClose }: { programId: string; settings: LearningQuizSettingsDto; onClose: () => void }) {
  const [draft, setDraft] = useState(() => toDraft(settings));
  const update = useUpdateQuizSettings();
  const operations = useOperationIds();
  const ok = fields.every(({ key }) => valid(draft[key]));
  const save = async () => {
    const next: LearningQuizSettingsDto = { quizPass: draft.quizPass / 100, testPass: draft.testPass / 100, examPass: draft.examPass / 100, writtenAnswers: draft.writtenAnswers };
    const key = `settings:${JSON.stringify(next)}`;
    try { await update.mutateAsync({ operationId: operations.take(key), programId, settings: next }); operations.done(key); onClose(); } catch { /* shown below */ }
  };
  return <section aria-labelledby="quiz-settings-heading" className="rounded-xl border border-border-subtle bg-surface p-5 shadow-sm">
    <h3 id="quiz-settings-heading" className="font-serif text-xl text-text-primary">Quiz settings for this course</h3>
    <p className="mt-1 text-sm text-text-secondary">Pass marks change the verdict words only. Nothing is ever locked.</p>
    <div className="mt-4 grid gap-3 sm:grid-cols-3">
      {fields.map(({ key, label }) => (
        <div key={key} className="rounded-xl border border-border-subtle bg-bg/50 px-4 py-3">
          <NumberStepper
            label={`${label} pass mark`}
            value={draft[key]}
            min={1}
            max={100}
            step={5}
            suffix="%"
            onValueChange={(value) => setDraft((current) => ({ ...current, [key]: value }))}
            decrementLabel={`Decrease ${label.toLowerCase()} pass mark`}
            incrementLabel={`Increase ${label.toLowerCase()} pass mark`}
            className="w-full"
          />
        </div>
      ))}
    </div>
    {!ok && <p role="alert" className="mt-2 text-sm text-danger-fg">Each pass mark is a whole number from 1 to 100.</p>}
    <div className="mt-4 rounded-xl border border-border-subtle bg-bg/40 px-4 py-3">
      <Checkbox
        checked={draft.writtenAnswers}
        onCheckedChange={(checked) => setDraft((current) => ({ ...current, writtenAnswers: checked }))}
        label="Include written answers (graded by your model)"
        description="Off leaves out written explanations and the model check of near-miss short answers. Every grade still says why."
      />
    </div>
    {update.error && <p role="alert" className="mt-3 text-sm text-danger-fg">{update.error.message}</p>}
    <div className="mt-4 flex justify-end gap-2">
      <Button type="button" variant="outline" onClick={onClose}>Cancel</Button>
      <Button type="button" disabled={!ok || update.isPending} onClick={() => void save()}>{update.isPending ? 'Saving…' : 'Save settings'}</Button>
    </div>
  </section>;
}
