import { ArrowDown, ArrowUp, Pencil, Trash2 } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

import { describeSpan, type DraftFinding } from './findings';
import { severityLabel, severityTone } from './severity';

/**
 * The learner's comments in the order they will be handed in. Leading with the most severe issue counts in the score,
 * so the order is the learner's to change.
 */
export function FindingsList({ findings, editingId, onShow, onEdit, onRemove, onMove }: {
  findings: readonly DraftFinding[];
  editingId: string | null;
  onShow: (finding: DraftFinding) => void;
  onEdit: (finding: DraftFinding) => void;
  onRemove: (finding: DraftFinding) => void;
  onMove: (finding: DraftFinding, delta: -1 | 1) => void;
}) {
  return (
    <section aria-labelledby="drill-findings-title">
      <h3 id="drill-findings-title" className="text-sm font-medium text-text-primary">
        Your comments <span className="font-normal text-text-muted">({findings.length})</span>
      </h3>
      <p className="mt-1 text-xs leading-5 text-text-secondary">
        Put what you would fix first at the top: leading with the most severe issue counts.
      </p>
      {findings.length === 0 ? (
        <p className="mt-3 text-sm text-text-muted">No comments yet. Select lines in the code, or write a general comment.</p>
      ) : (
        <ol className="mt-3 space-y-2">
          {findings.map((finding, index) => (
            <li
              key={finding.id}
              aria-label={`Comment ${index + 1}`}
              className={cn('rounded-xl border bg-surface p-3', finding.id === editingId ? 'border-accent' : 'border-border')}
            >
              <div className="flex items-start justify-between gap-2">
                {finding.span ? (
                  <button
                    type="button"
                    title="Show these lines in the code"
                    onClick={() => onShow(finding)}
                    className="min-w-0 text-left font-mono text-xs text-text-secondary hover:text-accent focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
                  >
                    <span className="font-sans font-semibold tabular-nums text-text-muted">{index + 1}</span>{' '}
                    {describeSpan(finding.span)}
                  </button>
                ) : (
                  <span className="min-w-0 text-xs text-text-secondary">
                    <span className="font-semibold tabular-nums text-text-muted">{index + 1}</span>{' '}
                    General comment
                  </span>
                )}
                {finding.severity && <span className={severityTone(finding.severity)}>{severityLabel(finding.severity)}</span>}
              </div>
              <p className="mt-2 whitespace-pre-wrap text-sm leading-6 text-text-primary">{finding.text}</p>
              <div className="mt-2 flex items-center gap-1">
                <Button type="button" size="icon" variant="ghost" aria-label={`Move comment ${index + 1} up`} disabled={index === 0} onClick={() => onMove(finding, -1)}>
                  <ArrowUp aria-hidden="true" />
                </Button>
                <Button type="button" size="icon" variant="ghost" aria-label={`Move comment ${index + 1} down`} disabled={index === findings.length - 1} onClick={() => onMove(finding, 1)}>
                  <ArrowDown aria-hidden="true" />
                </Button>
                <Button type="button" size="icon" variant="ghost" aria-label={`Edit comment ${index + 1}`} onClick={() => onEdit(finding)}>
                  <Pencil aria-hidden="true" />
                </Button>
                <Button type="button" size="icon" variant="ghost" aria-label={`Delete comment ${index + 1}`} onClick={() => onRemove(finding)}>
                  <Trash2 aria-hidden="true" />
                </Button>
              </div>
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}
