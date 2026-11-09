import type React from 'react';
import { AlertCircle, CheckCircle2, Clock, HelpCircle, XCircle } from 'lucide-react';

export type CriterionState = 'met' | 'partial' | 'missed' | 'insufficient_evidence' | 'pending';

export type Outcome =
  | { kind: 'points'; points: number; maxPoints: number }
  | { kind: 'insufficient_evidence' }
  | { kind: 'pending' }
  | { kind: 'found' }
  | { kind: 'missed' }
  | { kind: 'not_an_issue' }
  | { kind: 'also_valid' };

export interface CriterionEvidence {
  kind: 'quote' | 'lines' | 'run' | 'example' | 'hint' | 'note';
  text: string;
}

export interface CriterionRow {
  id: string;
  title: string;
  description?: string;
  tag?: string;
  outcome: Outcome;
  feedback?: string;
  evidence?: CriterionEvidence[];
  nextStep?: string;
}

export interface CriterionItem {
  id: string;
  title: string;
  description?: string;
  tag?: string;
  state: CriterionState;
  points?: number | null;
  maxPoints?: number | null;
  observation?: string | null;
  quote?: string | null;
  evidence?: CriterionEvidence[];
  nextStep?: string | null;
}

export interface CriteriaListProps {
  title?: string;
  subtitle?: string;
  items?: CriterionItem[];
  heading?: string;
  headingLevel?: 2 | 3 | 4;
  rows?: CriterionRow[];
  note?: React.ReactNode;
  footer?: React.ReactNode;
  className?: string;
}

export function OutcomeBadge({ outcome }: { outcome: Outcome }) {
  switch (outcome.kind) {
    case 'points': {
      const full = outcome.points >= outcome.maxPoints;
      const zero = outcome.points <= 0;
      const icon = full ? (
        <CheckCircle2 size={14} className="shrink-0 text-success-fg" aria-hidden="true" />
      ) : zero ? (
        <XCircle size={14} className="shrink-0 text-danger-fg" aria-hidden="true" />
      ) : (
        <AlertCircle size={14} className="shrink-0 text-warning-fg" aria-hidden="true" />
      );
      const tone = full
        ? 'bg-success-muted text-success-fg'
        : zero
          ? 'bg-danger-muted text-danger-fg'
          : 'bg-warning-muted text-warning-fg';
      return (
        <span className={`inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-xs font-medium ${tone}`}>
          {icon}
          <span>{outcome.points} of {outcome.maxPoints}</span>
        </span>
      );
    }
    case 'found':
      return (
        <span className="inline-flex items-center gap-1.5 rounded-full bg-success-muted px-2.5 py-0.5 text-xs font-medium text-success-fg">
          <CheckCircle2 size={14} className="shrink-0 text-success-fg" aria-hidden="true" />
          <span>Found</span>
        </span>
      );
    case 'missed':
      return (
        <span className="inline-flex items-center gap-1.5 rounded-full bg-danger-muted px-2.5 py-0.5 text-xs font-medium text-danger-fg">
          <XCircle size={14} className="shrink-0 text-danger-fg" aria-hidden="true" />
          <span>Missed</span>
        </span>
      );
    case 'not_an_issue':
      return (
        <span className="inline-flex items-center gap-1.5 rounded-full bg-warning-muted px-2.5 py-0.5 text-xs font-medium text-warning-fg">
          <AlertCircle size={14} className="shrink-0 text-warning-fg" aria-hidden="true" />
          <span>Not an issue</span>
        </span>
      );
    case 'also_valid':
      return (
        <span className="inline-flex items-center gap-1.5 rounded-full bg-surface-raised px-2.5 py-0.5 text-xs font-medium text-text-secondary">
          <CheckCircle2 size={14} className="shrink-0 text-text-secondary" aria-hidden="true" />
          <span>Also valid</span>
        </span>
      );
    case 'insufficient_evidence':
      return (
        <span className="inline-flex items-center gap-1.5 rounded-full bg-surface-raised px-2.5 py-0.5 text-xs font-medium text-text-tertiary">
          <HelpCircle size={14} className="shrink-0 text-text-tertiary" aria-hidden="true" />
          <span>Not enough evidence</span>
        </span>
      );
    case 'pending':
      return (
        <span className="inline-flex items-center gap-1.5 rounded-full bg-surface-raised px-2.5 py-0.5 text-xs font-medium text-text-muted">
          <Clock size={14} className="shrink-0 text-text-muted" aria-hidden="true" />
          <span>Pending</span>
        </span>
      );
  }
}

function stateBadge(state: CriterionState) {
  switch (state) {
    case 'met':
      return {
        label: 'Met',
        icon: <CheckCircle2 size={14} className="shrink-0 text-success-fg" aria-hidden="true" />,
        className: 'bg-success-muted text-success-fg',
      };
    case 'partial':
      return {
        label: 'Partial',
        icon: <AlertCircle size={14} className="shrink-0 text-warning-fg" aria-hidden="true" />,
        className: 'bg-warning-muted text-warning-fg',
      };
    case 'missed':
      return {
        label: 'Missed',
        icon: <XCircle size={14} className="shrink-0 text-danger-fg" aria-hidden="true" />,
        className: 'bg-danger-muted text-danger-fg',
      };
    case 'insufficient_evidence':
      return {
        label: 'Not enough evidence',
        icon: <HelpCircle size={14} className="shrink-0 text-text-tertiary" aria-hidden="true" />,
        className: 'bg-surface-raised text-text-tertiary',
      };
    case 'pending':
      return {
        label: 'Pending',
        icon: <Clock size={14} className="shrink-0 text-text-muted" aria-hidden="true" />,
        className: 'bg-surface-raised text-text-muted',
      };
  }
}

export function CriteriaList({
  title,
  subtitle,
  items,
  heading,
  headingLevel = 3,
  rows,
  note,
  footer,
  className = '',
}: CriteriaListProps) {
  const effectiveTitle = title ?? heading;
  const effectiveFooter = footer ?? note;
  const HeadingTag = `h${headingLevel}` as const;

  const normalizedItems: CriterionItem[] =
    items ??
    (rows ?? []).map((row) => {
      const outcome = row.outcome;
      const state: CriterionState =
        outcome.kind === 'points'
          ? outcome.points >= outcome.maxPoints
            ? 'met'
            : outcome.points > 0
              ? 'partial'
              : 'missed'
          : outcome.kind === 'found' || outcome.kind === 'also_valid'
            ? 'met'
            : outcome.kind === 'missed'
              ? 'missed'
              : outcome.kind === 'not_an_issue'
                ? 'partial'
                : outcome.kind === 'insufficient_evidence'
                  ? 'insufficient_evidence'
                  : 'pending';
      return {
        id: row.id,
        title: row.title,
        description: row.description,
        tag: row.tag,
        state,
        points: outcome.kind === 'points' ? outcome.points : null,
        maxPoints: outcome.kind === 'points' ? outcome.maxPoints : null,
        observation: row.feedback ?? null,
        evidence: row.evidence,
        nextStep: row.nextStep ?? null,
      };
    });

  return (
    <section className={`rounded-2xl border border-border-default bg-surface p-4 sm:p-5 ${className}`}>
      {(effectiveTitle || subtitle) && (
        <header className="mb-4">
          {effectiveTitle && <HeadingTag className="font-serif text-lg text-text-primary">{effectiveTitle}</HeadingTag>}
          {subtitle && <p className="mt-1 text-xs text-text-secondary">{subtitle}</p>}
        </header>
      )}

      <ul className="space-y-4">
        {normalizedItems.map((item, idx) => {
          const rawRow = rows?.[idx];
          const badge = stateBadge(item.state);
          const hasScore = item.points !== null && item.points !== undefined;
          const scoreText = hasScore
            ? `${item.points} / ${item.maxPoints ?? 4}`
            : item.state === 'insufficient_evidence'
              ? 'Not enough evidence'
              : null;

          return (
            <li
              key={item.id}
              className="rounded-xl border border-border-subtle bg-surface-sunken p-3 sm:p-4"
            >
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex items-center gap-2">
                  <span className="text-xs font-semibold text-text-primary">{item.title}</span>
                  {item.tag && (
                    <span className="rounded-full bg-surface-raised px-2 py-0.5 text-xs text-text-muted">
                      {item.tag}
                    </span>
                  )}
                </div>

                <div className="flex items-center gap-2">
                  {rawRow ? (
                    <OutcomeBadge outcome={rawRow.outcome} />
                  ) : (
                    <>
                      {scoreText && (
                        <span className="text-xs font-medium tabular-nums text-text-secondary">
                          {scoreText}
                        </span>
                      )}
                      <span
                        className={`inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-xs font-medium ${badge.className}`}
                      >
                        {badge.icon}
                        <span>{badge.label}</span>
                      </span>
                    </>
                  )}
                </div>
              </div>

              {item.description && (
                <p className="mt-1 text-xs text-text-secondary">{item.description}</p>
              )}

              {(item.observation || item.quote || item.evidence?.length || item.nextStep) && (
                <div className="mt-3 space-y-2 border-t border-border-subtle pt-2">
                  {item.observation && (
                    <p className="text-xs text-text-secondary">{item.observation}</p>
                  )}
                  {item.quote && (
                    <blockquote className="border-l-2 border-border-default pl-2 text-xs italic text-text-muted">
                      “{item.quote}”
                    </blockquote>
                  )}
                  {item.evidence && item.evidence.length > 0 && (
                    <ul className="flex flex-wrap gap-2">
                      {item.evidence.map((ev, evIdx) => (
                        <li
                          key={`${ev.kind}-${evIdx}`}
                          className="inline-flex items-center gap-1.5 rounded-full bg-surface-raised px-2.5 py-1 text-xs text-text-secondary"
                        >
                          <span>{ev.text}</span>
                        </li>
                      ))}
                    </ul>
                  )}
                  {item.nextStep && (
                    <p className="text-xs text-text-primary">
                      <span className="font-semibold text-text-secondary">Next step: </span>
                      {item.nextStep}
                    </p>
                  )}
                </div>
              )}
            </li>
          );
        })}
      </ul>

      {effectiveFooter && <div className="mt-4 border-t border-border-subtle pt-3">{effectiveFooter}</div>}
    </section>
  );
}
