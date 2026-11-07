import { useMemo, useRef, useState } from 'react';

import { ArrowRight } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { kindOf } from '@/features/learning/drills/drillMaterial';
import { CodeContrast, type ContrastFocus } from '@/features/learning/drills/reveal/CodeContrast';
import { FindingList } from '@/features/learning/drills/reveal/FindingList';
import { FixResults } from '@/features/learning/drills/reveal/FixResults';
import { IssueList } from '@/features/learning/drills/reveal/IssueCard';
import { PassageContrast } from '@/features/learning/drills/reveal/PassageContrast';
import { PatternList } from '@/features/learning/drills/reveal/PatternList';
import { lineLabel } from '@/features/learning/drills/reveal/revealCopy';
import { cleanLinesOf, contrastPaths, rankIssues, type RankedIssue } from '@/features/learning/drills/reveal/revealPresentation';
import { RevealSummary } from '@/features/learning/drills/reveal/RevealSummary';
import { UngradedReview } from '@/features/learning/drills/reveal/UngradedReview';
import type { CodeReviewResultDto, CodeReviewRevealDto } from '@/lib/bindings';

export interface DrillRevealProps {
  result: CodeReviewResultDto;
  onNextDrill: () => void;
  onRegrade: () => void;
  regrading: boolean;
}

function scrollBehavior(): ScrollBehavior {
  const reduced = typeof window.matchMedia === 'function' && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  return reduced ? 'auto' : 'smooth';
}

function GradedReveal({ result, reveal, onNextDrill }: {
  result: CodeReviewResultDto;
  reveal: CodeReviewRevealDto;
  onNextDrill: () => void;
}) {
  const ranked = useMemo(() => rankIssues(reveal.issues), [reveal.issues]);
  const paths = useMemo(() => contrastPaths(result.drill.files, reveal.cleanFiles), [result.drill.files, reveal.cleanFiles]);
  const [path, setPath] = useState(paths[0] ?? '');
  const [focus, setFocus] = useState<ContrastFocus | null>(null);
  const kind = kindOf(result.drill);
  const contrastRef = useRef<HTMLDivElement>(null);

  const show = (entry: RankedIssue) => {
    const { span } = entry.issue;
    setPath(span.path);
    setFocus((current) => ({
      path: span.path,
      label: lineLabel(span, kind),
      quote: entry.issue.sourceQuote,
      reviewed: { startLine: span.start, endLine: span.end },
      clean: cleanLinesOf(entry.issue, reveal.cleanFiles),
      nonce: (current?.nonce ?? 0) + 1,
    }));
    contrastRef.current?.scrollIntoView?.({ block: 'start', behavior: scrollBehavior() });
  };

  return (
    <div className="mx-auto flex w-full max-w-6xl flex-col gap-6">
      <RevealSummary result={result} reveal={reveal} />
      <IssueList ranked={ranked} findings={reveal.findings} onShow={show} material={kind} />
      <FindingList findings={reveal.findings} ranked={ranked} material={kind} />
      {reveal.fix && reveal.fix.length > 0 && <FixResults fixes={reveal.fix} ranked={ranked} material={kind} />}
      <div ref={contrastRef} className="scroll-mt-4">
        {result.drill.material.kind === 'passage' ? (
          <PassageContrast
            reviewed={result.drill.files[0]}
            clean={reveal.cleanFiles[0]}
            ranked={ranked}
            cleanFiles={reveal.cleanFiles}
            source={reveal.source}
            focus={focus}
          />
        ) : (
          <CodeContrast
            language={result.drill.material.language}
            reviewed={result.drill.files}
            clean={reveal.cleanFiles}
            paths={paths}
            path={path}
            onPathChange={setPath}
            focus={focus}
          />
        )}
      </div>
      <PatternList ratings={reveal.ratings} ranked={ranked} />
      <div className="flex justify-end">
        <Button type="button" size="lg" onClick={onNextDrill}>
          Next drill
          <ArrowRight aria-hidden="true" />
        </Button>
      </div>
    </div>
  );
}

/**
 * What a drill's learner reads after handing in: every issue of the key with what came of it, their own comments
 * and fix, and what they reviewed beside the same without the issues; for an explanation, what the source says too.
 */
export function DrillReveal({ result, onNextDrill, onRegrade, regrading }: DrillRevealProps) {
  if (result.status === 'graded' && result.reveal) {
    return <GradedReveal key={result.attemptId} result={result} reveal={result.reveal} onNextDrill={onNextDrill} />;
  }
  if (result.status === 'couldnt_grade') {
    return <UngradedReview key={result.attemptId} result={result} onRegrade={onRegrade} onNextDrill={onNextDrill} regrading={regrading} />;
  }
  return (
    <p role="status" className="mx-auto w-full max-w-6xl text-sm text-text-secondary">
      {result.status === 'submitted' ? 'Your review is being graded.' : 'There is nothing to show for this review yet.'}
    </p>
  );
}
