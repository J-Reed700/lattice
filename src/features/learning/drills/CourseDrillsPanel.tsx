import { ArrowLeft } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

import { DrillBuilding } from './DrillBuilding';
import { DrillChoose } from './DrillChoose';
import { DrillFailure } from './DrillFailure';
import { DrillGrading } from './DrillGrading';
import { drillMeta } from './drillMaterial';
import { DrillReview } from './DrillReview';
import { DrillReveal } from './reveal/DrillReveal';
import { useCodeReviewDrill } from './useCodeReviewDrill';

/** A course's review drills, from the choice of a lesson to the reveal of what was planted in it. */
export function CourseDrillsPanel({ programId }: { programId: string }) {
  const drill = useCodeReviewDrill(programId);
  const { stage, failure, result, session } = drill;
  const back = (
    <Button type="button" variant="ghost" className="-ml-3 gap-2" onClick={drill.toChoices}>
      <ArrowLeft className="h-4 w-4" aria-hidden="true" />
      Back to drill setup
    </Button>
  );

  if ((stage === 'reviewing' || stage === 'submitting') && session) {
    return (
      <div className="h-[calc(100dvh-8rem)] min-h-[30rem]">
        <DrillReview drill={drill} back={back} />
      </div>
    );
  }

  let content;
  if (stage === 'choose') {
    content = (
      <DrillChoose
        programId={programId}
        options={drill.options}
        onChange={drill.setOptions}
        onStart={(patch) => drill.start(patch)}
        onOpenAttempt={drill.restore}
      />
    );
  } else if (stage === 'starting') {
    content = failure
      ? (
        <DrillFailure failure={failure} onRetry={drill.retry}>
          <Button type="button" variant="ghost" onClick={drill.toChoices}>Back to the choices</Button>
        </DrillFailure>
      )
      : <DrillBuilding building={drill.building} onStop={drill.stopWaiting} />;
  } else if (stage === 'loading' || !result) {
    content = failure
      ? (
        <DrillFailure failure={failure} onRetry={drill.retry}>
          <Button type="button" variant="ghost" onClick={drill.toChoices}>Start a new drill</Button>
        </DrillFailure>
      )
      : <p role="status" className="text-sm text-text-secondary">Opening your review…</p>;
  } else if (stage === 'grading') {
    content = (
      <>
        <h2 className="font-serif text-2xl text-text-primary">{result.drill.title}</h2>
        <p className="mt-1 text-sm text-text-secondary">{drillMeta(result.drill, result.mode)}</p>
        {failure && <div className="my-4"><DrillFailure failure={failure} onRetry={drill.retry} /></div>}
        <DrillGrading result={result} message={drill.gradingMessage} regrading={drill.regrading} onRegrade={drill.regrade} />
      </>
    );
  } else {
    content = (
      <>
        {failure && <div className="mb-4"><DrillFailure failure={failure} onRetry={drill.retry} /></div>}
        <DrillReveal result={result} onNextDrill={drill.nextDrill} onRegrade={drill.regrade} regrading={drill.regrading} />
      </>
    );
  }

  return (
    <div className={cn('pb-8', stage === 'revealed' ? 'max-w-6xl' : 'max-w-[800px]')}>
      {stage !== 'choose' && <div className="mb-5">{back}</div>}
      {content}
    </div>
  );
}
