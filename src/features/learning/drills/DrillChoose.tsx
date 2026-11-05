import { useEffect, type ReactNode } from 'react';

import { Button } from '@/components/ui/button';
import { PageHeader } from '@/components/ui/PageHeader';
import { cn } from '@/lib/utils';

import {
  DEFAULT_LANGUAGE,
  languageLabel,
  sizesFor,
  validMinutes,
  type DrillMode,
  type DrillOptions,
  type ReviewKind,
} from './drillOptions';
import { useDrillCourses } from './useDrillCourses';
import { useDrillLanguages } from './useDrillLanguages';

const INPUT = 'rounded-lg border border-border bg-background px-3 py-2 text-sm text-text-primary outline-hidden focus:border-accent focus:ring-2 focus:ring-accent/15';

function Choice({ name, value, checked, onSelect, title, detail }: {
  name: string;
  value: string;
  checked: boolean;
  onSelect: () => void;
  title: string;
  detail: ReactNode;
}) {
  return (
    <label className={cn(
      'flex cursor-pointer items-start gap-3 rounded-xl border p-4 transition focus-within:ring-2 focus-within:ring-ring',
      checked ? 'border-accent bg-accent-muted' : 'border-border-subtle hover:border-border-strong',
    )}>
      <input type="radio" name={name} value={value} checked={checked} onChange={onSelect} className="mt-1 accent-[hsl(var(--accent))]" />
      <span className="min-w-0">
        <span className="block text-sm font-medium text-text-primary">{title}</span>
        <span className="mt-0.5 block text-xs leading-5 text-text-secondary">{detail}</span>
      </span>
    </label>
  );
}

const MODES: readonly { id: DrillMode; title: string; detail: string }[] = [
  { id: 'practice', title: 'Practice', detail: 'Untimed. Take the time to find each issue and say why it matters.' },
  { id: 'interview', title: 'Interview', detail: 'Timed, as in an interview: find the issue that matters most first, and say it clearly.' },
];

const MATERIALS: readonly { id: ReviewKind; title: string; detail: string }[] = [
  { id: 'code', title: 'Code', detail: 'Code you did not write, as you would review a pull request.' },
  { id: 'passage', title: 'A passage from a course', detail: 'A study note on something in one of your courses, with errors in it, as in an exam.' },
];

/** What the learner picks before a drill: what to review, how they practise it and how long it is. */
export function DrillChoose({ options, onChange, onStart }: {
  options: DrillOptions;
  onChange: (patch: Partial<DrillOptions>) => void;
  onStart: () => void;
}) {
  const { languages, error, reload } = useDrillLanguages();
  const passage = options.material === 'passage';
  const { courses, error: coursesError, reload: reloadCourses } = useDrillCourses(passage);

  useEffect(() => {
    if (!languages || languages.includes(options.language)) return;
    onChange({ language: languages.includes(DEFAULT_LANGUAGE) ? DEFAULT_LANGUAGE : (languages[0] ?? options.language) });
  }, [languages, options.language, onChange]);

  useEffect(() => {
    if (!courses || courses.some((course) => course.programId === options.programId)) return;
    onChange({ programId: courses[0]?.programId ?? null });
  }, [courses, options.programId, onChange]);

  const minutesOk = options.mode !== 'interview' || validMinutes(options.minutes);
  const noCourse = passage && options.programId === null;
  const sizes = sizesFor(options.material);
  const unit = passage ? 'sentences' : 'lines';
  const things = passage ? 'errors' : 'issues';

  return (
    <>
      <PageHeader
        title="Review drills"
        description="Read something you did not write and find what is wrong: code, the way you would review a pull request, or an explanation of something in your courses. Select what is wrong, say why it matters and what to do. Then see every error that was planted, and how to say it."
      />
      <form
        className="space-y-8"
        onSubmit={(event) => {
          event.preventDefault();
          if (minutesOk && !noCourse) onStart();
        }}
      >
        <fieldset>
          <legend className="text-sm font-medium text-text-primary">What to review</legend>
          <div className="mt-2 grid gap-3 sm:grid-cols-2">
            {MATERIALS.map((material) => (
              <Choice
                key={material.id}
                name="drill-material"
                value={material.id}
                checked={options.material === material.id}
                onSelect={() => onChange({ material: material.id })}
                title={material.title}
                detail={material.detail}
              />
            ))}
          </div>
        </fieldset>

        {passage ? (
          <div className="space-y-6">
            <div>
              <label htmlFor="drill-course" className="text-sm font-medium text-text-primary">Course</label>
              <div className="mt-2 flex flex-wrap items-center gap-3">
                <select
                  id="drill-course"
                  value={options.programId ?? ''}
                  disabled={!courses || courses.length === 0}
                  onChange={(event) => onChange({ programId: event.target.value })}
                  className={cn(INPUT, 'min-w-48 max-w-full')}
                >
                  {(courses ?? []).map((course) => (
                    <option key={course.programId} value={course.programId}>{course.title}</option>
                  ))}
                </select>
                {coursesError && (
                  <p role="alert" className="flex items-center gap-2 text-xs text-danger-fg">
                    {coursesError}
                    <Button type="button" size="sm" variant="outline" onClick={reloadCourses}>Try again</Button>
                  </p>
                )}
              </div>
              {courses?.length === 0 && (
                <p role="status" className="mt-1.5 text-xs text-text-muted">None of your courses has a source to take a passage from yet.</p>
              )}
            </div>
            <div>
              <label htmlFor="drill-topic" className="text-sm font-medium text-text-primary">Topic <span className="font-normal text-text-muted">(optional)</span></label>
              <input
                id="drill-topic"
                type="text"
                value={options.topic}
                placeholder="Anything in the course’s sources"
                onChange={(event) => onChange({ topic: event.target.value })}
                className={cn(INPUT, 'mt-2 block w-full max-w-md')}
              />
            </div>
          </div>
        ) : (
          <div>
            <label htmlFor="drill-language" className="text-sm font-medium text-text-primary">Language</label>
            <div className="mt-2 flex flex-wrap items-center gap-3">
              <select
                id="drill-language"
                value={options.language}
                disabled={!languages}
                onChange={(event) => onChange({ language: event.target.value })}
                className={cn(INPUT, 'min-w-48')}
              >
                {(languages ?? [options.language]).map((id) => (
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

        <fieldset>
          <legend className="text-sm font-medium text-text-primary">Mode</legend>
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
            <label htmlFor="drill-minutes" className="text-sm font-medium text-text-primary">Time limit</label>
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
              <span className="text-sm text-text-secondary">minutes</span>
            </div>
            {minutesOk
              ? <p className="mt-1.5 text-xs text-text-muted">At zero the clock says so; it does not hand your review in for you.</p>
              : <p role="alert" className="mt-1.5 text-xs text-danger-fg">Enter a whole number of minutes, at least 1.</p>}
          </div>
        )}

        <fieldset>
          <legend className="text-sm font-medium text-text-primary">Size</legend>
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

        <Button type="submit" size="lg" disabled={!minutesOk || noCourse}>Start drill</Button>
      </form>
    </>
  );
}
