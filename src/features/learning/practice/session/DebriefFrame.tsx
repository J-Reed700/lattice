import { ArrowRight, CalendarClock, Check, X } from 'lucide-react';

import type * as Wire from '@/lib/bindings';

import { clockText, SETTING_NAMES } from './practiceNames';
import { CriteriaList, type CriterionView, type EvidenceView } from '../shared/CriteriaList';

import type { KindView } from './kind';

export interface DebriefFrameProps {
  session: Wire.PracticeSessionDto;
  debrief: Wire.PracticeDebriefDto;
  kind: Pick<KindView, 'Debrief'>;
  onExit: () => void;
  onTryInterview: () => void;
}

const RATINGS: Record<Wire.PatternRating, string> = {
  again: 'Again',
  hard: 'Hard',
  good: 'Good',
  easy: 'Easy',
};

const RUNG_NAMES: Record<Wire.Rung, string> = {
  nudge: 'nudge',
  idea: 'idea',
  plan: 'plan',
  point: 'line hint',
};

function evidenceView(evidence: Wire.ScorecardEvidence): EvidenceView {
  switch (evidence.kind) {
    case 'quote':
      return { kind: 'quote', text: evidence.text };
    case 'lines':
      return { kind: 'lines', path: evidence.path, start: evidence.start, end: evidence.end };
    case 'run':
      return { kind: 'run', label: 'A run' };
    case 'example':
      return { kind: 'example', label: `Example ${evidence.ordinal + 1}` };
    case 'hint':
      return { kind: 'hint', label: `Hint ${evidence.ordinal + 1}` };
  }
}

export function criteriaFrom(scorecard: Wire.Scorecard): CriterionView[] {
  return scorecard.rows.map((row) => ({
    id: row.criterionId,
    title: row.title,
    points: row.status === 'observed' ? row.points : null,
    maxPoints: row.maxPoints,
    evidence: row.evidence.map(evidenceView),
    feedback: row.feedback,
    nextStep: row.nextStep,
  }));
}

function plural(count: number, one: string, many = `${one}s`): string {
  return `${count} ${count === 1 ? one : many}`;
}

function comesBack(dueAt: number): string {
  return `Comes back ${new Date(dueAt).toLocaleDateString([], { weekday: 'short', day: 'numeric', month: 'short' })}`;
}

const CARD = 'rounded-xl border border-border-default bg-surface p-4';
const HEADING = 'm-0 mb-3 font-serif text-lg font-semibold text-text-primary';

export function DebriefFrame({ session, debrief, kind, onExit, onTryInterview }: DebriefFrameProps) {
  const { facts } = debrief;
  const { Debrief: KindDebrief } = kind;
  const totalMs = facts.stageMs.reduce((sum, stage) => sum + stage.activeMs, 0);

  return (
    <div className="mx-auto flex w-full max-w-5xl flex-col gap-5 p-4 sm:p-6">
      <header className="flex flex-col gap-2">
        <h1 className="m-0 font-serif text-2xl font-bold text-text-primary">Debrief: {session.title}</h1>
        <p className="m-0 text-sm text-text-secondary">
          {clockText(totalMs)} of active time.
        </p>
        <div className="flex flex-wrap items-center gap-3 pt-1">
          <button
            type="button"
            onClick={onTryInterview}
            className="inline-flex min-h-[44px] cursor-pointer items-center gap-2 rounded-lg bg-accent px-4 text-sm font-semibold text-accent-fg hover:opacity-90"
          >
            <span>Try it under interview conditions</span>
            <ArrowRight className="h-4 w-4" aria-hidden="true" />
          </button>
          <button
            type="button"
            onClick={onExit}
            className="min-h-[44px] cursor-pointer rounded-lg border border-border-default bg-surface px-4 text-sm font-semibold text-text-primary hover:bg-surface-raised"
          >
            Back to Practice
          </button>
        </div>
      </header>

      <div className="grid gap-5 lg:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]">
        <CriteriaList criteria={criteriaFrom(debrief.scorecard)} heading="How it went" />

        <section aria-labelledby="practice-facts-heading" className={CARD}>
          <h2 id="practice-facts-heading" className={HEADING}>
            What happened
          </h2>
          <h3 className="m-0 mb-1 text-xs font-semibold uppercase tracking-wide text-text-secondary">Time per stage</h3>
          <ul className="m-0 mb-3 list-none space-y-1 p-0 text-sm text-text-primary">
            {facts.stageMs.map((stage) => (
              <li key={stage.stage} className="flex justify-between gap-3">
                <span>{session.stages.find((item) => item.stage === stage.stage)?.label ?? stage.stage}</span>
                <span className="font-mono">{clockText(stage.activeMs)}</span>
              </li>
            ))}
          </ul>
          <ul className="m-0 list-none space-y-1 p-0 text-sm text-text-primary">
            <li>{plural(facts.checks, 'run')}</li>
            <li>{plural(facts.submits, 'submit')}</li>
            <li>
              {facts.hints.length === 0
                ? 'No hints'
                : facts.hints.map((hint) => `${hint.count} ${RUNG_NAMES[hint.rung]}`).join(', ')}
            </li>
          </ul>
          <h3 className="m-0 mb-1 mt-3 text-xs font-semibold uppercase tracking-wide text-text-secondary">Settings</h3>
          <ul className="m-0 list-none space-y-1 p-0 text-sm text-text-primary">
            {facts.settings.map((setting) => (
              <li key={setting}>{SETTING_NAMES[setting]}</li>
            ))}
          </ul>
        </section>
      </div>

      <KindDebrief session={session} debrief={debrief} />

      {debrief.misreadings.length > 0 ? (
        <section aria-labelledby="practice-misreadings-heading" className={CARD}>
          <h2 id="practice-misreadings-heading" className={HEADING}>
            Common mistakes
          </h2>
          <ul className="m-0 list-none space-y-2 p-0">
            {debrief.misreadings.map((misreading) => (
              <li key={misreading.description} className="flex items-start gap-2 text-sm text-text-primary">
                {misreading.caught ? (
                  <Check className="mt-0.5 h-4 w-4 flex-none text-success-fg" aria-hidden="true" />
                ) : (
                  <X className="mt-0.5 h-4 w-4 flex-none text-danger-fg" aria-hidden="true" />
                )}
                <span>
                  <span className="block">{misreading.description}</span>
                  <span className="block text-text-secondary">
                    {misreading.caught ? 'Your examples caught this' : 'Your examples did not catch this'}
                  </span>
                </span>
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      {debrief.techniques.length > 0 ? (
        <section aria-labelledby="practice-techniques-heading" className={CARD}>
          <h2 id="practice-techniques-heading" className={HEADING}>
            Problem patterns
          </h2>
          <ul className="m-0 list-none space-y-3 p-0">
            {debrief.techniques.map((technique) => (
              <li key={technique.techniqueId} className="flex flex-col gap-1 text-sm">
                <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                  <span className="font-semibold text-text-primary">{technique.name}</span>
                  <span className="rounded-full bg-surface-raised px-2.5 py-0.5 text-xs font-medium text-text-secondary">
                    {RATINGS[technique.rating]}
                  </span>
                  <span className="inline-flex items-center gap-1.5 text-text-secondary">
                    <CalendarClock className="h-4 w-4" aria-hidden="true" />
                    <span>{comesBack(technique.dueAt)}</span>
                  </span>
                </div>
                <span className="text-text-secondary">{technique.cue}</span>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
    </div>
  );
}
