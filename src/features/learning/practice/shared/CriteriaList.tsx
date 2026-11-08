import type React from 'react';
import { AlertCircle, CheckCircle2, Circle, Clock, HelpCircle, MinusCircle } from 'lucide-react';

import { cn } from '@/lib/utils';

export type CriterionState = 'met' | 'partial' | 'missed' | 'insufficient_evidence' | 'pending';

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
  nextStep?: string | null;
}

export interface CriteriaListProps {
  title?: string;
  subtitle?: string;
  items: CriterionItem[];
  footer?: React.ReactNode;
  className?: string;
}

const STATE_ICONS: Record<CriterionState, React.ComponentType<{ className?: string }>> = {
  met: CheckCircle2,
  partial: MinusCircle,
  missed: AlertCircle,
  insufficient_evidence: HelpCircle,
  pending: Clock,
};

const STATE_LABELS: Record<CriterionState, string> = {
  met: 'Met',
  partial: 'Partially met',
  missed: 'Missed',
  insufficient_evidence: 'Not enough evidence',
  pending: 'Pending',
};

/**
 * Shared criteria list for practice debriefs and scorecards.
 * Owned by cs-results; provided here so cs-thread workbench panel can import it cleanly.
 */
export function CriteriaList({
  title,
  subtitle,
  items,
  footer,
  className,
}: CriteriaListProps): JSX.Element {
  return (
    <section className={cn('rounded-2xl border border-border-default bg-surface p-4 sm:p-5', className)}>
      {title && (
        <div className="mb-3">
          <h4 className="text-sm font-semibold text-text-primary">{title}</h4>
          {subtitle && <p className="mt-0.5 text-xs text-text-secondary">{subtitle}</p>}
        </div>
      )}

      <ul className="space-y-2.5">
        {items.map((item) => {
          const Icon = STATE_ICONS[item.state] ?? Circle;
          const stateLabel = STATE_LABELS[item.state];

          return (
            <li
              key={item.id}
              className="rounded-xl border border-border-subtle bg-surface-sunken p-3"
            >
              <div className="flex items-start justify-between gap-2">
                <div className="flex items-center gap-2">
                  <Icon className="h-4 w-4 shrink-0 text-text-secondary" aria-hidden="true" />
                  <span className="text-xs font-medium text-text-primary">{item.title}</span>
                </div>
                <span className="text-xs text-text-muted">{stateLabel}</span>
              </div>
              {item.description && (
                <p className="mt-1 pl-6 text-xs text-text-secondary">{item.description}</p>
              )}
              {item.observation && (
                <p className="mt-1 pl-6 text-xs text-text-muted">{item.observation}</p>
              )}
              {item.nextStep && (
                <p className="mt-1 pl-6 text-xs font-medium text-text-primary">Next step: {item.nextStep}</p>
              )}
            </li>
          );
        })}
      </ul>

      {footer && <div className="mt-4">{footer}</div>}
    </section>
  );
}
