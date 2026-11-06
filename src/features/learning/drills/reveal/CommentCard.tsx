import { useId } from 'react';

import { Badge } from '@/components/ui/badge';
import { lineLabel } from '@/features/learning/drills/reveal/revealCopy';
import { eyebrow, panel } from '@/features/learning/drills/reveal/revealStyle';
import { severityLabel, severityTone } from '@/features/learning/drills/severity';
import type { CodeReviewSeverity, CodeReviewSpanDto } from '@/lib/bindings';

export interface CommentVerdict {
  label: string;
  tone: string;
  reason: string;
}

/** One of the learner's comments: where it is, what it says, the severity they gave it, and what came of it. */
export function CommentCard({ number, span, text, severity, verdict }: {
  number: number;
  span: CodeReviewSpanDto | null;
  text: string;
  severity: CodeReviewSeverity | null;
  verdict?: CommentVerdict;
}) {
  const titleId = useId();
  return (
    <article aria-labelledby={titleId} className={`${panel} p-4 sm:p-5`}>
      <header className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <h3 id={titleId} className={eyebrow}>Comment {number}</h3>
        <span className="font-mono text-xs text-text-muted">{span ? lineLabel(span) : 'No lines selected'}</span>
        {severity && (
          <div className="inline-flex items-center gap-1.5 text-xs text-text-muted">
            Your severity
            <Badge variant="outline" className={severityTone(severity)}>{severityLabel(severity)}</Badge>
          </div>
        )}
        {verdict && <Badge variant="outline" className={verdict.tone}>{verdict.label}</Badge>}
      </header>
      <p className="mt-2 whitespace-pre-wrap text-sm leading-6 text-text-primary">{text}</p>
      {verdict?.reason && <p className="mt-2 text-sm leading-6 text-text-secondary">{verdict.reason}</p>}
    </article>
  );
}
