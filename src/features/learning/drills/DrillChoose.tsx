import { useEffect, type ReactNode } from 'react';

import { Button } from '@/components/ui/button';
import { PageHeader } from '@/components/ui/PageHeader';
import { cn } from '@/lib/utils';

import {
  DEFAULT_LANGUAGE,
  DRILL_SIZES,
  languageLabel,
  validMinutes,
  type DrillMode,
  type DrillOptions,
} from './drillOptions';
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

/** What the learner picks before a drill: its language, how they practise it and how long it is. */
export function DrillChoose({ options, onChange, onStart }: {
  options: DrillOptions;
  onChange: (patch: Partial<DrillOptions>) => void;
  onStart: () => void;
}) {
  const { languages, error, reload } = useDrillLanguages();

  useEffect(() => {
    if (!languages || languages.includes(options.language)) return;
    onChange({ language: languages.includes(DEFAULT_LANGUAGE) ? DEFAULT_LANGUAGE : (languages[0] ?? options.language) });
  }, [languages, options.language, onChange]);

  const minutesOk = options.mode !== 'interview' || validMinutes(options.minutes);

  return (
    <>
      <PageHeader
        title="Code review drills"
        description="Read code you did not write, the way you would review a pull request: select lines, say what is wrong, why it matters and what to do. Then see every issue that was planted, and how to say it in an interview."
      />
      <form
        className="space-y-8"
        onSubmit={(event) => {
          event.preventDefault();
          if (minutesOk) onStart();
        }}
      >
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
            {DRILL_SIZES.map((size) => (
              <Choice
                key={size.id}
                name="drill-size"
                value={size.id}
                checked={options.size === size.id}
                onSelect={() => onChange({ size: size.id })}
                title={size.label}
                detail={`About ${size.lines} lines, ${size.issues} issues`}
              />
            ))}
          </div>
        </fieldset>

        <Button type="submit" size="lg" disabled={!minutesOk}>Start drill</Button>
      </form>
    </>
  );
}
