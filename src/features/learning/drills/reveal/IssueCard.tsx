import { useId } from 'react';

import { Check, CircleDot, FileCode, Minus, X, type LucideIcon } from 'lucide-react';

import { Badge } from '@/components/ui/badge';
import type { ReviewKind } from '@/features/learning/drills/drillOptions';
import { FixDiff } from '@/features/learning/drills/reveal/FixDiff';
import { fencedCode } from '@/features/learning/drills/reveal/lessonText';
import { plainText } from '@/features/learning/drills/reveal/plainText';
import { ALSO_VALID_NOT_RAISED, ISSUE_OUTCOME, lineLabel } from '@/features/learning/drills/reveal/revealCopy';
import type { RankedIssue } from '@/features/learning/drills/reveal/revealPresentation';
import { eyebrow, jumpButton, panel, sectionHeading } from '@/features/learning/drills/reveal/revealStyle';
import { severityLabel, severityTone } from '@/features/learning/drills/severity';
import type { IssueResultKind, RevealedFindingDto } from '@/lib/bindings';

const OUTCOME_ICON: Record<IssueResultKind, LucideIcon> = {
  found: Check,
  half_found: CircleDot,
  missed: X,
};

export interface CreditedComment {
  number: number;
  text: string;
}

/** One key issue: what it is, what came of it, what it costs, how to say it, and the fix. */
export function IssueCard({ entry, credited, onShow, material = 'code' }: {
  entry: RankedIssue;
  credited: CreditedComment | null;
  onShow: (entry: RankedIssue) => void;
  material?: ReviewKind;
}) {
  const { issue, number } = entry;
  const titleId = useId();
  const notRaised = issue.alsoValid && issue.outcome === 'missed';
  const outcome = notRaised ? ALSO_VALID_NOT_RAISED : ISSUE_OUTCOME[issue.outcome];
  const OutcomeIcon = notRaised ? Minus : OUTCOME_ICON[issue.outcome];
  const where = lineLabel(issue.span, material);
  const place = material === 'passage' ? 'the explanation' : 'the code';
  return (
    <article aria-labelledby={titleId} className={`${panel} p-5 sm:p-6`}>
      <header className="flex flex-wrap items-start justify-between gap-x-4 gap-y-2">
        <div className="min-w-0">
          <p className={eyebrow}>Issue {number}</p>
          <h3 id={titleId} className="mt-0.5 font-serif text-lg leading-snug text-text-primary">{issue.patternName}</h3>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Badge variant="outline" className={severityTone(issue.severity)}>{severityLabel(issue.severity)}</Badge>
          <Badge variant="outline" className={`gap-1 ${outcome.tone}`}>
            <OutcomeIcon size={12} strokeWidth={2.5} aria-hidden="true" />
            {outcome.label}
          </Badge>
        </div>
      </header>
      <button
        type="button"
        onClick={() => onShow(entry)}
        aria-label={`Show ${where} in ${place}`}
        className={`${jumpButton} mt-2 font-mono text-xs`}
      >
        <FileCode size={13} aria-hidden="true" className="shrink-0" />
        {where}
      </button>
      {issue.reason && <p className="mt-3 text-sm leading-6 text-text-secondary">{issue.reason}</p>}
      {credited && (
        <button
          type="button"
          onClick={() => onShow(entry)}
          title={material === 'passage' ? 'Show this issue’s sentences in the explanation' : 'Show this issue’s lines in the code'}
          className={`${jumpButton} mt-2 text-sm`}
        >
          <FileCode size={13} aria-hidden="true" className="shrink-0" />
          <span>Credited to your comment {credited.number}{credited.text && `: “${credited.text}”`}</span>
        </button>
      )}
      <dl className="mt-4 grid gap-4 sm:grid-cols-2">
        <div>
          <dt className={eyebrow}>What happens</dt>
          <dd className="mt-1 text-sm leading-6 text-text-primary">{issue.consequence}</dd>
        </div>
        <div>
          <dt className={eyebrow}>How to say it</dt>
          <dd className="mt-1 border-l-2 border-accent/40 pl-3 text-sm leading-6 text-text-primary">{issue.explanation}</dd>
        </div>
      </dl>
      {material === 'passage' && issue.sourceQuote && (
        <div className="mt-4">
          <h4 className={eyebrow}>What the source says</h4>
          <blockquote className="mt-1 border-l-2 border-warning-fg/50 pl-3 text-sm leading-6 text-text-primary">{plainText(issue.sourceQuote)}</blockquote>
        </div>
      )}
      {issue.lesson && issue.lesson.sentences.length > 0 && (
        <div className="mt-4">
          <h4 className={eyebrow}>From the lesson</h4>
          <ul className="mt-1 space-y-3">
            {issue.lesson.sentences.map((sentence, index) => {
              const code = fencedCode(sentence.text);
              return (
                <li key={index}>
                  {code === null
                    ? <blockquote className="whitespace-pre-wrap border-l-2 border-warning-fg/50 pl-3 text-sm leading-6 text-text-primary">{plainText(sentence.text)}</blockquote>
                    : <pre className="overflow-x-auto rounded-lg bg-surface-raised p-3 font-mono text-xs leading-5 text-text-primary">{code}</pre>}
                  <p className="mt-1 text-xs text-text-muted">{sentence.source === null ? 'The lesson’s own teaching' : `From ${sentence.source}`}</p>
                </li>
              );
            })}
          </ul>
        </div>
      )}
      <div className="mt-4">
        <h4 className={eyebrow}>The fix</h4>
        <div className="mt-1.5"><FixDiff fix={issue.fix} prose={material === 'passage'} /></div>
      </div>
    </article>
  );
}

function IssueCards({ ranked, findings, onShow, material }: {
  ranked: readonly RankedIssue[];
  findings: readonly RevealedFindingDto[];
  onShow: (entry: RankedIssue) => void;
  material: ReviewKind;
}) {
  return (
    <ol className="space-y-4">
      {ranked.map((entry) => {
        const credit = entry.issue.finding;
        const comment: RevealedFindingDto | undefined = credit === null ? undefined : findings[credit];
        return (
          <li key={entry.issue.id}>
            <IssueCard
              entry={entry}
              credited={credit === null ? null : { number: credit + 1, text: comment?.text ?? '' }}
              onShow={onShow}
              material={material}
            />
          </li>
        );
      })}
    </ol>
  );
}

/**
 * Every planted issue, most severe first, then the also-valid ones a blind review raised, each numbered as the
 * learner's comments refer to it.
 */
export function IssueList({ ranked, findings, onShow, material = 'code' }: {
  ranked: readonly RankedIssue[];
  findings: readonly RevealedFindingDto[];
  onShow: (entry: RankedIssue) => void;
  material?: ReviewKind;
}) {
  const headingId = useId();
  const alsoValidId = useId();
  const planted = ranked.filter((entry) => !entry.issue.alsoValid);
  const alsoValid = ranked.filter((entry) => entry.issue.alsoValid);
  return (
    <>
      <section aria-labelledby={headingId} className="space-y-3">
        <h2 id={headingId} className={sectionHeading}>Issues, most severe first</h2>
        <IssueCards ranked={planted} findings={findings} onShow={onShow} material={material} />
      </section>
      {alsoValid.length > 0 && (
        <section aria-labelledby={alsoValidId} className="space-y-3">
          <div>
            <h2 id={alsoValidId} className={sectionHeading}>Also valid</h2>
            <p className="mt-1 text-sm text-text-secondary">
              A blind review raised these too. They were not planted, so raising one counts for you and leaving one costs nothing.
            </p>
          </div>
          <IssueCards ranked={alsoValid} findings={findings} onShow={onShow} material={material} />
        </section>
      )}
    </>
  );
}
