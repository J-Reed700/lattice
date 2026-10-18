import { useId, useRef, useState, type FormEvent, type KeyboardEvent } from 'react';

import { ChevronDown, RotateCcw } from 'lucide-react';

import {
  impactLine,
  LEVEL_MEANING,
  LEVEL_WORD,
  overrideLines,
  SITE_STANDINGS,
  STANDING_MEANING,
  STANDING_WORD,
  TRUST_LEVELS,
} from '@/features/learning/sources/trust/trustPresentation';
import { useClearSourceTrustOverride, useSetSourceTrustOverride, useSourceTrust } from '@/features/learning/sources/trust/useSourceTrust';
import type {
  LearningSiteStanding,
  LearningSourceTrustEntryDto,
  LearningTrustChangeResultDto,
  LearningTrustImpactDto,
  LearningTrustLevel,
  LearningTrustOverrideDto,
  LearningTrustOverrideScopeDto,
} from '@/lib/bindings';

type Scope = 'source' | 'site' | 'site_subject';

const FIELD = 'mt-1 w-full rounded-lg border border-border bg-background px-2.5 py-2 text-xs text-text-primary';

export function scopeOf(entry: LearningTrustOverrideDto): LearningTrustOverrideScopeDto {
  if (entry.scope === 'source') return { scope: 'source', programId: entry.programId, sourceId: entry.sourceId };
  if (entry.scope === 'site') return { scope: 'site', registrableDomain: entry.registrableDomain };
  return { scope: 'site_subject', registrableDomain: entry.registrableDomain, class: entry.class };
}

function unique(values: string[]) {
  return [...new Set(values.map((value) => value.trim()).filter(Boolean))];
}

/**
 * Overrides for one source (§19, decision 4): this source in this course
 * (any level), its site everywhere (a standing only, never primary), or its
 * site for chosen subject classes (any level, primary included). Before
 * applying, it says how many waiting claims the change sends back to be
 * re-decided.
 */
export function TrustOverrideMenu({
  entry,
  programId,
  sourceTitle,
  subjectOptions = [],
}: {
  entry: LearningSourceTrustEntryDto;
  programId: string;
  sourceTitle: string;
  /** The claim's subject, its aliases and its broader classes, when known. */
  subjectOptions?: string[];
}) {
  const id = useId();
  const toggleRef = useRef<HTMLButtonElement>(null);
  const site = entry.provenance?.site ?? null;
  const [open, setOpen] = useState(false);
  const [scope, setScope] = useState<Scope>('source');
  const [level, setLevel] = useState<LearningTrustLevel>(entry.trust.level);
  const [standing, setStanding] = useState<LearningSiteStanding>('identified');
  const [classes, setClasses] = useState<string[]>([]);
  const [extraClass, setExtraClass] = useState('');
  const [note, setNote] = useState('');
  const [error, setError] = useState('');
  const [done, setDone] = useState<string | null>(null);
  const set = useSetSourceTrustOverride();
  const clear = useClearSourceTrustOverride();
  const fetched = useSourceTrust(open && !entry.impact ? { programId, sourceIds: [entry.sourceId], claim: null, includeImpact: true } : null);
  const impacts = entry.impact ?? fetched.data?.sources[0]?.impact ?? null;
  const impact: LearningTrustImpactDto | null = impacts ? (scope === 'source' ? impacts.source : impacts.site) : null;
  const options = unique([...subjectOptions, ...classes]);
  const chosen = unique([...classes, extraClass]);
  const busy = set.isPending || clear.isPending;

  const close = () => {
    setOpen(false);
    toggleRef.current?.focus();
  };
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === 'Escape') {
      event.stopPropagation();
      close();
    }
  };
  const reported = (result: LearningTrustChangeResultDto | null, target: 'source' | 'site') =>
    setDone(result ? `Saved. ${impactLine(result.impact, target)}` : 'There was no override to reset.');

  const apply = async (event: FormEvent) => {
    event.preventDefault();
    setError('');
    setDone(null);
    const text = note.trim() || null;
    try {
      if (scope === 'source') {
        reported(await set.mutateAsync({ scope: 'source', programId, sourceId: entry.sourceId, level, note: text }), 'source');
      } else if (!site) {
        setError('This source has no site to trust.');
      } else if (scope === 'site') {
        reported(await set.mutateAsync({ scope: 'site', registrableDomain: site, standing, note: text }), 'site');
      } else if (chosen.length === 0) {
        setError('Choose at least one subject this site is trusted for.');
      } else {
        let last: LearningTrustChangeResultDto | null = null;
        for (const classLabel of chosen) {
          last = await set.mutateAsync({ scope: 'site_subject', registrableDomain: site, classLabel, level, note: text });
        }
        reported(last, 'site');
        setExtraClass('');
      }
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  const reset = async (override: LearningTrustOverrideDto) => {
    setError('');
    setDone(null);
    try {
      reported(await clear.mutateAsync(scopeOf(override)), override.scope === 'source' ? 'source' : 'site');
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  const preview = scope === 'site_subject' && site && chosen.length > 0
    ? `Trust ${site} as ${LEVEL_WORD[level].toLowerCase()} for: ${chosen.join(', ')}`
    : scope === 'site' && site
      ? `Treat ${site} as ${STANDING_WORD[standing].toLowerCase()} everywhere. A site is never primary by itself; choose “for a subject” for that.`
      : scope === 'source'
        ? `Trust “${sourceTitle}” as ${LEVEL_WORD[level].toLowerCase()} in this course.`
        : null;
  const applyLabel = impact && impact.claims > 0
    ? `Apply and re-decide ${impact.claims} ${impact.claims === 1 ? 'claim' : 'claims'}`
    : 'Apply';

  return (
    <div className="text-xs" onKeyDown={onKeyDown}>
      <button
        ref={toggleRef}
        type="button"
        aria-expanded={open}
        aria-controls={`${id}-menu`}
        onClick={() => (open ? close() : setOpen(true))}
        className="inline-flex min-h-8 items-center gap-1 rounded-full border border-border px-3 font-medium text-text-primary hover:border-accent/50"
      >
        Change trust… <ChevronDown size={13} aria-hidden="true" className={open ? 'rotate-180' : ''} />
      </button>
      {open && (
        <form id={`${id}-menu`} aria-label={`Trust for ${sourceTitle}`} onSubmit={(event) => void apply(event)} className="mt-2 space-y-3 rounded-xl border border-border bg-background p-3">
          {entry.overrides.length > 0 && (
            <section aria-label="Overrides in force">
              <h4 className="text-[10px] font-semibold uppercase tracking-wider text-text-muted">In force</h4>
              <ul className="mt-1 space-y-1">
                {entry.overrides.map((override) => {
                  const line = overrideLines([override])[0];
                  return (
                    <li key={line} className="flex items-center justify-between gap-2">
                      <span className="text-text-primary">{line}</span>
                      <button type="button" disabled={busy} onClick={() => void reset(override)} aria-label={`Reset: ${line}`} className="inline-flex min-h-8 items-center gap-1 rounded-full px-2 text-accent hover:underline disabled:opacity-50">
                        <RotateCcw size={12} aria-hidden="true" /> Reset
                      </button>
                    </li>
                  );
                })}
              </ul>
            </section>
          )}
          <fieldset>
            <legend className="font-medium text-text-secondary">Apply to</legend>
            <label className="mt-1 flex items-start gap-2"><input type="radio" name={`${id}-scope`} checked={scope === 'source'} onChange={() => setScope('source')} className="mt-0.5" />
              <span>This source, in this course <span className="block text-text-muted">Any level, primary included.</span></span></label>
            <label className="mt-1 flex items-start gap-2"><input type="radio" name={`${id}-scope`} disabled={!site} checked={scope === 'site'} onChange={() => setScope('site')} className="mt-0.5" />
              <span>{site ?? 'Its site'}, everywhere <span className="block text-text-muted">Says who the publisher is; never makes it primary.</span></span></label>
            <label className="mt-1 flex items-start gap-2"><input type="radio" name={`${id}-scope`} disabled={!site} checked={scope === 'site_subject'} onChange={() => setScope('site_subject')} className="mt-0.5" />
              <span>{site ?? 'Its site'}, for a subject <span className="block text-text-muted">Any level for claims about that subject; several sites can be primary for one.</span></span></label>
          </fieldset>
          {scope === 'site' ? (
            <label className="block font-medium text-text-secondary">Standing
              <select value={standing} onChange={(event) => setStanding(event.target.value as LearningSiteStanding)} className={FIELD}>
                {SITE_STANDINGS.map((value) => <option key={value} value={value}>{STANDING_WORD[value]} — {STANDING_MEANING[value]}</option>)}
              </select>
            </label>
          ) : (
            <label className="block font-medium text-text-secondary">Level
              <select value={level} onChange={(event) => setLevel(event.target.value as LearningTrustLevel)} className={FIELD}>
                {TRUST_LEVELS.map((value) => <option key={value} value={value}>{LEVEL_WORD[value]} — {LEVEL_MEANING[value]}</option>)}
              </select>
            </label>
          )}
          {scope === 'site_subject' && (
            <fieldset>
              <legend className="font-medium text-text-secondary">Subjects</legend>
              {options.map((option) => (
                <label key={option} className="mt-1 flex items-center gap-2">
                  <input type="checkbox" checked={classes.includes(option)} onChange={(event) => setClasses((current) => event.target.checked ? [...current, option] : current.filter((value) => value !== option))} />
                  {option}
                </label>
              ))}
              <label className="mt-2 block text-text-muted">Another subject
                <input value={extraClass} onChange={(event) => setExtraClass(event.target.value)} placeholder="e.g. Web platform APIs" className={FIELD} />
              </label>
            </fieldset>
          )}
          <label className="block font-medium text-text-secondary">Note <span className="font-normal text-text-muted">(optional)</span>
            <input value={note} onChange={(event) => setNote(event.target.value)} maxLength={280} className={FIELD} />
          </label>
          {preview && <p className="font-medium text-text-primary">{preview}</p>}
          {impact
            ? <p role="status">{impactLine(impact, scope === 'source' ? 'source' : 'site')}</p>
            : fetched.isFetching && <p role="status" className="text-text-muted">Counting the claims this would re-decide…</p>}
          {error && <p role="alert" className="text-rose-700">{error}</p>}
          {done && <p role="status" className="text-text-primary">{done}</p>}
          <div className="flex flex-wrap gap-2">
            <button type="submit" disabled={busy} className="min-h-8 rounded-full bg-accent px-3 font-semibold text-accent-fg disabled:opacity-50">{busy ? 'Saving…' : applyLabel}</button>
            <button type="button" onClick={close} className="min-h-8 rounded-full border border-border px-3 text-text-primary">Close</button>
          </div>
        </form>
      )}
    </div>
  );
}
