import type { ReactNode } from 'react';

import { Button } from '@/components/ui/button';
import type { CourseDrillAttemptDto, DrillLessonDto, NextDrillDto } from '@/lib/bindings';
import { cn } from '@/lib/utils';

import {
  languageLabel,
  sizesFor,
  validMinutes,
  type DrillMode,
  type DrillOptions,
} from './drillOptions';
import { MODE_LABEL, momentOf } from './reveal/revealCopy';
import { useCourseDrills } from './useCourseDrills';
import { useDrillLanguages } from './useDrillLanguages';

const INPUT = 'rounded-lg border border-border-default bg-bg px-3 py-2 text-xs text-text-primary outline-hidden focus:border-accent focus:ring-2 focus:ring-accent-muted';

function Choice({ name, value, checked, disabled = false, onSelect, title, detail }: {
  name: string;
  value: string;
  checked: boolean;
  disabled?: boolean;
  onSelect: () => void;
  title: string;
  detail: ReactNode;
}) {
  return (
    <label className={cn(
      'flex items-start gap-3 rounded-xl border p-4 transition focus-within:ring-2 focus-within:ring-ring',
      disabled ? 'cursor-not-allowed opacity-60' : 'cursor-pointer',
      checked ? 'border-accent bg-accent-muted' : cn('border-border-subtle', !disabled && 'hover:border-border-default'),
    )}>
      <input type="radio" name={name} value={value} checked={checked} disabled={disabled} onChange={onSelect} className="mt-1 accent-[hsl(var(--accent))]" />
      <span className="min-w-0">
        <span className="block text-xs font-semibold text-text-primary">{title}</span>
        <span className="mt-0.5 block text-xs leading-5 text-text-secondary">{detail}</span>
      </span>
    </label>
  );
}

const MODES: readonly { id: DrillMode; title: string; detail: string }[] = [
  { id: 'practice', title: 'Practice', detail: 'Untimed. Take the time to find each issue and say why it matters.' },
  { id: 'interview', title: 'Interview', detail: 'Timed, as in an interview: find the issue that matters most first, and say it clearly.' },
];

const READING = 'Reading the course’s lessons…';
const CODE_DETAIL = 'Code you did not write, built on the lesson’s ideas, as you would review a pull request.';
const PASSAGE_TITLE = 'An explanation of the lesson’s sources';
const PASSAGE_DETAIL = 'An explanation of a passage from the lesson’s sources, with errors in it, as in an exam.';
const NO_CODE = 'No lesson of this course can be drilled in code yet.';
const NO_SOURCE = 'The course has no source with text yet.';
const NO_PASSAGE = 'None of the lessons’ sources holds a passage to explain.';
const NOTHING_TO_DRILL = 'No lesson of this course can be drilled yet.';
const PICKED_LINE = 'If no drill of it is ready, one is written when you start, usually in a minute or two.';

const NEXT_LINE: Record<NextDrillDto['status'], string> = {
  ready: 'A drill of it is ready.',
  building: 'A drill of it is being written.',
  to_build: 'A drill of it is written when you start, usually in a minute or two.',
};

function standingText(lesson: DrillLessonDto): string {
  switch (lesson.standing) {
    case 'due':
      return 'Due now';
    case 'new':
      return 'New';
    case 'rested':
      return lesson.dueAt === null
        ? 'Due again'
        : `Due again ${new Date(lesson.dueAt).toLocaleDateString([], { day: 'numeric', month: 'short' })}`;
    case 'pending':
      return 'Not yet graded';
  }
}

function lessonText(lesson: DrillLessonDto, fits: boolean, misfit: string): string {
  return `${lesson.title} — ${standingText(lesson)}${lesson.current ? ', current lesson' : ''}${fits ? '' : ` (${misfit})`}`;
}

function outcomeText(attempt: CourseDrillAttemptDto): string {
  switch (attempt.status) {
    case 'open':
      return 'Continue';
    case 'submitted':
      return 'Waiting for its grade';
    case 'graded':
      return attempt.score
        ? `Found ${attempt.score.found} of ${attempt.score.found + attempt.score.halfFound + attempt.score.missed}`
        : 'Graded';
    case 'couldnt_grade':
      return 'Could not be graded';
  }
}

function recentLine(attempt: CourseDrillAttemptDto): string {
  return [
    attempt.lesson?.title,
    attempt.material.kind === 'code' ? languageLabel(attempt.material.language) : 'Explanation',
    MODE_LABEL[attempt.mode],
    momentOf(attempt.startedAt)?.text,
  ].filter(Boolean).join(' · ');
}

/** What the learner picks before a drill: what to review, the lesson, how they practise it and how long it is. */
export function DrillChoose({ programId, options, onChange, onStart, onOpenAttempt }: {
  programId: string;
  options: DrillOptions;
  onChange: (patch: Partial<DrillOptions>) => void;
  onStart: (patch: Partial<DrillOptions>) => void;
  onOpenAttempt: (attemptId: string) => void;
}) {
  const { view, error: viewError, reload: reloadView } = useCourseDrills(programId, options.language);
  const { languages, error, reload } = useDrillLanguages();

  // Computed effective material instead of mutating via useEffect
  const effectiveMaterial = view
    ? options.material === 'code' && !view.nextCode && view.nextPassage
      ? 'passage'
      : options.material === 'passage' && !view.nextPassage && view.nextCode
        ? 'code'
        : options.material
    : options.material;

  const passage = effectiveMaterial === 'passage';
  const language = options.language ?? view?.language ?? null;
  const next = view ? (passage ? view.nextPassage : view.nextCode) : null;

  // Computed effective lessonId instead of mutating via useEffect
  const effectiveLessonId = (() => {
    if (!view || !options.lessonId) return null;
    const picked = view.modules.flatMap((module) => module.lessons).find((lesson) => lesson.lessonId === options.lessonId);
    const fits = picked && (passage ? picked.fitsPassage : picked.fitsCode);
    return fits ? options.lessonId : null;
  })();

  const nextLesson = view && next ? view.modules.flatMap((module) => module.lessons).find((lesson) => lesson.lessonId === next.lessonId) : undefined;
  let nextOption = 'Reading the lessons…';
  if (view) nextOption = nextLesson ? `Next: ${nextLesson.title}` : 'No lesson can be drilled';
  let lessonLine: string | null = null;
  if (view) {
    if (effectiveLessonId) lessonLine = PICKED_LINE;
    else lessonLine = next ? NEXT_LINE[next.status] : NOTHING_TO_DRILL;
  }

  const codeOff = view !== null && !view.nextCode;
  const passageOff = view !== null && !view.nextPassage;
  const minutesOk = options.mode !== 'interview' || validMinutes(options.minutes);
  const canStart = view !== null && minutesOk && (next !== null || effectiveLessonId !== null) && (passage || Boolean(language));
  const sizes = sizesFor(effectiveMaterial);
  const unit = passage ? 'sentences' : 'lines';
  const things = passage ? 'errors' : 'issues';

  return (
    <>
      <h2 className="font-serif text-2xl text-text-primary">Review drills</h2>
      <p className="mt-2 max-w-2xl text-xs leading-6 text-text-secondary">
        Review something you did not write, built from a lesson of this course, and find what is wrong in it. Then see every planted error, and what the lesson says about it.
      </p>
      {!view && !viewError && <p role="status" className="mt-4 text-xs text-text-secondary">{READING}</p>}
      {viewError && (
        <p role="alert" className="mt-4 flex items-center gap-2 text-xs text-danger-fg">
          {viewError}
          <Button type="button" size="sm" variant="outline" onClick={reloadView}>Try again</Button>
        </p>
      )}
      <form
        className="mt-8 space-y-8"
        onSubmit={(event) => {
          event.preventDefault();
          if (canStart) onStart({ language, material: effectiveMaterial, lessonId: effectiveLessonId });
        }}
      >
        <fieldset>
          <legend className="text-xs font-semibold text-text-primary">What to review</legend>
          <div className="mt-2 grid gap-3 sm:grid-cols-2">
            <Choice
              name="drill-material"
              value="code"
              checked={!passage}
              disabled={codeOff}
              onSelect={() => onChange({ material: 'code' })}
              title="Code"
              detail={<>{CODE_DETAIL}{codeOff && <span className="mt-1 block text-text-muted">{NO_CODE}</span>}</>}
            />
            <Choice
              name="drill-material"
              value="passage"
              checked={passage}
              disabled={passageOff}
              onSelect={() => onChange({ material: 'passage' })}
              title={PASSAGE_TITLE}
              detail={<>{PASSAGE_DETAIL}{passageOff && <span className="mt-1 block text-text-muted">{view?.hasSources ? NO_PASSAGE : NO_SOURCE}</span>}</>}
            />
          </div>
        </fieldset>

        {!passage && (
          <div>
            <label htmlFor="drill-language" className="text-xs font-semibold text-text-primary">Language</label>
            <div className="mt-2 flex flex-wrap items-center gap-3">
              <select
                id="drill-language"
                value={language ?? ''}
                disabled={!languages}
                onChange={(event) => onChange({ language: event.target.value })}
                className={cn(INPUT, 'min-w-48')}
              >
                {(languages ?? (language ? [language] : [])).map((id) => (
                  <option key={id} value={id}>{languageLabel(id)}</option>
                ))}
              </select>
              {error && (
                <p role="alert" className="flex items-center gap-2 text-xs text-danger-fg">
                  {error}
                  <Button type="button" size="sm" variant="outline" onClick={reload}>Try again</Button>
                </p>
              )}
            </div>
          </div>
        )}

        <div>
          <label htmlFor="drill-lesson" className="text-xs font-semibold text-text-primary">Lesson</label>
          <div className="mt-2">
            <select
              id="drill-lesson"
              value={effectiveLessonId ?? ''}
              disabled={!view}
              onChange={(event) => onChange({ lessonId: event.target.value || null })}
              className={cn(INPUT, 'min-w-48 max-w-full')}
            >
              <option value="">{nextOption}</option>
              {(view?.modules ?? []).filter((module) => module.lessons.length > 0).map((module) => (
                <optgroup key={module.moduleId} label={module.title}>
                  {module.lessons.map((lesson) => {
                    const fits = passage ? lesson.fitsPassage : lesson.fitsCode;
                    return (
                      <option key={lesson.lessonId} value={lesson.lessonId} disabled={!fits}>
                        {lessonText(lesson, fits, passage ? 'no passage in the sources fits it' : 'no code drill fits it')}
                      </option>
                    );
                  })}
                </optgroup>
              ))}
            </select>
          </div>
          {lessonLine && <p className="mt-1.5 text-xs text-text-muted">{lessonLine}</p>}
        </div>

        <fieldset>
          <legend className="text-xs font-semibold text-text-primary">Mode</legend>
          <div className="mt-2 grid gap-3 sm:grid-cols-2">
            {MODES.map((mode) => (
              <Choice
                key={mode.id}
                name="drill-mode"
                value={mode.id}
                checked={options.mode === mode.id}
                onSelect={() => onChange({ mode: mode.id })}
                title={mode.title}
                detail={mode.detail}
              />
            ))}
          </div>
        </fieldset>

        {options.mode === 'interview' && (
          <div>
            <label htmlFor="drill-minutes" className="text-xs font-semibold text-text-primary">Time limit</label>
            <div className="mt-2 flex items-center gap-2">
              <input
                id="drill-minutes"
                type="number"
                min={1}
                step={1}
                inputMode="numeric"
                value={Number.isNaN(options.minutes) ? '' : options.minutes}
                onChange={(event) => onChange({ minutes: event.target.valueAsNumber })}
                className={cn(INPUT, 'w-24')}
              />
              <span className="text-xs text-text-secondary">minutes</span>
            </div>
            {minutesOk
              ? <p className="mt-1.5 text-xs text-text-muted">At zero the clock says so; it does not hand your review in for you.</p>
              : <p role="alert" className="mt-1.5 text-xs text-danger-fg">Enter a whole number of minutes, at least 1.</p>}
          </div>
        )}

        <fieldset>
          <legend className="text-xs font-semibold text-text-primary">Size</legend>
          <div className="mt-2 grid gap-3 sm:grid-cols-3">
            {sizes.map((size) => (
              <Choice
                key={size.id}
                name="drill-size"
                value={size.id}
                checked={options.size === size.id}
                onSelect={() => onChange({ size: size.id })}
                title={size.label}
                detail={`About ${size.lines} ${unit}, ${size.issues} ${things}`}
              />
            ))}
          </div>
        </fieldset>

        <Button type="submit" size="lg" disabled={!canStart}>Start drill</Button>
      </form>

      {view && view.recent.length > 0 && (
        <section className="mt-12">
          <h3 className="text-xs font-semibold text-text-primary">Recent drills</h3>
          <ul className="mt-2 space-y-2">
            {view.recent.map((attempt) => (
              <li key={attempt.attemptId}>
                <button
                  type="button"
                  onClick={() => onOpenAttempt(attempt.attemptId)}
                  className="block w-full rounded-xl border border-border-subtle p-4 text-left transition hover:border-border-default focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
                >
                  <span className="block text-xs font-semibold text-text-primary">{attempt.title}</span>
                  <span className="mt-0.5 block text-xs leading-5 text-text-secondary">{recentLine(attempt)}</span>
                  <span className="mt-0.5 block text-xs leading-5 text-text-muted">{outcomeText(attempt)}</span>
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
    </>
  );
}
