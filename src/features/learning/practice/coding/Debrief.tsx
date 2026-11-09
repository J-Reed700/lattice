import { useState } from 'react';
import {
  ArrowRight,
  Calendar,
  CheckCircle2,
  ChevronRight,
  HelpCircle,
  Play,
} from 'lucide-react';
import { CriteriaList, type CriterionRow } from '../shared/CriteriaList';
import { SaveAsReviewCard } from '../shared/SaveAsReviewCard';
import type { CodingDebriefDto } from './codingApi';

export interface DebriefProps {
  debrief: CodingDebriefDto;
  programId: string;
  lessonId?: string | null;
  onRetryInterview?: () => void;
  onExit?: () => void;
  className?: string;
}

function formatRating(rating: string): string {
  switch (rating.toLowerCase()) {
    case 'easy':
      return 'Easy';
    case 'good':
      return 'Good';
    case 'hard':
      return 'Hard';
    default:
      return 'Again';
  }
}

function formatDueDate(dueAt: number): string {
  try {
    const d = new Date(dueAt);
    const day = d.toLocaleDateString(undefined, { weekday: 'short' });
    const dateNum = d.getDate();
    const month = d.toLocaleDateString(undefined, { month: 'short' });
    return `Back ${day} ${dateNum} ${month}`;
  } catch {
    return 'Coming back';
  }
}

export function Debrief({
  debrief,
  programId,
  lessonId,
  onRetryInterview,
  onExit,
  className = '',
}: DebriefProps) {
  const [showReference, setShowReference] = useState(false);

  // Map scorecard rows to CriteriaList format
  const criteriaRows: CriterionRow[] = debrief.scorecard.rows.map((row) => ({
    id: row.criterionId,
    title: row.title,
    description: row.description,
    outcome:
      row.status === 'insufficient_evidence' || row.points === null
        ? { kind: 'insufficient_evidence' }
        : { kind: 'points', points: row.points, maxPoints: row.maxPoints },
    feedback: row.feedback,
    nextStep: row.nextStep,
    evidence: row.evidence.map((e) => ({
      kind: e.kind,
      text: e.text,
    })),
  }));

  const settingLabel =
    debrief.setting === 'coached'
      ? 'COACHED'
      : debrief.setting === 'mock'
      ? 'MOCK'
      : 'WATCHED';

  // Total active time across stages
  const totalMs = debrief.stageTimings.reduce((sum, s) => sum + s.elapsedMs, 0);
  const totalMin = Math.max(1, Math.round(totalMs / 60000));

  return (
    <div className={`mx-auto max-w-4xl space-y-8 p-4 sm:p-6 ${className}`}>
      {/* Top Banner / Headline */}
      <section className="space-y-3">
        <div className="text-xxs font-semibold uppercase tracking-wider text-text-tertiary">
          DEBRIEF · CODE · {settingLabel}
        </div>
        <h1 className="font-serif text-2xl font-bold text-text-primary sm:text-3xl m-0">
          Coding session debrief
        </h1>
        <div className="text-sm text-text-secondary">
          {totalMin} min · {debrief.setting === 'coached' ? 'Full help' : 'Hints only'}
        </div>
        <div className="flex flex-wrap items-center gap-3 pt-2">
          {onRetryInterview && (
            <button
              type="button"
              onClick={onRetryInterview}
              className="inline-flex min-h-[44px] cursor-pointer items-center justify-center gap-2 rounded-lg bg-accent px-4 text-sm font-semibold text-accent-fg hover:opacity-90"
            >
              <span>Try it under interview conditions</span>
              <ArrowRight className="h-4 w-4" aria-hidden="true" />
            </button>
          )}
          {onExit && (
            <button
              type="button"
              onClick={onExit}
              className="inline-flex min-h-[44px] cursor-pointer items-center justify-center gap-2 rounded-lg border border-border-default bg-surface px-4 text-sm font-semibold text-text-primary hover:bg-surface-raised"
            >
              Back to Practice
            </button>
          )}
        </div>
      </section>

      {/* 1. What happened (Objective session facts) */}
      <section aria-labelledby="heading-happened" className="space-y-4">
        <div>
          <h2 id="heading-happened" className="font-serif text-xl font-semibold text-text-primary m-0">
            What happened
          </h2>
          <p className="mt-0.5 text-xs text-text-tertiary">
            Counted by the app, not scored.
          </p>
        </div>

        <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
          <div className="rounded-xl border border-border-default bg-surface p-4 shadow-xs space-y-1">
            <CheckCircle2 className="h-4 w-4 text-success-fg" aria-hidden="true" />
            <div className="font-serif text-xl font-semibold text-text-primary">All pass</div>
            <div className="text-xs text-text-secondary">cases pass</div>
          </div>

          <div className="rounded-xl border border-border-default bg-surface p-4 shadow-xs space-y-1">
            <CheckCircle2 className="h-4 w-4 text-text-secondary" aria-hidden="true" />
            <div className="font-serif text-xl font-semibold text-text-primary">
              {debrief.uncaughtMisreadings.length === 0 ? 'All' : 'Checked'}
            </div>
            <div className="text-xs text-text-secondary">common mistakes caught</div>
          </div>

          <div className="rounded-xl border border-border-default bg-surface p-4 shadow-xs space-y-1">
            <HelpCircle className="h-4 w-4 text-text-secondary" aria-hidden="true" />
            <div className="font-serif text-xl font-semibold text-text-primary">
              {debrief.hintsUsed.length === 1 ? '1 hint' : `${debrief.hintsUsed.length} hints`}
            </div>
            <div className="text-xs text-text-secondary">
              {debrief.hintsUsed[0] ? `a ${debrief.hintsUsed[0]}` : 'none used'}
            </div>
          </div>

          <div className="rounded-xl border border-border-default bg-surface p-4 shadow-xs space-y-1">
            <Play className="h-4 w-4 text-text-secondary" aria-hidden="true" />
            <div className="font-serif text-xl font-semibold text-text-primary">
              {debrief.runCount === 1 ? '1 run' : `${debrief.runCount} runs`}
            </div>
            <div className="text-xs text-text-secondary">the last one passed everything</div>
          </div>
        </div>

        {/* Time per stage */}
        <div className="rounded-xl border border-border-default bg-surface p-4 shadow-xs space-y-3">
          <h3 className="font-medium text-sm text-text-primary m-0">Time per stage</h3>
          <ul className="space-y-2 text-xs list-none m-0 p-0">
            {debrief.stageTimings.map((st) => {
              const minutes = Math.max(1, Math.round(st.elapsedMs / 60000));
              const pct = totalMs > 0 ? Math.min(100, Math.round((st.elapsedMs / totalMs) * 100)) : 25;
              const stageCapitalized = st.stage.charAt(0).toUpperCase() + st.stage.slice(1);

              return (
                <li key={st.stage} className="flex items-center gap-3">
                  <span className="w-24 text-text-secondary">
                    {stageCapitalized}
                    {st.stage === 'code' && <span className="sr-only"> stage</span>}
                  </span>
                  <div
                    aria-hidden="true"
                    className="h-2 flex-1 overflow-hidden rounded-full bg-surface-raised"
                  >
                    <div
                      className="h-full rounded-full bg-accent"
                      style={{ width: `${pct}%` }}
                    />
                  </div>
                  <span className="w-14 text-right font-mono text-text-secondary">
                    {minutes} min
                  </span>
                </li>
              );
            })}
          </ul>
        </div>
      </section>

      {/* 2. How it went (CriteriaList with 4 Scorecard criteria) */}
      <section aria-labelledby="heading-criteria" className="space-y-4">
        <CriteriaList
          heading="How it went"
          headingLevel={2}
          subtitle="Four areas, each scored 0 to 4, with the evidence it's based on."
          rows={criteriaRows}
        />
      </section>

      {/* 3. The strong answer */}
      <section aria-labelledby="heading-strong-answer" className="rounded-xl border border-border-default bg-surface p-5 shadow-xs space-y-4">
        <h2 id="heading-strong-answer" className="font-serif text-xl font-semibold text-text-primary m-0">
          The strong answer
        </h2>
        <div className="text-sm leading-relaxed text-text-primary whitespace-pre-wrap">
          {debrief.approachSummary}
        </div>
        <div className="font-mono text-xs text-text-secondary">
          Target complexity: {debrief.targetComplexity}
        </div>

        <div className="pt-2">
          <button
            type="button"
            onClick={() => setShowReference((v) => !v)}
            className="inline-flex min-h-[40px] cursor-pointer items-center gap-2 rounded-lg border border-border-default bg-surface px-4 text-xs font-semibold text-text-primary hover:bg-surface-raised"
          >
            <ChevronRight
              className={`h-4 w-4 transition-transform ${showReference ? 'rotate-90' : ''}`}
              aria-hidden="true"
            />
            <span>Show the reference code</span>
          </button>

          {showReference && (
            <div className="mt-3 overflow-x-auto rounded-lg border border-border-default bg-surface-sunken p-4 font-mono text-xs text-text-primary whitespace-pre">
              {debrief.referenceSolution}
            </div>
          )}
        </div>
      </section>

      {/* 4. Coming back (Pattern cards) */}
      {debrief.patterns && debrief.patterns.length > 0 && (
        <section aria-labelledby="heading-coming-back" className="rounded-xl border border-border-default bg-surface p-5 shadow-xs space-y-4">
          <div>
            <h2 id="heading-coming-back" className="font-serif text-xl font-semibold text-text-primary m-0">
              Coming back
            </h2>
            <p className="mt-0.5 text-xs text-text-tertiary">
              Each pattern in this problem, rated by what happened and spaced for review.
            </p>
          </div>

          <div className="divide-y divide-border-subtle">
            {debrief.patterns.map((p) => (
              <div key={p.patternId} className="flex flex-wrap items-center gap-3 py-3">
                <span className="font-semibold text-sm text-text-primary min-w-[140px]">
                  {p.name}
                </span>
                <span className="inline-flex min-h-[28px] items-center gap-1.5 rounded-full bg-success-muted px-2.5 text-xs font-medium text-success-fg">
                  <CheckCircle2 className="h-3.5 w-3.5" aria-hidden="true" />
                  <span>{formatRating(p.rating)}</span>
                </span>
                <span className="text-xs text-text-secondary flex-1 min-w-[200px]">
                  {p.recognitionCue}
                </span>
                <span className="ml-auto inline-flex items-center gap-1.5 font-mono text-xs text-text-tertiary">
                  <Calendar className="h-3.5 w-3.5" aria-hidden="true" />
                  <span>{formatDueDate(p.dueAt)}</span>
                </span>
              </div>
            ))}
          </div>
        </section>
      )}

      {/* 5. Save a mistake */}
      {debrief.uncaughtMisreadings && debrief.uncaughtMisreadings.length > 0 && (
        <section
          aria-labelledby="heading-save-mistake"
          ref={(el) => {
            const btn = el?.querySelector('button');
            if (btn && !btn.getAttribute('aria-label')) {
              btn.setAttribute('aria-label', 'Save as a review card');
            }
          }}
          className="rounded-xl border border-border-default bg-surface p-5 shadow-xs space-y-4"
        >
          <h2 id="heading-save-mistake" className="font-serif text-xl font-semibold text-text-primary m-0">
            Save a mistake
          </h2>
          <SaveAsReviewCard
            programId={programId}
            lessonId={lessonId ?? 'lesson-1'}
            question={debrief.uncaughtMisreadings[0]}
            promptText={debrief.uncaughtMisreadings[0]}
            promptLabel="In your own words, what's the difference?"
            answerLabel="In your own words, what's the difference?"
          />
        </section>
      )}
    </div>
  );
}
