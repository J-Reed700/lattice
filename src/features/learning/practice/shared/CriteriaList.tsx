import { AlertCircle, CheckCircle2, FileCode2, HelpCircle, Lightbulb, MessageSquareQuote, Play, XCircle } from 'lucide-react';
import type React from 'react';

export type EvidenceView =
  | { kind: 'quote'; text: string }
  | { kind: 'lines'; path: string; start: number; end: number }
  | { kind: 'run'; label: string }
  | { kind: 'example'; label: string }
  | { kind: 'hint'; label: string }
  | { kind: 'text'; text: string };

export interface CriterionView {
  id: string;
  title: string;
  points: number | null;
  maxPoints: number;
  evidence: EvidenceView[];
  feedback?: string;
  nextStep?: string;
}

function Score({ points, maxPoints }: { points: number | null; maxPoints: number }) {
  if (points === null) {
    return (
      <span className="inline-flex items-center gap-1.5 rounded-full bg-surface-raised px-2.5 py-0.5 text-xs font-medium text-text-secondary">
        <HelpCircle size={14} className="shrink-0" aria-hidden="true" />
        <span>Not enough evidence</span>
      </span>
    );
  }
  const full = points >= maxPoints;
  const none = points <= 0;
  const Icon = full ? CheckCircle2 : none ? XCircle : AlertCircle;
  const tone = full
    ? 'bg-success-muted text-success-fg'
    : none
      ? 'bg-danger-muted text-danger-fg'
      : 'bg-warning-muted text-warning-fg';
  return (
    <span className={`inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-xs font-medium ${tone}`}>
      <Icon size={14} className="shrink-0" aria-hidden="true" />
      <span>
        {points} of {maxPoints}
      </span>
    </span>
  );
}

function Evidence({ item }: { item: EvidenceView }) {
  const chip = 'inline-flex items-center gap-1.5 rounded-full bg-surface-raised px-2.5 py-1 text-xs text-text-secondary';
  const icon = (node: React.ReactNode) => <span aria-hidden="true">{node}</span>;
  switch (item.kind) {
    case 'quote':
      return (
        <blockquote className="flex gap-2 border-l-2 border-border-default pl-2 text-xs text-text-secondary">
          {icon(<MessageSquareQuote size={13} />)}
          <span>“{item.text}”</span>
        </blockquote>
      );
    case 'lines':
      return (
        <span className={chip}>
          {icon(<FileCode2 size={13} />)}
          <span>
            {item.path}, {item.start === item.end ? `line ${item.start}` : `lines ${item.start}–${item.end}`}
          </span>
        </span>
      );
    case 'run':
    case 'example':
      return (
        <span className={chip}>
          {icon(<Play size={13} />)}
          <span>{item.label}</span>
        </span>
      );
    case 'hint':
      return (
        <span className={chip}>
          {icon(<Lightbulb size={13} />)}
          <span>{item.label}</span>
        </span>
      );
    case 'text':
      return <span className={chip}>{item.text}</span>;
  }
}

/** Criteria with their points, evidence, feedback and next step, all open. */
export function CriteriaList({ criteria, heading }: { criteria: CriterionView[]; heading?: string }) {
  return (
    <section className="rounded-2xl border border-border-default bg-surface p-4 sm:p-5">
      {heading && <h3 className="mb-4 font-serif text-lg text-text-primary">{heading}</h3>}
      <ul className="space-y-4">
        {criteria.map((criterion) => (
          <li key={criterion.id} className="rounded-xl border border-border-subtle bg-surface-sunken p-3 sm:p-4">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <span className="text-sm font-semibold text-text-primary">{criterion.title}</span>
              <Score points={criterion.points} maxPoints={criterion.maxPoints} />
            </div>
            {(criterion.feedback || criterion.evidence.length > 0 || criterion.nextStep) && (
              <div className="mt-3 space-y-2 border-t border-border-subtle pt-2">
                {criterion.feedback && <p className="text-xs text-text-secondary">{criterion.feedback}</p>}
                {criterion.evidence.length > 0 && (
                  <ul className="flex flex-wrap gap-2">
                    {criterion.evidence.map((item, index) => (
                      <li key={index}>
                        <Evidence item={item} />
                      </li>
                    ))}
                  </ul>
                )}
                {criterion.nextStep && (
                  <p className="text-xs text-text-primary">
                    <span className="font-semibold text-text-secondary">Next step: </span>
                    {criterion.nextStep}
                  </p>
                )}
              </div>
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}
